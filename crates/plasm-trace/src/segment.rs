//! One append-only trace segment (tool / domain / expression row).

use plasm_runtime::http_trace::HttpTraceEntry;
use plasm_runtime::{ExecutionSource, ExecutionStats};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Source + REPL display strings recorded with each executed line trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlasmLineTraceMeta {
    pub source_expression: String,
    pub repl_pre: String,
    pub repl_post: String,
    pub capability: Option<String>,
    pub operation: String,
    pub api_entry_id: Option<String>,
}

/// Stable archive identity for an execute run snapshot (HTTP artifacts + MCP `resources/read`).
/// Aligns with `RunArtifactDocument` (`run_id`, `prompt_hash`, `session_id`, optional `resource_index`) for durable storage and future web deep-links.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunArtifactArchiveRef {
    pub prompt_hash: String,
    pub session_id: String,
    pub run_id: Uuid,
    /// Present when the client read via short `plasm://session/.../r/{n}` (monotonic index).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_index: Option<u64>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Append-only trace segments (JSON-serializable for HTTP + SSE + Iceberg `payload_json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceSegment {
    AddCapabilities {
        domain_prompt_chars_added: u64,
        reused_session: bool,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        mode: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entry_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        entities: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        seeds: Vec<String>,
    },
    ExpandDomain {
        domain_prompt_chars_added: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entry_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        entities: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        seeds: Vec<String>,
    },
    PlasmInvocation {
        call_index: u64,
        batch: bool,
        expression_count: usize,
        /// Character weight of this invocation (for aggregate replay from durable rows).
        plasm_invocation_chars_added: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_chars: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    PlasmLine {
        call_index: u64,
        line_index: usize,
        source_expression: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        repl_pre: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        repl_post: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capability: Option<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        api_entry_id: Option<String>,
        duration_ms: u64,
        stats: ExecutionStats,
        source: ExecutionSource,
        request_fingerprints: Vec<String>,
        http_calls: Vec<HttpTraceEntry>,
    },
    PlasmError {
        call_index: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        line_index: Option<usize>,
        message: String,
    },
    /// Domain prompt character weight without an `add_capabilities` / `expand_domain` row (rare; durable parity).
    DomainPromptCharsDelta { chars_added: u64 },
    /// Response markdown character weight (MCP tool body sizing; pairs with successful `plasm` tool).
    PlasmResponseCharsDelta {
        chars_added: u64,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call_index: Option<u64>,
        #[serde(default, skip_serializing_if = "is_false")]
        batch: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expression_count: Option<usize>,
    },
    /// MCP `resources/read` on a run snapshot URI (size + timing + archive ref for future UI).
    McpResourceRead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        archive: Option<RunArtifactArchiveRef>,
        /// Truncated request URI for display.
        uri_display: String,
        chars_added: u64,
        #[serde(default, skip_serializing_if = "is_false")]
        is_binary: bool,
        duration_ms: u64,
        /// `"success"` or `"error"`.
        result: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_class: Option<String>,
    },
}
