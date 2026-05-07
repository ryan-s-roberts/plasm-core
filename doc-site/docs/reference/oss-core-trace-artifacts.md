# OSS / core: traces and run artifacts without object storage

**Enterprise / SaaS** may use **trace sink** (`PLASM_TRACE_SINK_URL` → `plasm-trace-sink`) and **object stores** (`PLASM_RUN_ARTIFACTS_URL`). **Core single-user OSS** should rely on **local disk** env vars — no S3 required.

## Environment variables (implemented)

| Purpose | Variable | Behavior |
|---------|----------|----------|
| Local trace archive | `PLASM_TRACE_ARCHIVE_DIR` | When set, completed traces are written under `traces/{tenant_id}/{trace_id}/` (summary + NDJSON). See [`local_trace_archive.rs`](../plasm-oss/crates/plasm-agent-core/src/local_trace_archive.rs). |
| Run snapshots / plan archive | `PLASM_RUN_ARTIFACTS_DIR` | Filesystem backend for execute run JSON and plan archive. **Precedence:** if `PLASM_RUN_ARTIFACTS_URL` is set, object store wins and `PLASM_RUN_ARTIFACTS_DIR` is ignored for backend selection. See [`run_artifacts.rs`](../plasm-oss/crates/plasm-agent-core/src/run_artifacts.rs). |
| Trace sink (optional, hosted-class) | `PLASM_TRACE_SINK_URL`, `PLASM_TRACE_SINK_READ_URL` | HTTP ingest + read base for durable tenant history beyond local archive. |

## Recommended OSS defaults (convention)

The runtime does **not** auto-assign directories when vars are unset (in-memory traces / artifacts otherwise). For a **viable local desktop** story, operators should set explicit paths, for example:

- **Traces:** `PLASM_TRACE_ARCHIVE_DIR="$HOME/.plasm/local/traces"`
- **Run artifacts:** `PLASM_RUN_ARTIFACTS_DIR="$HOME/.plasm/local/run-artifacts"`

Use absolute paths in scripts and systemd/desktop entries. Ensure the user running `plasm-mcp` can create those directories.

## Docs and UX alignment

- Document these vars in any **core** onboarding path (README / desktop installer), separate from Helm/object-store guides under `deploy/`.
- **Phoenix trace UI** (`ProjectTracesLive`, SSE proxy) expects agent `/v1/traces*`; durable list/detail requires local archive or sink — see [`http_traces.rs`](../plasm-oss/crates/plasm-agent-core/src/http_traces.rs).
