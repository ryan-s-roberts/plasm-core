//! OpenTelemetry (OTLP) wiring — standard `OTEL_*` env vars via [`plasm_otel`].
//!
//! Custom metrics (when a meter provider is installed) include `plasm.trace_hub.*` for MCP
//! in-memory trace queues, completed-trace cap evictions, and bounded durable ingest — see
//! [`crate::trace_hub_metrics`], [`crate::trace_hub::TraceHubBounds`], and [`crate::trace_hub::TraceHubConfig`].
//! Plugin-dir hot reload emits `plasm.plugin_catalog.reload.*` (see [`crate::metrics::record_plugin_catalog_reload`])
//! and structured `tracing` spans named `plasm.plugin_catalog.reload` plus `target = "plasm_plugin_catalog"` logs.

/// Install `tracing` + OTLP when collector endpoints are configured; otherwise stderr `tracing` only.
///
/// Misconfiguration falls back to console logging inside [`plasm_otel::init`].
pub fn init() -> anyhow::Result<()> {
    plasm_otel::init("plasm-agent")
}

/// Like [`init`], but sends the human-readable **fmt** layer through `make_writer` (for example a TUI log sink).
pub fn init_with_fmt_make_writer<W>(make_writer: W) -> anyhow::Result<()>
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + Clone + 'static,
{
    plasm_otel::init_with_fmt_make_writer("plasm-agent", make_writer)
}
