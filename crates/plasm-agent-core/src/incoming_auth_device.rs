//! RFC 8628-style device authorization for **incoming** (platform) auth — used by the `plasm` CLI.
//!
//! - `POST /v1/incoming-auth/device/start` — public; returns `device_code`, `user_code`, `verification_uri`.
//! - `POST /v1/incoming-auth/device/poll` — public; CLI polls with `device_code`.
//! - `POST /internal/incoming-auth/v1/device/complete` — control-plane; Phoenix after GitHub sign-in.

use axum::extract::Extension;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

use crate::control_plane_http::control_plane_headers_authorized;
use crate::incoming_auth::IncomingAuthVerifier;
use crate::server_state::PlasmHostState;

const DEFAULT_DEVICE_TTL_SECS: u64 = 900;
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

#[derive(Clone)]
pub struct IncomingAuthDeviceStore {
    inner: Arc<RwLock<HashMap<String, DeviceSession>>>,
    by_user_code: Arc<RwLock<HashMap<String, String>>>,
}

#[derive(Clone)]
struct DeviceSession {
    user_code: String,
    expires_at: Instant,
    poll_interval_secs: u64,
    status: DeviceStatus,
}

#[derive(Clone)]
enum DeviceStatus {
    Pending,
    Approved { access_token: String },
    Expired,
}

impl IncomingAuthDeviceStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            by_user_code: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn purge_expired(&self) {
        let now = Instant::now();
        let mut inner = self.inner.write().await;
        let expired: Vec<String> = inner
            .iter()
            .filter(|(_, s)| s.expires_at <= now || matches!(s.status, DeviceStatus::Expired))
            .map(|(k, _)| k.clone())
            .collect();
        for code in expired {
            if let Some(sess) = inner.remove(&code) {
                let mut idx = self.by_user_code.write().await;
                idx.remove(&sess.user_code);
            }
        }
    }
}

fn public_web_origin() -> String {
    std::env::var("PLASM_PUBLIC_WEB_ORIGIN")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://platform.plasm.tools".to_string())
}

fn mint_user_code() -> String {
    let raw = uuid::Uuid::new_v4().to_string().replace('-', "");
    let s = raw[..8].to_uppercase();
    format!("{}-{}", &s[..4], &s[4..8])
}

fn mint_device_code() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn jwt_ttl_secs() -> u64 {
    std::env::var("PLASM_INCOMING_AUTH_JWT_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(30 * 86_400)
}

/// Mint an HS256 incoming JWT compatible with [`IncomingAuthVerifier`].
pub fn mint_incoming_access_token(
    verifier: &IncomingAuthVerifier,
    sub: &str,
    tenant_id: &str,
) -> Result<String, String> {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    let config = verifier.config();
    let secret = config
        .jwt_secret
        .as_deref()
        .ok_or_else(|| "JWT minting not configured (PLASM_AUTH_JWT_SECRET)".to_string())?;

    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        + jwt_ttl_secs();

    let mut claims = json!({
        "sub": sub,
        "tenant_id": tenant_id,
        "exp": exp,
    });
    if let Some(ref iss) = config.jwt_issuer {
        claims["iss"] = json!(iss);
    }
    if let Some(ref aud) = config.jwt_audience {
        claims["aud"] = json!(aud);
    }

    encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct DeviceStartBody {
    #[serde(default)]
    _client_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeviceStartResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}

async fn device_start_handler(
    Extension(st): Extension<PlasmHostState>,
    Json(_body): Json<DeviceStartBody>,
) -> Result<Json<DeviceStartResponse>, (StatusCode, Json<serde_json::Value>)> {
    let Some(verifier) = st.incoming_auth.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "incoming_auth_disabled",
                "message": "incoming auth is not configured on this server",
            })),
        ));
    };
    if !verifier.config().has_jwt() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "jwt_not_configured",
                "message": "PLASM_AUTH_JWT_SECRET is required for CLI device login",
            })),
        ));
    }

    let store = st.incoming_auth_device();
    store.purge_expired().await;

    let device_code = mint_device_code();
    let user_code = mint_user_code();
    let ttl = DEFAULT_DEVICE_TTL_SECS;
    let interval = DEFAULT_POLL_INTERVAL_SECS;
    let web = public_web_origin();
    let verification_uri = format!("{web}/login?user_code={user_code}&client=plasm-cli");
    let verification_uri_complete = verification_uri.clone();

    {
        let mut inner = store.inner.write().await;
        inner.insert(
            device_code.clone(),
            DeviceSession {
                user_code: user_code.clone(),
                expires_at: Instant::now() + Duration::from_secs(ttl),
                poll_interval_secs: interval,
                status: DeviceStatus::Pending,
            },
        );
    }
    {
        let mut idx = store.by_user_code.write().await;
        idx.insert(user_code.clone(), device_code.clone());
    }

    Ok(Json(DeviceStartResponse {
        device_code,
        user_code,
        verification_uri,
        verification_uri_complete,
        expires_in: ttl,
        interval,
    }))
}

