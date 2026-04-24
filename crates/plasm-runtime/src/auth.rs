//! Authentication provider abstraction.
//!
//! # Overview
//!
//! Authentication in Plasm is a two-layer design:
//!
//! 1. **`AuthScheme`** (declared in `domain.yaml`, lives in `plasm-core`) — pure data describing
//!    *what kind* of auth is needed and *which environment variable names* hold the secrets.
//!
//! 2. **`SecretProvider` / `AuthResolver`** (this module, lives in `plasm-runtime`) — async
//!    runtime layer that reads secrets and resolves them into concrete HTTP credentials
//!    (`ResolvedAuth`) ready to be injected into outbound requests.
//!
//! # Extension
//!
//! Swap `EnvSecretProvider` for any `impl SecretProvider` (Vault, AWS Secrets Manager, etc.)
//! by constructing `AuthResolver::new(scheme, Arc::new(my_provider))`.

use crate::hosted_oauth_kv::{
    build_oauth_token_http_client, post_oauth_token_form_json,
    resolve_hosted_bearer_default_no_refresh, HOSTED_OAUTH_EXPIRY_SKEW_SECS,
};
use crate::RuntimeError;
use futures_util::future::BoxFuture;
use plasm_core::AuthScheme;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ── SecretProvider trait ──────────────────────────────────────────────────────

/// Async provider for named secrets.
///
/// The default implementation reads from environment variables, but callers can
/// substitute any backend (Vault, AWS SSM, GCP Secret Manager, …) by implementing
/// this trait and passing it to [`AuthResolver`].
///
/// The return type is `BoxFuture` to keep the trait dyn-compatible so that
/// `Arc<dyn SecretProvider>` works across crate boundaries.
pub trait SecretProvider: Send + Sync {
    /// Retrieve the secret stored under `key` (typically an env-var name).
    ///
    /// Returns `None` if the secret is not present (unset env var, missing entry, …).
    fn get_secret<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<String>>;

    /// Resolve a Plasm-hosted credential from auth-framework `kv_store` (or equivalent).
    ///
    /// Default: always `None` (env-only deployments). `plasm-agent` overrides this when
    /// wiring [`AuthResolver`] for HTTP/MCP.
    fn get_hosted_secret<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            let _ = key;
            None
        })
    }

    /// Resolve a Bearer token from a `hosted_kv` key (JSON [`OutboundOAuthKvV1`](crate::hosted_oauth_kv::OutboundOAuthKvV1) envelope).
    ///
    /// Default: read via [`Self::get_hosted_secret`]. Expired v1 envelopes without agent-side
    /// refresh return [`RuntimeError::AuthenticationError`].
    fn resolve_hosted_bearer<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxFuture<'a, Result<String, RuntimeError>> {
        Box::pin(async move {
            let raw = self.get_hosted_secret(key).await.ok_or_else(|| {
                RuntimeError::AuthenticationError {
                    message: format!(
                        "Hosted credential '{key}' is not available (bearer_token). \
                         Store it via the control plane or check auth-framework storage."
                    ),
                }
            })?;
            resolve_hosted_bearer_default_no_refresh(&raw)
        })
    }
}

// ── EnvSecretProvider ─────────────────────────────────────────────────────────

/// [`SecretProvider`] that reads secrets from environment variables.
///
/// This is the default implementation. `key` is treated as an env-var name.
#[derive(Debug, Clone, Default)]
pub struct EnvSecretProvider;

impl SecretProvider for EnvSecretProvider {
    fn get_secret<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<String>> {
        let value = std::env::var(key).ok();
        Box::pin(async move { value })
    }
}

// ── ResolvedAuth ──────────────────────────────────────────────────────────────

/// Concrete credentials to inject into a single HTTP request.
///
/// Both fields may be populated simultaneously (unusual but not forbidden).
#[derive(Debug, Clone, Default)]
pub struct ResolvedAuth {
    /// HTTP headers to add (e.g. `Authorization: Bearer …`, `X-Api-Key: …`).
    pub headers: Vec<(String, String)>,
    /// Query parameters to append (e.g. `("apikey", "abc123")`).
    pub query_params: Vec<(String, String)>,
}

// ── CachedToken (OAuth2) ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    /// Wall-clock instant at which this token expires (with `HOSTED_OAUTH_EXPIRY_SKEW_SECS` margin).
    expires_at: Instant,
}

