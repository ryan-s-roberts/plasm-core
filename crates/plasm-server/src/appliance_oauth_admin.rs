//! In-process outbound OAuth provider administration (DB + KV + catalog refresh), shared by TUI and CLI.

use std::sync::Arc;
use std::time::Duration;

use auth_framework::storage::AuthStorage;
use plasm_agent_core::mcp_config_repository::McpConfigRepository;
use plasm_agent_core::oauth_binding_kv::{oauth_binding_kv_key, write_oauth_binding_pointer};
use plasm_agent_core::oauth_link_catalog::OauthLinkCatalog;
use plasm_agent_core::oauth_provider_model::RuntimeOauthProviderMeta;
use plasm_agent_core::oauth_provider_repository;
use plasm_agent_core::oauth_runtime_source::{
    apply_runtime_source_to_catalog, PostgresOauthRuntimeProviderSource,
};
use plasm_runtime::{
    build_oauth_token_http_client, parse_outbound_oauth_kv_v1, poll_oauth_device_token_once,
    request_oauth_device_authorization, OAuthDeviceTokenPoll, OutboundOAuthKvV1,
};

/// Fixed KV location for this catalog `entry_id` (matches `plasm-server oauth provider upsert`).
pub fn appliance_oauth_client_secret_kv_key(entry_id: &str) -> Result<String, String> {
    const PREFIX: &str = "plasm:oauth_app:v1:";
    let e = entry_id.trim();
    if e.is_empty() {
        return Err("entry_id must be non-empty".into());
    }
    let s = format!("{PREFIX}{e}");
    if s.len() > 255 {
        return Err(format!(
            "OAuth client secret storage key exceeds 255 chars (len={}); use a shorter entry_id",
            s.len()
        ));
    }
    Ok(s)
}

