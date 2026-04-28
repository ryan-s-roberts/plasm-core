//! Optional callback to Phoenix when hosted OAuth refresh fails with `invalid_grant`.

use std::time::Duration;

use plasm_runtime::{RuntimeError, runtime_error_is_oauth_invalid_grant};

use crate::control_plane_http::{
    X_PLASM_CONTROL_PLANE_SECRET, control_plane_secret_from_env_strict,
};

#[derive(Clone)]
pub(crate) struct WebConnectedAccountNotifyConfig {
    base_url: String,
    secret: String,
}

impl WebConnectedAccountNotifyConfig {
    pub(crate) fn from_env() -> Option<Self> {
        let base = std::env::var("PLASM_WEB_CONNECTED_ACCOUNT_CALLBACK_BASE_URL")
            .ok()?
            .trim()
            .trim_end_matches('/')
            .to_string();
        if base.is_empty() {
            return None;
        }
        let secret = control_plane_secret_from_env_strict()?;
        Some(Self {
            base_url: base,
            secret,
        })
    }

    pub(crate) fn spawn_notify_if_invalid_grant(&self, hosted_kv_key: String, err: &RuntimeError) {
        if !runtime_error_is_oauth_invalid_grant(err) {
            return;
        }
        let msg = err.to_string();
        let url = format!(
            "{}/internal/outbound-connected-account/v1/needs-reconnect",
            self.base_url
        );
        let secret = self.secret.clone();
        let description = truncate_utf8_by_bytes(&msg, 900);
        tokio::spawn(async move {
            notify_post(url, secret, hosted_kv_key, description).await;
        });
    }
}

fn truncate_utf8_by_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

async fn notify_post(url: String, secret: String, hosted_kv_key: String, description: String) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let body = serde_json::json!({
        "hosted_kv_key": hosted_kv_key,
        "oauth_error": "invalid_grant",
        "oauth_error_description": description,
    });
    match client
        .post(url)
        .header(X_PLASM_CONTROL_PLANE_SECRET, secret)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => {
            tracing::debug!(
                target: "plasm_agent::web_connected_account_notify",
                status = %resp.status(),
                "needs-reconnect callback non-success"
            );
        }
        Err(e) => {
            tracing::debug!(
                target: "plasm_agent::web_connected_account_notify",
                error = %e,
                "needs-reconnect callback transport error"
            );
        }
    }
}