#[derive(Debug, Deserialize)]
struct DevicePollBody {
    device_code: String,
}

#[derive(Debug, Serialize)]
struct DevicePollSuccess {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
}

#[derive(Debug, Serialize)]
struct DevicePollPending {
    error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<u64>,
}

async fn device_poll_handler(
    Extension(st): Extension<PlasmHostState>,
    Json(body): Json<DevicePollBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let device_code = body.device_code.trim();
    if device_code.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_request", "message": "device_code is required" })),
        ));
    }

    let store = st.incoming_auth_device();
    store.purge_expired().await;

    let mut inner = store.inner.write().await;
    let Some(sess) = inner.get_mut(device_code) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "expired_token", "message": "unknown or expired device_code" })),
        ));
    };
    if sess.expires_at <= Instant::now() {
        sess.status = DeviceStatus::Expired;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "expired_token" })),
        ));
    }

    match &sess.status {
        DeviceStatus::Pending => Ok(Json(serde_json::to_value(DevicePollPending {
            error: "authorization_pending",
            interval: Some(sess.poll_interval_secs),
        }).expect("serialize"))),
        DeviceStatus::Expired => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "expired_token" })),
        )),
        DeviceStatus::Approved { access_token } => {
            let resp = DevicePollSuccess {
                access_token: access_token.clone(),
                token_type: "Bearer",
                expires_in: jwt_ttl_secs(),
            };
            Ok(Json(serde_json::to_value(resp).expect("serialize")))
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCompleteBody {
    user_code: String,
    subject: String,
    #[serde(default)]
    github_login: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeviceCompleteResponse {
    ok: bool,
}

async fn device_complete_handler(
    Extension(st): Extension<PlasmHostState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<DeviceCompleteBody>,
) -> Result<Json<DeviceCompleteResponse>, StatusCode> {
    if !control_plane_headers_authorized(&headers, "incoming-auth device complete") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let user_code = body.user_code.trim().to_uppercase();
    let subject = body.subject.trim();
    if user_code.is_empty() || subject.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let Some(verifier) = st.incoming_auth.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let Some(tenant_store) = st.tenant_binding() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let store = st.incoming_auth_device();
    let device_code = {
        let idx = store.by_user_code.read().await;
        idx.get(&user_code).cloned()
    };
    let Some(device_code) = device_code else {
        return Err(StatusCode::NOT_FOUND);
    };

    let login = body
        .github_login
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let row = tenant_store
        .resolve_or_insert(subject, login)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let token = mint_incoming_access_token(verifier, subject, &row.tenant_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut inner = store.inner.write().await;
    let Some(sess) = inner.get_mut(&device_code) else {
        return Err(StatusCode::NOT_FOUND);
    };
    if sess.expires_at <= Instant::now() {
        return Err(StatusCode::BAD_REQUEST);
    }
    sess.status = DeviceStatus::Approved {
        access_token: token,
    };

    Ok(Json(DeviceCompleteResponse { ok: true }))
}

/// Public device authorization routes (no incoming-auth middleware).
pub fn incoming_auth_device_public_routes() -> Router {
    Router::new()
        .route(
            "/v1/incoming-auth/device/start",
            post(device_start_handler),
        )
        .route("/v1/incoming-auth/device/poll", post(device_poll_handler))
}

/// Control-plane completion after browser GitHub sign-in.
pub fn incoming_auth_device_internal_routes() -> Router {
    Router::new().route(
        "/internal/incoming-auth/v1/device/complete",
        post(device_complete_handler),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incoming_auth::{IncomingAuthConfig, IncomingAuthMode, IncomingAuthVerifier};

    #[test]
    fn mint_incoming_access_token_round_trip() {
        let config = IncomingAuthConfig {
            mode: IncomingAuthMode::Off,
            jwt_secret: Some("unit-test-secret-long-enough".into()),
            jwt_issuer: None,
            jwt_audience: None,
            api_keys_file: None,
        };
        let v = IncomingAuthVerifier::new(config).unwrap();
        let tok = mint_incoming_access_token(&v, "github:1", "tenant-a").unwrap();
        let p = v.verify_bearer_token(&tok).unwrap();
        assert_eq!(p.tenant_id, "tenant-a");
        assert_eq!(p.subject, "github:1");
    }
}
