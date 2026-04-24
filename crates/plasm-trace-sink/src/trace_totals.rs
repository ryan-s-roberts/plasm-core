//! Trace head row → [`TraceTotals`] for list views (shared by Iceberg decoders and SQL projection).

use plasm_trace::{totals_from_session_data, SessionTraceCountersSnapshot};

use crate::model::{TraceHeadRow, TraceTotals};

/// Derive list totals from a durable head row (`totals_json` snapshot or line-based fallback).
pub(crate) fn trace_totals_from_head_row(h: &TraceHeadRow) -> TraceTotals {
    let tj = h.totals_json.trim();
    if !tj.is_empty() {
        if let Ok(snap) = serde_json::from_str::<SessionTraceCountersSnapshot>(tj) {
            let mcp = h.mcp_session_id.clone().unwrap_or_default();
            let data = snap.into_session_data(mcp);
            return totals_from_session_data(&data).into();
        }
    }
    TraceTotals {
        plasm_tool_calls: h.max_call_index.map(|c| (c.max(0) as u64) + 1).unwrap_or(0),
        plasm_expressions: 0,
        expression_lines: h.expression_lines.max(0) as u64,
        ..TraceTotals::default()
    }
}
