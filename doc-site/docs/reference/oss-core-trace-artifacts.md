# OSS / core: traces and run artifacts without object storage

**Enterprise / SaaS** may use **trace sink** (`PLASM_TRACE_SINK_URL` → `plasm-trace-sink`) and **object stores** (`PLASM_RUN_ARTIFACTS_URL`). **Core single-user OSS** should rely on **local disk** env vars — no S3 required.

## Environment variables (implemented)

| Purpose | Variable | Behavior |
|---------|----------|----------|
| Local trace archive | `PLASM_TRACE_ARCHIVE_DIR` | When set, completed traces are written under `traces/{tenant_id}/{trace_id}/` (summary + NDJSON). See ``local_trace_archive.rs``. |
| Run snapshots / plan archive | `PLASM_RUN_ARTIFACTS_DIR` | Filesystem backend for execute run JSON and plan archive. **Precedence:** if `PLASM_RUN_ARTIFACTS_URL` is set, object store wins and `PLASM_RUN_ARTIFACTS_DIR` is ignored for backend selection. See ``run_artifacts.rs``. |
| Trace sink (optional, hosted-class) | `PLASM_TRACE_SINK_URL`, `PLASM_TRACE_SINK_READ_URL` | HTTP ingest + read base for durable tenant history beyond local archive. |

## Recommended OSS defaults (convention)

The runtime does **not** auto-assign directories when vars are unset (in-memory traces / artifacts otherwise). For a **viable local desktop** story, operators should set explicit paths, for example:

- **Traces:** `PLASM_TRACE_ARCHIVE_DIR="$HOME/.plasm/local/traces"`
- **Run artifacts:** `PLASM_RUN_ARTIFACTS_DIR="$HOME/.plasm/local/run-artifacts"`

Use absolute paths in scripts and systemd entries. Ensure the user running **`plasm-server`** can create those directories.

## Run snapshot identity (`run_id`)

Execute run JSON snapshots and MCP `plasm://execute/.../run/{run_id}` URIs use a **single wire form**: ASCII prefix **`pr`** plus **64 hexadecimal digits** (lowercase in server output; parsers accept uppercase hex). This is the SHA256 of a versioned JSON bundle (pinned `catalog_cgs_hash`, `domain_revision`, `entry_id`, trimmed source line, serialized parsed plan, and **sorted** `request_fingerprints`) so paging and distinct HTTP batches produce distinct ids. Durable filesystem / object-store blob filenames use the **full 32-byte digest** (see `run_artifacts.rs`); hyphenated **UUID** `run_id` segments are **not** accepted on GET or in `resources/read` canonical URIs (full cutover; no legacy fallback).

## Docs and UX alignment

- Document these vars in any **core** onboarding path (README / installer), separate from hosted object-store guides.
- Durable trace list/detail requires **`PLASM_TRACE_ARCHIVE_DIR`** or a trace sink URL; see [Control station — Storage tab](../appliance/tui.md#other-tabs).
