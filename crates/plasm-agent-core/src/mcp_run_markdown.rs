//! MCP `plasm` tool Markdown: previews, snapshot URI lines, TSV-vs-table choice, and expression previews.
//!
//! **Projection vs transport summary:** path-expression **projection** (which fields/rows the executor
//! materializes) is separate from this module’s table/TSV/preview paths, which may still cap lossy
//! or reference-only cells or defer full JSON to `resources/read`. See repository `docs/mcp-session-reuse.md` (section 5).

use crate::output::{
    format_result_tsv_with_cgs, format_result_with_cgs, lossy_summary_field_names,
    InBandSummaryReport, LossySummaryFieldNames, OutputFormat,
};
use crate::run_artifacts::RunArtifactHandle;
use plasm_core::CGS;
use plasm_runtime::ExecutionResult;
use std::collections::BTreeSet;

/// When MCP uses session meta compaction + adaptive preview; above this Unicode scalar count, markdown omits full tables.
pub const MCP_PLASM_MARKDOWN_PREVIEW_THRESHOLD_CHARS: usize = 12_000;

/// Reserved after `## Result (preview)`; snapshot lines in the body carry the `resources/read` hint.
pub(crate) const MCP_MARKDOWN_PREVIEW_SINGLE_PROLOGUE: &str = "";

/// Reserved after `# Batch run (preview)`; same as single preview.
pub(crate) const MCP_MARKDOWN_PREVIEW_BATCH_PROLOGUE: &str = "";

/// Sorted unique field names omitted from the in-band summary as `(in artifact)` (reference-only strings).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OmittedReferenceOnlyFields(Vec<String>);

