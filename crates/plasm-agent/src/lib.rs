//! Compatibility library crate: re-exports [`plasm_agent_core`] and wires the `plasm-mcp` entrypoint.
//!
//! Most functionality lives in `plasm-agent-core` (open-source host) and `plasm-saas` (private
//! control-plane surface). This crate exists so downstream tools can keep depending on the
//! `plasm_agent` name and the existing binary commands.

pub use plasm_agent_core::*;

mod mcp_main;

pub use mcp_main::run_mcp_main;
