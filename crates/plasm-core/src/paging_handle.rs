//! Opaque host-minted pagination continuation handles.
//!
//! - **HTTP execute** (no MCP logical session): plain `pg1`, `pg2`, …
//! - **MCP `plasm`**: namespaced `s0_pg1`, `s1_pg2`, … where `s0` matches [`logical_session_ref`].

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Session-scoped opaque handle for LLM `page(...)` continuations (not a CGS entity name).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PagingHandle(String);

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PagingHandleParseError {
    #[error("paging handle must be plain `pg` + digits or namespaced `s` + digits + `_pg` + digits (got {0:?})")]
    InvalidFormat(String),
}

/// `logical_session_ref` segment: `s` + decimal digits (matches MCP tool contract).
#[inline]
pub fn is_valid_logical_session_ref_segment(s: &str) -> bool {
    s.len() >= 2 && s.starts_with('s') && s[1..].chars().all(|c| c.is_ascii_digit())
}

fn valid_plain_paging(s: &str) -> bool {
    if s.len() < 3 || !s.starts_with("pg") {
        return false;
    }
    let num = &s[2..];
    if num.is_empty() || num.len() > 24 {
        return false;
    }
    num.chars().all(|c| c.is_ascii_digit())
}

fn valid_namespaced_paging(s: &str) -> bool {
    let Some((slot, rest)) = s.split_once("_pg") else {
        return false;
    };
    if !is_valid_logical_session_ref_segment(slot) {
        return false;
    }
    if rest.is_empty() || rest.len() > 24 {
        return false;
    }
    rest.chars().all(|c| c.is_ascii_digit())
}

impl PagingHandle {
    /// Parses a client-supplied handle from `page(<ident>)` syntax: plain `pgN` or namespaced `s0_pgN`.
    pub fn parse(s: impl AsRef<str>) -> Result<Self, PagingHandleParseError> {
        let s = s.as_ref().trim();
        if s.contains("_pg") {
            if valid_namespaced_paging(s) {
                return Ok(Self(s.to_string()));
            }
            return Err(PagingHandleParseError::InvalidFormat(s.to_string()));
        }
        if valid_plain_paging(s) {
            return Ok(Self(s.to_string()));
        }
        Err(PagingHandleParseError::InvalidFormat(s.to_string()))
    }

    /// Host mint: monotonic `pgN` (HTTP execute without logical session).
    #[must_use]
    pub fn mint_monotonic(n: u64) -> Self {
        Self(format!("pg{n}"))
    }

    /// Host mint: MCP logical session slot + monotonic sequence within the execute session.
    /// `logical_session_ref` must satisfy [`is_valid_logical_session_ref_segment`].
    #[must_use]
    pub fn mint_namespaced(logical_session_ref: &str, n: u64) -> Self {
        Self(format!("{logical_session_ref}_pg{n}"))
    }

    /// `true` if this is a plain `pgN` handle (HTTP path).
    #[must_use]
    pub fn is_plain(&self) -> bool {
        valid_plain_paging(self.as_str())
    }

    /// `true` if this is `s{n}_pg{m}` (MCP path).
    #[must_use]
    pub fn is_logical_namespaced(&self) -> bool {
        valid_namespaced_paging(self.as_str())
    }

    /// For namespaced handles, returns the `s{n}` slot prefix (e.g. `s0`).
    #[must_use]
    pub fn logical_session_slot(&self) -> Option<&str> {
        let s = self.as_str();
        if !self.is_logical_namespaced() {
            return None;
        }
        s.split_once("_pg").map(|(slot, _)| slot)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PagingHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_plain() {
        assert_eq!(PagingHandle::parse("pg1").unwrap().as_str(), "pg1");
        assert_eq!(PagingHandle::parse("  pg42 ").unwrap().as_str(), "pg42");
        assert!(PagingHandle::parse("pg1").unwrap().is_plain());
        assert!(!PagingHandle::parse("pg1").unwrap().is_logical_namespaced());
    }

    #[test]
    fn parse_accepts_namespaced() {
        let h = PagingHandle::parse("s0_pg1").unwrap();
        assert_eq!(h.as_str(), "s0_pg1");
        assert!(h.is_logical_namespaced());
        assert!(!h.is_plain());
        assert_eq!(h.logical_session_slot(), Some("s0"));
    }

    #[test]
    fn parse_rejects_bad_namespaced() {
        assert!(PagingHandle::parse("s_pg1").is_err());
        assert!(PagingHandle::parse("s0_pg").is_err());
        assert!(PagingHandle::parse("x0_pg1").is_err());
    }

    #[test]
    fn parse_rejects_non_plain() {
        assert!(PagingHandle::parse("x1").is_err());
        assert!(PagingHandle::parse("p").is_err());
        assert!(PagingHandle::parse("pg").is_err());
        assert!(PagingHandle::parse("pgx1").is_err());
    }

    #[test]
    fn mint_namespaced_shape() {
        let h = PagingHandle::mint_namespaced("s3", 7);
        assert_eq!(h.as_str(), "s3_pg7");
        assert!(h.is_logical_namespaced());
    }

    #[test]
    fn serde_round_trips_as_string() {
        let h = PagingHandle::mint_monotonic(7);
        let v = serde_json::to_string(&h).unwrap();
        assert_eq!(v, "\"pg7\"");
        let back: PagingHandle = serde_json::from_str(&v).unwrap();
        assert_eq!(back, h);
    }
}
