//! Re-exports [`plasm_agent_core`] and the OSS `plasm-mcp` entrypoint.
//!
//! Data-plane behavior lives in `plasm-agent-core`. The private monorepo composes `plasm-saas` and
//! `plasm-mcp-app` for the full hosted control plane; this crate stays the open-source surface.

pub use plasm_agent_core::*;

pub mod embedded_postgres;
mod mcp_main;

pub use mcp_main::run_mcp_main;
