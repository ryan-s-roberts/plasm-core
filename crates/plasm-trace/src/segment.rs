//! One append-only trace segment (tool / domain / expression row).

use plasm_observability_contracts::RunArtifactArchiveRef;
use plasm_runtime::http_trace::HttpTraceEntry;
use plasm_runtime::{ExecutionSource, ExecutionStats};
use serde::{Deserialize, Serialize};

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
    CodePlanEvaluate {
        plan_handle: String,
        plan_id: String,
        plan_name: String,
        plan_hash: String,
        prompt_hash: String,
        session_id: String,
        node_count: usize,
        code_chars: u64,
    },
    CodePlanExecute {
        plan_handle: String,
        plan_id: String,
        plan_name: String,
        plan_hash: String,
        prompt_hash: String,
        session_id: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        run_ids: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::TraceSegment;

    #[test]
    fn code_plan_trace_segments_carry_provenance() {
        let eval = TraceSegment::CodePlanEvaluate {
            plan_handle: "p1".into(),
            plan_id: "00000000-0000-0000-0000-000000000000".into(),
            plan_name: "demo".into(),
            plan_hash: "abc".into(),
            prompt_hash: "p".repeat(64),
            session_id: "s1".into(),
            node_count: 2,
            code_chars: 42,
        };
        let v = serde_json::to_value(eval).expect("json");
        assert_eq!(v["kind"], "code_plan_evaluate");
        assert_eq!(v["plan_handle"], "p1");

        let exec = TraceSegment::CodePlanExecute {
            plan_handle: "p1".into(),
            plan_id: "00000000-0000-0000-0000-000000000000".into(),
            plan_name: "demo".into(),
            plan_hash: "abc".into(),
            prompt_hash: "p".repeat(64),
            session_id: "s1".into(),
            run_ids: vec!["r1".into()],
        };
        let v = serde_json::to_value(exec).expect("json");
        assert_eq!(v["kind"], "code_plan_execute");
        assert_eq!(v["run_ids"][0], "r1");
    }
}