/// Disable a provider row in Postgres (when configured) and refresh the in-memory catalog.
pub async fn appliance_oauth_provider_disable(
    repo: Option<&McpConfigRepository>,
    catalog: &OauthLinkCatalog,
    entry_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let entry_id = entry_id.trim();
    if entry_id.is_empty() {
        return Err("entry_id required".into());
    }
    if let Some(r) = repo {
        let n = oauth_provider_repository::set_oauth_provider_enabled(r.pool(), entry_id, false)
            .await?;
        if n == 0 {
            catalog.remove_runtime(entry_id).await;
        }
        let src = PostgresOauthRuntimeProviderSource::new(r.pool().clone());
        apply_runtime_source_to_catalog(&src, catalog).await?;
    } else {
        catalog.remove_runtime(entry_id).await;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ApplianceOauthUpsert {
    pub entry_id: String,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: String,
    pub device_authorization_endpoint: Option<String>,
    pub default_scopes: Vec<String>,
    pub client_id: String,
    pub client_secret_key: String,
    pub client_secret_value: Option<String>,
    pub enabled: bool,
}

pub async fn appliance_oauth_upsert_provider(
    repo: Option<&McpConfigRepository>,
    catalog: &OauthLinkCatalog,
    storage: &Arc<dyn AuthStorage>,
    u: ApplianceOauthUpsert,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let entry_id = u.entry_id.trim();
    if entry_id.is_empty() {
        return Err("entry_id required".into());
    }
    let token_ep = u.token_endpoint.trim();
    if token_ep.is_empty() {
        return Err("token_endpoint required".into());
    }

    RuntimeOauthProviderMeta::try_from_parts(
        u.authorization_endpoint.as_deref(),
        token_ep,
        u.device_authorization_endpoint.as_deref(),
        u.default_scopes.clone(),
        u.client_id.trim(),
        u.client_secret_key.trim(),
    )
    .map_err(|e| format!("invalid provider metadata: {e}"))?;

    if let Some(secret) = u
        .client_secret_value
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        storage
            .store_kv(u.client_secret_key.trim(), secret.as_bytes(), None)
            .await
            .map_err(|e| format!("KV client secret write failed: {e}"))?;
    }

    if u.enabled {
        if let Some(r) = repo {
            oauth_provider_repository::upsert_oauth_provider_app(
                r.pool(),
                oauth_provider_repository::UpsertOauthProviderParams {
                    entry_id,
                    authorization_endpoint: u.authorization_endpoint.as_deref(),
                    token_endpoint: token_ep,
                    device_authorization_endpoint: u.device_authorization_endpoint.as_deref(),
                    client_id: u.client_id.trim(),
                    client_secret_key: u.client_secret_key.trim(),
                    enabled: true,
                },
            )
            .await?;
            let src = PostgresOauthRuntimeProviderSource::new(r.pool().clone());
            apply_runtime_source_to_catalog(&src, catalog).await?;
        } else {
            let meta = RuntimeOauthProviderMeta::try_from_parts(
                u.authorization_endpoint.as_deref(),
                token_ep,
                u.device_authorization_endpoint.as_deref(),
                u.default_scopes.clone(),
                u.client_id.trim(),
                u.client_secret_key.trim(),
            )
            .map_err(|e| format!("invalid provider metadata: {e}"))?;
            catalog.upsert_runtime(entry_id.to_string(), meta).await;
        }
    } else if let Some(r) = repo {
        let n = oauth_provider_repository::set_oauth_provider_enabled(r.pool(), entry_id, false)
            .await?;
        if n == 0 {
            catalog.remove_runtime(entry_id).await;
        }
        let src = PostgresOauthRuntimeProviderSource::new(r.pool().clone());
        apply_runtime_source_to_catalog(&src, catalog).await?;
    } else {
        catalog.remove_runtime(entry_id).await;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthBindingStatus {
    pub hint: String,
    pub bound: bool,
}

pub async fn oauth_binding_status(
    storage: &Arc<dyn AuthStorage>,
    entry_id: &str,
) -> OAuthBindingStatus {
    let key = oauth_binding_kv_key(entry_id);
    let Ok(Some(raw)) = storage.get_kv(key.as_str()).await else {
        return OAuthBindingStatus {
            hint: "no binding".into(),
            bound: false,
        };
    };
    let Ok(ptr) = serde_json::from_slice::<serde_json::Value>(&raw) else {
        return OAuthBindingStatus {
            hint: "binding corrupt".into(),
            bound: false,
        };
    };
    let Some(hkv) = ptr.get("hosted_kv_key").and_then(|v| v.as_str()) else {
        return OAuthBindingStatus {
            hint: "binding incomplete".into(),
            bound: false,
        };
    };
    let Ok(Some(tok)) = storage.get_kv(hkv).await else {
        return OAuthBindingStatus {
            hint: "token missing".into(),
            bound: false,
        };
    };
    let utf8 = String::from_utf8_lossy(&tok);
    match parse_outbound_oauth_kv_v1(utf8.as_ref()) {
        Ok(env) => OAuthBindingStatus {
            hint: format!(
                "kv ok · exp {:?}",
                env.expires_at_unix
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| "?".into())
            ),
            bound: true,
        },
        Err(_) => OAuthBindingStatus {
            hint: "kv present".into(),
            bound: true,
        },
    }
}

#[derive(Debug, Clone)]
pub struct DeviceBindPrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in_secs: u64,
    pub poll_interval_secs: u64,
}

#[derive(Debug, Clone)]
pub struct DeviceBindOutcome {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in_secs: u64,
    pub poll_interval_secs: u64,
}

/// Device authorization + poll until token stored (RFC 8628).
pub async fn appliance_oauth_device_bind(
    entry_id: &str,
    catalog: &OauthLinkCatalog,
    storage: &Arc<dyn AuthStorage>,
    scopes: &[String],
    max_wait: Duration,
    on_start: impl FnOnce(&DeviceBindPrompt),
) -> Result<DeviceBindOutcome, Box<dyn std::error::Error + Send + Sync>> {
    let entry_id = entry_id.trim();
    if entry_id.is_empty() {
        return Err("entry_id required".into());
    }

    let cfg = catalog
        .resolve_for_oauth_start(storage, entry_id)
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
            std::io::Error::other(e.refresh_failure_message()).into()
        })?;

    let device_url = cfg
        .device_authorization_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|s: &&str| !s.is_empty())
        .ok_or_else(|| {
            "device_authorization_endpoint missing for this entry (upsert provider with device URL)"
                .to_string()
        })?;

    let http_timeout = Duration::from_secs(30);
    let http = build_oauth_token_http_client(http_timeout).map_err(|e| e.to_string())?;

    let start = request_oauth_device_authorization(
        &http,
        device_url,
        cfg.client_id.trim(),
        Some(cfg.client_secret.as_str()),
        scopes,
        http_timeout,
    )
    .await
    .map_err(|e| e.to_string())?;

    let mut interval = Duration::from_secs(start.interval.unwrap_or(5).max(1));
    let prompt = DeviceBindPrompt {
        user_code: start.user_code.clone(),
        verification_uri: start.verification_uri.clone(),
        verification_uri_complete: start.verification_uri_complete.clone(),
        expires_in_secs: start.expires_in,
        poll_interval_secs: interval.as_secs(),
    };
    on_start(&prompt);
    let deadline = tokio::time::Instant::now() + max_wait;

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err("device authorization timed out".into());
        }

        match poll_oauth_device_token_once(
            &http,
            cfg.token_endpoint.trim(),
            cfg.client_id.trim(),
            Some(cfg.client_secret.as_str()),
            start.device_code.trim(),
            http_timeout,
        )
        .await
        .map_err(|e| e.to_string())?
        {
            OAuthDeviceTokenPoll::Success(token_json) => {
                let envelope =
                    OutboundOAuthKvV1::from_token_json_for_entry(entry_id.to_string(), &token_json)
                        .map_err(|e| e.to_string())?;
                let hosted_kv_key = format!("plasm:outbound:v1:{}", uuid::Uuid::new_v4());
                let envelope_bytes = serde_json::to_vec(&envelope)?;
                storage
                    .store_kv(&hosted_kv_key, &envelope_bytes, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let _ = write_oauth_binding_pointer(storage, entry_id, &hosted_kv_key).await;
                return Ok(DeviceBindOutcome {
                    user_code: prompt.user_code.clone(),
                    verification_uri: prompt.verification_uri.clone(),
                    verification_uri_complete: prompt.verification_uri_complete.clone(),
                    expires_in_secs: prompt.expires_in_secs,
                    poll_interval_secs: interval.as_secs(),
                });
            }
            OAuthDeviceTokenPoll::AuthorizationPending => {
                tokio::time::sleep(interval).await;
            }
            OAuthDeviceTokenPoll::SlowDown { interval_secs } => {
                interval = Duration::from_secs(interval_secs.max(1));
                tokio::time::sleep(interval).await;
            }
            OAuthDeviceTokenPoll::OAuthError { error, .. } => {
                return Err(format!("device token error: {error}").into());
            }
        }
    }
}
