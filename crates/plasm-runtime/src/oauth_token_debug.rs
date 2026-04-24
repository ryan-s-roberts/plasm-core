//! Redacted summaries of OAuth 2.0 token endpoint JSON for logging and tracing.
//! Never stores secret values — only key names, presence flags, and string lengths.

use serde::{Deserialize, Serialize};

/// Secret-safe snapshot of a token endpoint JSON body (RFC 6749 §5.1 / §5.2, OIDC token response).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenEndpointResponseSummary {
    /// Top-level JSON keys, sorted for stable logs.
    pub top_level_keys: Vec<String>,
    pub has_access_token: bool,
    /// Byte length of `access_token` when it is a non-empty JSON string; `None` if absent or wrong type.
    pub access_token_len: Option<usize>,
    pub has_refresh_token: bool,
    pub refresh_token_len: Option<usize>,
    pub has_id_token: bool,
    pub id_token_len: Option<usize>,
    pub token_type: Option<String>,
    pub scope: Option<String>,
    /// `expires_in` as a short display string (number or string from JSON).
    pub expires_in: Option<String>,
    pub rfc6749_error: Option<String>,
    pub rfc6749_error_description: Option<String>,
}

impl TokenEndpointResponseSummary {
    /// Build a redacted summary without copying bearer secrets.
    pub fn from_value(body: &serde_json::Value) -> Self {
        let Some(obj) = body.as_object() else {
            return Self::empty();
        };

        let mut top_level_keys: Vec<String> = obj.keys().cloned().collect();
        top_level_keys.sort();

        let access_token_len = obj
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(str::len);
        let has_access_token = access_token_len.map(|n| n > 0).unwrap_or(false);

        let refresh_token_len = obj
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(str::len);
        let has_refresh_token = refresh_token_len.map(|n| n > 0).unwrap_or(false);

        let id_token_len = obj.get("id_token").and_then(|v| v.as_str()).map(str::len);
        let has_id_token = id_token_len.map(|n| n > 0).unwrap_or(false);

        let token_type = obj
            .get("token_type")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let scope = obj
            .get("scope")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let expires_in = obj.get("expires_in").map(expires_in_display);

        let rfc6749_error = obj
            .get("error")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let rfc6749_error_description = obj
            .get("error_description")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        Self {
            top_level_keys,
            has_access_token,
            access_token_len: if access_token_len == Some(0) {
                None
            } else {
                access_token_len
            },
            has_refresh_token,
            refresh_token_len: if refresh_token_len == Some(0) {
                None
            } else {
                refresh_token_len
            },
            has_id_token,
            id_token_len: if id_token_len == Some(0) {
                None
            } else {
                id_token_len
            },
            token_type,
            scope,
            expires_in,
            rfc6749_error,
            rfc6749_error_description,
        }
    }

    fn empty() -> Self {
        Self {
            top_level_keys: vec![],
            has_access_token: false,
            access_token_len: None,
            has_refresh_token: false,
            refresh_token_len: None,
            has_id_token: false,
            id_token_len: None,
            token_type: None,
            scope: None,
            expires_in: None,
            rfc6749_error: None,
            rfc6749_error_description: None,
        }
    }
}

fn expires_in_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summary_normal_access_refresh_scope() {
        let body = json!({
            "access_token": "x".repeat(12),
            "refresh_token": "r".repeat(8),
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "https://www.googleapis.com/auth/gmail.readonly"
        });
        let s = TokenEndpointResponseSummary::from_value(&body);
        assert_eq!(
            s.top_level_keys,
            vec![
                "access_token",
                "expires_in",
                "refresh_token",
                "scope",
                "token_type"
            ]
        );
        assert!(s.has_access_token);
        assert_eq!(s.access_token_len, Some(12));
        assert!(s.has_refresh_token);
        assert_eq!(s.refresh_token_len, Some(8));
        assert!(!s.has_id_token);
        assert_eq!(s.token_type.as_deref(), Some("Bearer"));
        assert!(s.scope.unwrap().contains("gmail"));
        assert_eq!(s.expires_in.as_deref(), Some("3600"));
        assert!(s.rfc6749_error.is_none());
    }

    #[test]
    fn summary_oidc_id_token_only() {
        let body = json!({
            "id_token": "eyJ".to_string() + &"a".repeat(50),
            "token_type": "Bearer",
            "expires_in": "3600"
        });
        let s = TokenEndpointResponseSummary::from_value(&body);
        assert!(s.has_id_token);
        assert_eq!(s.id_token_len, Some(53));
        assert!(!s.has_access_token);
        assert_eq!(s.access_token_len, None);
    }

    #[test]
    fn summary_rfc6749_error() {
        let body = json!({
            "error": "invalid_grant",
            "error_description": "Token has been expired or revoked."
        });
        let s = TokenEndpointResponseSummary::from_value(&body);
        assert_eq!(s.rfc6749_error.as_deref(), Some("invalid_grant"));
        assert!(s
            .rfc6749_error_description
            .as_ref()
            .unwrap()
            .contains("revoked"));
        assert!(!s.has_access_token);
    }

    #[test]
    fn summary_non_object() {
        let s = TokenEndpointResponseSummary::from_value(&json!("nope"));
        assert!(s.top_level_keys.is_empty());
    }
}