impl CachedToken {
    fn is_valid(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

// ── AuthResolver ──────────────────────────────────────────────────────────────

/// Combines an [`AuthScheme`] with a [`SecretProvider`] to produce [`ResolvedAuth`]
/// for each outbound HTTP request.
///
/// For [`AuthScheme::Oauth2ClientCredentials`], the resolved token is cached in a
/// `RwLock`-protected field so concurrent requests share the same token.
pub struct AuthResolver {
    scheme: AuthScheme,
    provider: Arc<dyn SecretProvider>,
    /// Cached OAuth2 access token (populated lazily, refreshed on expiry).
    token_cache: RwLock<Option<CachedToken>>,
}

impl AuthResolver {
    /// Create a new resolver for `scheme` that fetches secrets via `provider`.
    pub fn new(scheme: AuthScheme, provider: Arc<dyn SecretProvider>) -> Self {
        Self {
            scheme,
            provider,
            token_cache: RwLock::new(None),
        }
    }

    /// Create a resolver backed by the default [`EnvSecretProvider`].
    pub fn from_env(scheme: AuthScheme) -> Self {
        Self::new(scheme, Arc::new(EnvSecretProvider))
    }

    /// Resolve credentials for the next outbound request.
    ///
    /// - For static schemes (`ApiKeyHeader`, `ApiKeyQuery`, `BearerToken`): reads the
    ///   secret on every call (cheap env-var lookup).
    /// - For `Oauth2ClientCredentials`: returns the cached token if valid, otherwise
    ///   exchanges client credentials for a fresh token, caches it, then returns it.
    pub async fn resolve(&self) -> Result<ResolvedAuth, RuntimeError> {
        match &self.scheme {
            AuthScheme::None => Ok(ResolvedAuth {
                headers: vec![],
                query_params: vec![],
            }),
            AuthScheme::ApiKeyHeader {
                header,
                env,
                hosted_kv,
            } => {
                let value = self
                    .resolve_credential_slot(env.as_deref(), hosted_kv.as_deref(), "api_key_header")
                    .await?;
                Ok(ResolvedAuth {
                    headers: vec![(header.clone(), value)],
                    query_params: vec![],
                })
            }

            AuthScheme::ApiKeyQuery {
                param,
                env,
                hosted_kv,
            } => {
                let value = self
                    .resolve_credential_slot(env.as_deref(), hosted_kv.as_deref(), "api_key_query")
                    .await?;
                Ok(ResolvedAuth {
                    headers: vec![],
                    query_params: vec![(param.clone(), value)],
                })
            }

            AuthScheme::BearerToken { env, hosted_kv } => {
                let e = env.as_deref().map(str::trim).filter(|s| !s.is_empty());
                let h = hosted_kv
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let token = match (e, h) {
                    (_, Some(kv)) => self.provider.resolve_hosted_bearer(kv).await?,
                    (Some(name), None) => self.require_secret_trimmed(name).await?,
                    (None, None) => {
                        return Err(RuntimeError::AuthenticationError {
                            message: "Invalid auth schema: missing credential for bearer_token (expected env or hosted_kv)."
                                .to_string(),
                        });
                    }
                };
                Ok(ResolvedAuth {
                    headers: vec![("Authorization".to_string(), format!("Bearer {}", token))],
                    query_params: vec![],
                })
            }

            AuthScheme::Oauth2ClientCredentials {
                token_url,
                client_id_env,
                client_id_hosted_kv,
                client_secret_env,
                client_secret_hosted_kv,
                scopes,
            } => {
                self.resolve_oauth2(
                    token_url,
                    client_id_env.as_deref(),
                    client_id_hosted_kv.as_deref(),
                    client_secret_env.as_deref(),
                    client_secret_hosted_kv.as_deref(),
                    scopes,
                )
                .await
            }
        }
    }

    /// Invalidate any cached OAuth2 token (call this on receiving a 401).
    pub async fn invalidate_token(&self) {
        let mut guard = self.token_cache.write().await;
        *guard = None;
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Rejects whitespace-only values (avoids sending blank API keys).
    async fn require_secret_trimmed(&self, env_var: &str) -> Result<String, RuntimeError> {
        let raw = self.provider.get_secret(env_var).await.ok_or_else(|| {
            RuntimeError::AuthenticationError {
                message: format!(
                    "Required secret '{}' is not set. \
                     Set the environment variable before running plasm.",
                    env_var
                ),
            }
        })?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(RuntimeError::AuthenticationError {
                message: format!(
                    "Environment variable '{}' is set but empty or whitespace-only.",
                    env_var
                ),
            });
        }
        Ok(trimmed.to_string())
    }

    async fn resolve_credential_slot(
        &self,
        env: Option<&str>,
        hosted_kv: Option<&str>,
        context: &'static str,
    ) -> Result<String, RuntimeError> {
        let e = env.map(str::trim).filter(|s| !s.is_empty());
        let h = hosted_kv.map(str::trim).filter(|s| !s.is_empty());
        match (e, h) {
            (_, Some(kv)) => {
                let raw = self
                    .provider
                    .get_hosted_secret(kv)
                    .await
                    .ok_or_else(|| RuntimeError::AuthenticationError {
                        message: format!(
                            "Hosted credential '{kv}' is not available ({context}). \
                             Store it via the control plane or check auth-framework storage."
                        ),
                    })?;
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err(RuntimeError::AuthenticationError {
                        message: format!(
                            "Hosted credential '{kv}' is empty or whitespace-only ({context})."
                        ),
                    });
                }
                // OAuth link + outbound `plasm:outbound:v1:*` keys store JSON v1 envelopes; `api_key_*`
                // slots historically used raw `get_hosted_secret` and would send JSON as the header/query
                // value. Reuse bearer resolution so access tokens (and refresh) work for `api_key_header`
                // (e.g. Linear `Authorization`) as well as `bearer_token`.
                if trimmed.starts_with('{') {
                    return self.provider.resolve_hosted_bearer(kv).await;
                }
                Ok(trimmed.to_string())
            }
            (Some(name), None) => self.require_secret_trimmed(name).await,
            (None, None) => Err(RuntimeError::AuthenticationError {
                message: format!(
                    "Invalid auth schema: missing credential for {context} (expected env or hosted_kv)."
                ),
            }),
        }
    }

    async fn resolve_oauth2(
        &self,
        token_url: &str,
        client_id_env: Option<&str>,
        client_id_hosted_kv: Option<&str>,
        client_secret_env: Option<&str>,
        client_secret_hosted_kv: Option<&str>,
        scopes: &[String],
    ) -> Result<ResolvedAuth, RuntimeError> {
        // Fast path: return cached token if still valid.
        {
            let guard = self.token_cache.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.is_valid() {
                    return Ok(ResolvedAuth {
                        headers: vec![(
                            "Authorization".to_string(),
                            format!("Bearer {}", cached.access_token),
                        )],
                        query_params: vec![],
                    });
                }
            }
        }

        // Slow path: exchange client credentials for a new token.
        let client_id = self
            .resolve_credential_slot(client_id_env, client_id_hosted_kv, "oauth2_client_id")
            .await?;
        let client_secret = self
            .resolve_credential_slot(
                client_secret_env,
                client_secret_hosted_kv,
                "oauth2_client_secret",
            )
            .await?;

        let http = build_oauth_token_http_client(Duration::from_secs(15))?;

        let mut form = HashMap::new();
        form.insert("grant_type".into(), "client_credentials".into());
        form.insert("client_id".into(), client_id);
        form.insert("client_secret".into(), client_secret);
        if !scopes.is_empty() {
            form.insert("scope".into(), scopes.join(" "));
        }

        let body = post_oauth_token_form_json(
            &http,
            token_url,
            form,
            Duration::from_secs(15),
            "OAuth2 client credentials",
        )
        .await?;

        let access_token = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RuntimeError::AuthenticationError {
                message: "OAuth2 response missing 'access_token' field".to_string(),
            })?
            .to_string();

        let expires_in_secs = body
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600);

        // Safety margin aligned with hosted OAuth proactive refresh skew.
        let margin =
            Duration::from_secs(expires_in_secs.saturating_sub(HOSTED_OAUTH_EXPIRY_SKEW_SECS));
        let expires_at = Instant::now() + margin;

        let cached = CachedToken {
            access_token: access_token.clone(),
            expires_at,
        };

        {
            let mut guard = self.token_cache.write().await;
            *guard = Some(cached);
        }

        Ok(ResolvedAuth {
            headers: vec![(
                "Authorization".to_string(),
                format!("Bearer {}", access_token),
            )],
            query_params: vec![],
        })
    }
}

impl std::fmt::Debug for AuthResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthResolver")
            .field("scheme", &self.scheme)
            .finish_non_exhaustive()
    }
}