impl OmittedReferenceOnlyFields {
    pub(crate) fn from_vec_sorted_dedup(mut v: Vec<String>) -> Self {
        v.sort();
        v.dedup();
        Self(v)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn join_comma(&self) -> String {
        self.0.join(", ")
    }
}

impl From<BTreeSet<String>> for OmittedReferenceOnlyFields {
    fn from(set: BTreeSet<String>) -> Self {
        Self(set.into_iter().collect())
    }
}

impl AsRef<[String]> for OmittedReferenceOnlyFields {
    fn as_ref(&self) -> &[String] {
        &self.0
    }
}

/// In-band execute result plus which fields were withheld or lossy-capped in the Markdown summary.
#[derive(Debug, Clone)]
pub(crate) struct McpFormattedExecuteResult {
    pub block: McpExecuteResultBlock,
    pub reference_only_omitted: OmittedReferenceOnlyFields,
    pub lossy_summary_fields: LossySummaryFieldNames,
    /// Observed clamps / reference-only replacements while building `block` (single format pass).
    pub in_band_report: InBandSummaryReport,
}

/// In-band execute result body: either a fenced TSV block or an ASCII summary table.
#[derive(Debug, Clone)]
pub(crate) enum McpExecuteResultBlock {
    TsvFence { body: String },
    AsciiTable { body: String },
}

impl McpExecuteResultBlock {
    pub(crate) fn into_mcp_result_markdown(self) -> String {
        match self {
            McpExecuteResultBlock::TsvFence { body } => {
                let mut s = String::from("```tsv\n");
                s.push_str(&body);
                s.push_str("\n```\n");
                s
            }
            McpExecuteResultBlock::AsciiTable { body } => body,
        }
    }
}

/// Truncate long expression source lines for MCP previews and traces.
pub(crate) fn execute_expression_preview(expr: &str) -> String {
    const MAX_CHARS: usize = 400;
    let t = expr.trim();
    let n = t.chars().count();
    if n <= MAX_CHARS {
        return t.to_string();
    }
    let truncated: String = t.chars().take(MAX_CHARS).collect();
    format!("{truncated}… (truncated, total {n} chars)")
}

pub(crate) fn mcp_preview_markdown_needed(use_mcp_meta_profile: bool, full: &str) -> bool {
    use_mcp_meta_profile && full.chars().count() > MCP_PLASM_MARKDOWN_PREVIEW_THRESHOLD_CHARS
}

/// Union of schema-tagged lossy columns and any field names recorded while formatting in-band cells
/// (default table budget, TSV transport clamp, reference-only).
pub(crate) fn merge_snapshot_column_hints(
    schema_lossy: &LossySummaryFieldNames,
    in_band: &InBandSummaryReport,
) -> LossySummaryFieldNames {
    let mut v: Vec<String> = schema_lossy.as_ref().to_vec();
    v.extend(in_band.field_names().cloned());
    LossySummaryFieldNames::from_vec_sorted_dedup(v)
}

/// One-line Markdown after an in-band result when the run snapshot must be fetched separately.
pub(crate) fn mcp_inline_run_snapshot_line(handle: &RunArtifactHandle) -> String {
    format!(
        "\n\n_Snapshot (`resources/read`):_ `{}`\n",
        handle.plasm_uri
    )
}

/// MCP `plasm` tool: use fenced TSV when there are no reference-only omissions; otherwise ASCII table with `(in artifact)`.
pub(crate) fn mcp_format_execute_result_table_or_tsv(
    result: &ExecutionResult,
    cgs: Option<&CGS>,
) -> McpFormattedExecuteResult {
    let lossy_summary_fields = lossy_summary_field_names(result, cgs);
    let (tsv, omitted_vec, tsv_report) = format_result_tsv_with_cgs(result, cgs);
    let reference_only_omitted = OmittedReferenceOnlyFields::from_vec_sorted_dedup(omitted_vec);
    if reference_only_omitted.is_empty() {
        McpFormattedExecuteResult {
            block: McpExecuteResultBlock::TsvFence { body: tsv },
            reference_only_omitted,
            lossy_summary_fields,
            in_band_report: tsv_report,
        }
    } else {
        let (table, omitted2, table_report) =
            format_result_with_cgs(result, OutputFormat::Table, cgs);
        let omitted2 = OmittedReferenceOnlyFields::from_vec_sorted_dedup(omitted2);
        debug_assert_eq!(
            reference_only_omitted, omitted2,
            "TSV vs table omission mismatch"
        );
        McpFormattedExecuteResult {
            block: McpExecuteResultBlock::AsciiTable { body: table },
            reference_only_omitted: omitted2,
            lossy_summary_fields,
            in_band_report: table_report,
        }
    }
}

pub(crate) fn mcp_compact_markdown_single(
    line: &str,
    parsed_display: &str,
    projection: Option<&[String]>,
    entity_rows: usize,
    omitted: &OmittedReferenceOnlyFields,
    lossy_summary_fields: &LossySummaryFieldNames,
) -> String {
    let mut out = String::from("## Result (preview)\n\n");
    out.push_str(MCP_MARKDOWN_PREVIEW_SINGLE_PROLOGUE);
    out.push_str("→ ");
    out.push_str(parsed_display);
    out.push('\n');
    if let Some(p) = projection {
        out.push_str("  projection: [");
        out.push_str(&p.join(", "));
        out.push_str("]\n");
    }
    out.push('\n');
    out.push('`');
    out.push_str(&execute_expression_preview(line));
    out.push_str("`\n\n");
    out.push_str("**Entity rows:** ");
    out.push_str(&entity_rows.to_string());
    out.push('\n');
    if !omitted.is_empty() {
        out.push_str("**Omitted from summary (reference-only fields):** ");
        out.push_str(&omitted.join_comma());
        out.push('\n');
    }
    if !lossy_summary_fields.is_empty() {
        out.push_str("**Abbreviated in-band columns (full text in snapshot):** ");
        out.push_str(&lossy_summary_fields.join_comma());
        out.push('\n');
    }
    out
}

pub(crate) fn mcp_compact_markdown_batch(
    total_steps: usize,
    total_entity_rows: usize,
    per_step: &[(String, String, usize)],
    omitted: &OmittedReferenceOnlyFields,
    lossy_summary_union: &LossySummaryFieldNames,
    truncated_step_uris: &[(usize, &RunArtifactHandle)],
) -> String {
    let mut out = String::from("# Batch run (preview)\n\n");
    out.push_str(MCP_MARKDOWN_PREVIEW_BATCH_PROLOGUE);
    out.push_str("**Steps:** ");
    out.push_str(&total_steps.to_string());
    out.push('\n');
    out.push_str("**Total entity rows (sum):** ");
    out.push_str(&total_entity_rows.to_string());
    out.push_str("\n\n### Per step\n\n");
    for (i, (line, disp, nrows)) in per_step.iter().enumerate() {
        let step_no = i + 1;
        out.push_str(&format!(
            "{}. `{}` → {}\n   **Rows:** {}\n",
            step_no,
            execute_expression_preview(line),
            disp,
            nrows
        ));
        if let Some((_, h)) = truncated_step_uris.iter().find(|(s, _)| *s == step_no) {
            out.push_str(&format!(
                "   _Snapshot (`resources/read`):_ `{}`\n",
                h.plasm_uri
            ));
        }
        out.push('\n');
    }
    if !omitted.is_empty() {
        out.push_str("**Omitted from summary (reference-only fields):** ");
        out.push_str(&omitted.join_comma());
        out.push('\n');
    }
    if !lossy_summary_union.is_empty() {
        out.push_str("**Abbreviated in-band columns (full text in snapshot):** ");
        out.push_str(&lossy_summary_union.join_comma());
        out.push('\n');
    }
    out
}

/// Prepends a short note when `use_mcp_meta` is true and there are reference-only omissions but no
/// inline snapshot row yet. Snapshot URIs elsewhere in the body remain the authoritative pointer to full JSON.
pub(crate) fn mcp_prepend_artifact_followup_markdown(
    markdown: String,
    use_mcp_meta: bool,
    truncated_snapshot_handles: &[RunArtifactHandle],
    omitted: &OmittedReferenceOnlyFields,
) -> String {
    if !use_mcp_meta || (truncated_snapshot_handles.is_empty() && omitted.is_empty()) {
        return markdown;
    }
    // Inline `_Snapshot (`resources/read`): …` rows already carry the hint; no extra banner.
    if !truncated_snapshot_handles.is_empty() {
        return markdown;
    }
    let mut p = String::from("**Reference-only fields omitted from this summary:** ");
    p.push_str(&omitted.join_comma());
    p.push_str(".\n\n");
    format!("{p}{markdown}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::LossySummaryFieldNames;
    use uuid::Uuid;

    #[test]
    fn mcp_markdown_preview_prologues_are_empty() {
        assert!(MCP_MARKDOWN_PREVIEW_SINGLE_PROLOGUE.is_empty());
        assert!(MCP_MARKDOWN_PREVIEW_BATCH_PROLOGUE.is_empty());
    }

    #[test]
    fn mcp_compact_markdown_single_preview_has_no_must_read_banner() {
        let omitted = OmittedReferenceOnlyFields::from_vec_sorted_dedup(vec!["body".into()]);
        let s = mcp_compact_markdown_single(
            "Issue(x).comments",
            "Issue(x).comments",
            None,
            2,
            &omitted,
            &LossySummaryFieldNames::default(),
        );
        assert!(s.starts_with("## Result (preview)"));
        assert!(!s.contains("MUST"), "preview: {s}");
        assert!(!s.contains("Optional full JSON"), "preview: {s}");
        assert!(s.contains("body"), "omitted fields listed: {s}");
    }

    #[test]
    fn mcp_compact_markdown_batch_preview_has_no_must_read_banner() {
        let omitted = OmittedReferenceOnlyFields::from_vec_sorted_dedup(vec!["commentBody".into()]);
        let h = sample_handle();
        let s = mcp_compact_markdown_batch(
            2,
            5,
            &[
                ("a".into(), "disp".into(), 3),
                ("b".into(), "disp2".into(), 2),
            ],
            &omitted,
            &LossySummaryFieldNames::default(),
            &[(1, &h)],
        );
        assert!(s.starts_with("# Batch run (preview)"));
        assert!(!s.contains("MUST"), "batch preview: {s}");
        assert!(!s.contains("Optional full JSON"), "batch preview: {s}");
        assert!(s.contains("commentBody"), "batch preview: {s}");
        assert!(s.contains(&h.plasm_uri), "truncated step URI inline: {s}");
    }

    fn sample_handle() -> RunArtifactHandle {
        RunArtifactHandle {
            run_id: Uuid::nil(),
            plasm_uri: "plasm://r/1".into(),
            canonical_plasm_uri: "plasm://execute/a/b/run/00000000-0000-0000-0000-000000000000"
                .into(),
            http_path: "/x".into(),
            payload_len: 1,
            request_fingerprints: vec![],
        }
    }

    #[test]
    fn mcp_prepend_artifact_followup_noop_without_mcp_meta() {
        let h = sample_handle();
        let omitted = OmittedReferenceOnlyFields::default();
        let out =
            mcp_prepend_artifact_followup_markdown("## Result\n".into(), false, &[h], &omitted);
        assert_eq!(out, "## Result\n");
    }

    #[test]
    fn mcp_prepend_artifact_followup_no_prefix_when_no_truncated_snapshots() {
        let rid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let h = RunArtifactHandle {
            run_id: rid,
            plasm_uri: "plasm://r/3".into(),
            canonical_plasm_uri: "plasm://execute/ph/sess/run/550e8400-e29b-41d4-a716-446655440000"
                .into(),
            http_path: "/execute/ph/sess/artifacts/550e8400-e29b-41d4-a716-446655440000".into(),
            payload_len: 100,
            request_fingerprints: vec!["abc".into()],
        };
        let omitted = OmittedReferenceOnlyFields::default();
        let out = mcp_prepend_artifact_followup_markdown("## Result\n".into(), true, &[], &omitted);
        assert_eq!(out, "## Result\n", "{out}");
        let body = format!("## Result\n{}", mcp_inline_run_snapshot_line(&h));
        let out2 = mcp_prepend_artifact_followup_markdown(body.clone(), true, &[h], &omitted);
        assert_eq!(out2, body, "{out2}");
        assert!(!out2.contains("Optional full JSON"), "{out2}");
        assert!(out2.contains("plasm://r/3"), "{out2}");
    }

    #[test]
    fn mcp_prepend_artifact_followup_with_handles_returns_body_unchanged() {
        let rid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let h = RunArtifactHandle {
            run_id: rid,
            plasm_uri: "plasm://r/2".into(),
            canonical_plasm_uri: "plasm://execute/ph/sess/run/550e8400-e29b-41d4-a716-446655440000"
                .into(),
            http_path: "/execute/ph/sess/artifacts/550e8400-e29b-41d4-a716-446655440000".into(),
            payload_len: 100,
            request_fingerprints: vec![],
        };
        let omitted = OmittedReferenceOnlyFields::from_vec_sorted_dedup(vec!["body".into()]);
        let out =
            mcp_prepend_artifact_followup_markdown("## Result\n".into(), true, &[h], &omitted);
        assert_eq!(out, "## Result\n", "{out}");
        assert!(!out.contains("MUST"), "{out}");
    }

    #[test]
    fn mcp_prepend_artifact_followup_omitted_only_without_handles() {
        let omitted = OmittedReferenceOnlyFields::from_vec_sorted_dedup(vec!["body".into()]);
        let out = mcp_prepend_artifact_followup_markdown("table\n".into(), true, &[], &omitted);
        assert!(out.contains("Reference-only fields omitted"), "{out}");
        assert!(out.contains("body"), "{out}");
        assert!(!out.contains("call **`resources/read`**"), "{out}");
        assert!(out.ends_with("table\n"), "{out}");
    }
}
