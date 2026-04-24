//! Validated path identifiers for [`crate::http_execute`] (`/execute/:prompt_hash/:session_id`).

use serde::Deserialize;
use serde::Deserializer;
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

const SHA256_HEX_LEN: usize = 64;
const SESSION_SIMPLE_HEX_LEN: usize = 32;

/// Lowercase SHA-256 digest of the rendered prompt, as 64 hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PromptHashHex(String);

/// `Uuid::simple()` form: 32 lowercase hex digits, no hyphens.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExecuteSessionId(String);

impl PromptHashHex {
    pub fn from_prompt_sha256(prompt: &str) -> Self {
        Self(hex::encode(Sha256::digest(prompt.as_bytes())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ExecuteSessionId {
    pub fn new_random() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PromptHashHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for ExecuteSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PromptHashHex {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != SHA256_HEX_LEN {
            return Err("prompt_hash must be exactly 64 hexadecimal characters (SHA-256 digest)");
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("prompt_hash must contain only ASCII hexadecimal digits");
        }
        Ok(Self(s.to_ascii_lowercase()))
    }
}

impl FromStr for ExecuteSessionId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != SESSION_SIMPLE_HEX_LEN {
            return Err("session_id must be exactly 32 hexadecimal characters (UUID simple form)");
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("session_id must contain only ASCII hexadecimal digits");
        }
        Ok(Self(s.to_ascii_lowercase()))
    }
}

impl<'de> Deserialize<'de> for PromptHashHex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ExecuteSessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_hash_round_trip() {
        let h = PromptHashHex::from_prompt_sha256("hello");
        let parsed: PromptHashHex = h.to_string().parse().unwrap();
        assert_eq!(h, parsed);
    }

    #[test]
    fn prompt_hash_normalizes_case() {
        let lower = PromptHashHex::from_prompt_sha256("x");
        let upper = lower.to_string().to_uppercase();
        let parsed: PromptHashHex = upper.parse().unwrap();
        assert_eq!(parsed, lower);
    }

    #[test]
    fn rejects_bad_prompt_hash_length() {
        assert!("".parse::<PromptHashHex>().is_err());
        assert!("a".repeat(63).parse::<PromptHashHex>().is_err());
        assert!("a".repeat(65).parse::<PromptHashHex>().is_err());
    }

    #[test]
    fn rejects_non_hex_in_prompt_hash() {
        let mut s = "a".repeat(64);
        s.replace_range(0..1, "g");
        assert!(s.parse::<PromptHashHex>().is_err());
    }

    #[test]
    fn rejects_bad_session_length() {
        assert!("".parse::<ExecuteSessionId>().is_err());
        assert!("0".repeat(31).parse::<ExecuteSessionId>().is_err());
    }
}
