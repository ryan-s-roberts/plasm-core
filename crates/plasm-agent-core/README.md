# plasm-agent-core

Open-source **MCP/HTTP host engine** for this workspace: execute, discovery, Streamable HTTP MCP, session + trace stores, and optional incoming-auth. Binaries live in the sibling [`plasm-agent`](../plasm-agent/) crate; the **SaaS `/internal/*` control-plane** surface is in [`plasm-saas`](../plasm-saas/). **Ownership map:** [docs/oss-saas-boundary.md](../../docs/oss-saas-boundary.md).

## Design boundary: no domain leakage

Plasm is a **general-purpose language and runtime for API mapping** (schema, expressions, CML, execution). **Domain-specific knowledge is forbidden in this crate:** no branches on particular CGS entity or capability names from `apis/…`, no field-alias or env-key hacks for one vendor’s HTTP templates, and no special transport cases tied to a single product.

Catalog behavior belongs in `**apis/<name>/`**, fixtures, and optional **plugins**—expressed as data and schema-driven rules. Code here stays **agnostic**, driven only by loaded CGS and generic IR/types.

See [AGENTS.md](../../AGENTS.md) for workspace layout and commands.

## Trace sink and TraceHub (MCP live traces)

- `**PLASM_TRACE_SINK_URL`** — Base URL for `**POST /v1/events**` (durable `mcp_trace_segment` audit rows to `plasm-trace-sink`). When unset, live SSE still streams from in-memory `[trace_hub](src/trace_hub.rs)`, but nothing is persisted; a **one-time** `tracing::warn!` is emitted per process on first attempted ingest.
- `**PLASM_TRACE_SINK_STRICT`** — If set to `1` / `true` / `yes` and `**PLASM_TRACE_SINK_URL` is unset**, a **one-time** `tracing::error!` is emitted (operators should fix config in environments where durable ingest is mandatory).
- `**PLASM_TRACE_HUB_INGEST_QUEUE_CAP`** — Bounded queue for async durable ingest jobs (default **512**). When full, **MCP / HTTP** trace emitters **wait** on `send` (after the live SSE `patch` is broadcast); durable work is not dropped for backpressure. `plasm.trace_hub.ingest_send_wait_ms` records that wait. `ingest_enqueue_failed_total` is for **closed** channel (shutdown), with optional SSE `durable_ingest` + `tracing::warn!`.

OpenTelemetry metric names for TraceHub + trace-sink HTTP are listed under **Meter: `plasm-agent`** in `[docs/otel-signoz-metrics-inventory.md](../../docs/otel-signoz-metrics-inventory.md)`.