//! Validated outbound OAuth link **runtime** metadata: shared by HTTP upsert, Postgres pull, and catalog.
//! Invalid URLs and KV key shapes are rejected at construction / serde time.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// OAuth authorization or token URL; must parse as `http` or `https`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OAuthEndpointUrl(String);

/// Reference to auth-framework KV where the OAuth **client secret** is stored (not the user token).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OauthClientSecretKvRef(String);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MetaBuildError {
    #[error("OAuth endpoint URL is missing or invalid (expected http/https)")]
    BadEndpointUrl,
    #[error(
        "OAuth client secret KV key is invalid (expected plasm:oauth_app:v1:… or plasm:outbound:…)"
    )]
    BadSecretKeyRef,
    #[error("client_id must be non-empty")]
    EmptyClientId,
}

impl OAuthEndpointUrl {
    pub fn try_new(raw: &str) -> Result<Self, MetaBuildError> {
        let t = raw.trim();
        if t.is_empty() {
            return Err(MetaBuildError::BadEndpointUrl);
        }
        let u = reqwest::Url::parse(t).map_err(|_| MetaBuildError::BadEndpointUrl)?;
        if u.scheme() != "http" && u.scheme() != "https" {
            return Err(MetaBuildError::BadEndpointUrl);
        }
        Ok(Self(u.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for OAuthEndpointUrl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OAuthEndpointUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::try_new(&s).map_err(serde::de::Error::custom)
    }
}

impl OauthClientSecretKvRef {
    pub fn try_new(raw: &str) -> Result<Self, MetaBuildError> {
        let k = raw.trim();
        if k.is_empty() || k.len() > 255 {
            return Err(MetaBuildError::BadSecretKeyRef);
        }
        if !k.starts_with("plasm:oauth_app:v1:") && !k.starts_with("plasm:outbound:") {
            return Err(MetaBuildError::BadSecretKeyRef);
        }
        Ok(Self(k.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for OauthClientSecretKvRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OauthClientSecretKvRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::try_new(&s).map_err(serde::de::Error::custom)
    }
}

/// Runtime upsert: metadata only; user-facing secret fetched from KV using `client_secret_key` at OAuth start.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeOauthProviderMeta {
    pub authorization_endpoint: OAuthEndpointUrl,
    pub token_endpoint: OAuthEndpointUrl,
    #[serde(default)]
    pub default_scopes: Vec<String>,
    pub client_id: String,
    pub client_secret_key: OauthClientSecretKvRef,
}

impl RuntimeOauthProviderMeta {
    pub fn try_new(
        authorization_endpoint: &str,
        token_endpoint: &str,
        default_scopes: Vec<String>,
        client_id: &str,
        client_secret_key: &str,
    ) -> Result<Self, MetaBuildError> {
        let authorization_endpoint = OAuthEndpointUrl::try_new(authorization_endpoint)?;
        let token_endpoint = OAuthEndpointUrl::try_new(token_endpoint)?;
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(MetaBuildError::EmptyClientId);
        }
        let client_secret_key = OauthClientSecretKvRef::try_new(client_secret_key)?;
        Ok(Self {
            authorization_endpoint,
            token_endpoint,
            default_scopes,
            client_id: client_id.to_string(),
            client_secret_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_rejects_non_http_scheme() {
        assert_eq!(
            OAuthEndpointUrl::try_new("ftp://x/y"),
            Err(MetaBuildError::BadEndpointUrl)
        );
    }

    #[test]
    fn secret_key_requires_prefix() {
        assert_eq!(
            OauthClientSecretKvRef::try_new("other:secret"),
            Err(MetaBuildError::BadSecretKeyRef)
        );
        assert!(OauthClientSecretKvRef::try_new("plasm:outbound:x").is_ok());
    }
}
