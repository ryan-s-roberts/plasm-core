//! Outbound HTTP credential resolution: environment variables plus auth-framework `kv_store` (`hosted_kv`).

use std::sync::Arc;

use auth_framework::storage::AuthStorage;
use futures_util::future::BoxFuture;
use plasm_runtime::RuntimeError;
use plasm_runtime::auth::{EnvSecretProvider, SecretProvider};
use plasm_runtime::hosted_oauth_kv::{
    HOSTED_OAUTH_EXPIRY_SKEW_SECS, HostedBearerResolution, build_oauth_token_http_client,
    classify_hosted_bearer_utf8, oauth_refresh_token_request, usable_refresh_token,
};

use crate::oauth_link_catalog::OauthLinkCatalog;
use crate::web_connected_account_notify::WebConnectedAccountNotifyConfig;

/// Reads env vars via [`EnvSecretProvider`] and Plasm-hosted secrets via [`AuthStorage::get_kv`].
///
/// Resolves OAuth-linked `plasm:outbound:*` JSON envelopes and refreshes access tokens using
/// [`OauthLinkCatalog::resolve_for_oauth_start`] for client credentials.
#[derive(Clone)]
pub struct AgentOutboundSecretProvider {
    storage: Arc<dyn AuthStorage>,
    oauth_link_catalog: Arc<OauthLinkCatalog>,
    web_notify: Option<WebConnectedAccountNotifyConfig>,
}

impl AgentOutboundSecretProvider {
    pub fn new(storage: Arc<dyn AuthStorage>, oauth_link_catalog: Arc<OauthLinkCatalog>) -> Self {
        Self {
            storage,
            oauth_link_catalog,
            web_notify: WebConnectedAccountNotifyConfig::from_env(),
        }
    }
}

impl SecretProvider for AgentOutboundSecretProvider {
    fn get_secret<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<String>> {
        EnvSecretProvider.get_secret(key)
    }

    fn get_hosted_secret<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<String>> {
        let key = key.to_string();
        let storage = self.storage.clone();
        Box::pin(async move {
            match storage.get_kv(&key).await {
                Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!(
                        target: "plasm_agent::outbound_secret_provider",
                        key = %key,
                        error = %e,
                        "hosted_kv read failed"
                    );
                    None
                }
            }
        })
    }

    fn resolve_hosted_bearer<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxFuture<'a, Result<String, RuntimeError>> {
        let key = key.to_string();
        let storage = self.storage.clone();
        let catalog = self.oauth_link_catalog.clone();
        let web_notify = self.web_notify.clone();
        Box::pin(async move {
            let raw_bytes =
                storage
                    .get_kv(&key)
                    .await
                    .map_err(|e| RuntimeError::AuthenticationError {
                        message: format!("hosted_kv read failed: {e}"),
                    })?;
            let Some(bytes) = raw_bytes else {
                return Err(RuntimeError::AuthenticationError {
                    message: format!(
                        "Hosted credential '{key}' is not available (bearer_token). \
                         Store it via the control plane or check auth-framework storage."
                    ),
                });
            };
            let raw = String::from_utf8_lossy(&bytes);
            let trimmed = raw.trim();
            match classify_hosted_bearer_utf8(trimmed, HOSTED_OAUTH_EXPIRY_SKEW_SECS)? {
                HostedBearerResolution::Ready(t) => Ok(t),
                HostedBearerResolution::NeedsRefresh(mut env) => {
                    let refresh_tok = usable_refresh_token(&env).ok_or_else(|| {
                        RuntimeError::AuthenticationError {
                            message: "OAuth access token expired and no refresh_token is stored; re-link the account."
                                .to_string(),
                        }
                    })?;
                    let prov = match catalog
                        .resolve_for_oauth_start(&storage, env.entry_id.trim())
                        .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            return Err(RuntimeError::AuthenticationError {
                                message: e.refresh_failure_message(),
                            });
                        }
                    };
                    let http = build_oauth_token_http_client(std::time::Duration::from_secs(30))?;
                    let body = match oauth_refresh_token_request(
                        &http,
                        prov.token_endpoint.trim(),
                        prov.client_id.trim(),
                        prov.client_secret.trim(),
                        refresh_tok,
                    )
                    .await
                    {
                        Ok(b) => b,
                        Err(e) => {
                            if let Some(ref n) = web_notify {
                                n.spawn_notify_if_invalid_grant(key.clone(), &e);
                            }
                            return Err(e);
                        }
                    };
                    env.apply_token_response(&body)?;
                    let out = serde_json::to_vec(&env).map_err(|e| {
                        RuntimeError::AuthenticationError {
                            message: format!("serialize oauth envelope: {e}"),
                        }
                    })?;
                    storage.store_kv(&key, &out, None).await.map_err(|e| {
                        RuntimeError::AuthenticationError {
                            message: format!("store refreshed oauth credential: {e}"),
                        }
                    })?;
                    Ok(env.access_token)
                }
            }
        })
    }
}
