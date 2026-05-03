# Genco / compile-plugin pipeline

**Scope:** OSS `**plasm-mcp`** (`plasm-agent` package) catalog modes (`--schema`, `--plugin-dir`), compile-only plugins, and session pinning.

This document describes how **generated** (e.g. genco-style) or hand-written `**cdylib`** artifacts integrate with Plasm’s compile-only plugin path, how **catalog metadata** (ABI v4) supports **startup without a checked-in registry YAML**, and how `**--plugin-dir`** loads multi-entry catalogs from disk.

## ABI versions

**Current ABI:** `[PLASM_PLUGIN_ABI_VERSION](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-abi/src/lib.rs)` **= 4**.

- **v3** introduced `**plasm_plugin_catalog_metadata`**; **v4** aligns catalog interchange with **required `CGS.http_backend`** (HTTP origin lives in embedded CGS YAML, not in `PluginCatalogMetadata`).
- Request/response bodies for compile calls remain: **4-byte little-endian ABI `u32`** + **CBOR** payload (see `[plasm-plugin-abi](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-abi)`).

## Artifacts

1. `**plasm-plugin-abi`** (`[crates/plasm-plugin-abi](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-abi)`) — typed compile request/response structs, **CBOR** (`ciborium`) payloads on the wire (prefix: little-endian `PLASM_PLUGIN_ABI_VERSION` `u32`), plus C symbol names (`plasm_plugin_compile_operation`, `plasm_plugin_compile_query`, `**plasm_plugin_catalog_metadata`**, `plasm_plugin_free_buffer`).
2. `**plasm-plugin-stub**` (`[crates/plasm-plugin-stub](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-stub)`) — reference `cdylib` (+ `rlib` for test builds) that forwards compile calls to `[plasm_compile](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-compile)` and serves embedded catalog metadata (used to validate the host loader).
3. **Host** — `[plasm-plugin-host](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-host)` loads the library and builds closures for `[plasm_runtime::ExecuteOptions](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)` (`compile_operation_fn`, `compile_query_fn`). `[load_catalog_metadata](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-host/src/plugin_metadata.rs)` performs a short `dlopen` to read `**plasm_plugin_catalog_metadata`** without keeping the library resident.

## Catalog metadata export (`plasm_plugin_catalog_metadata`)

ABI **v4** plugins export:

```text
plasm_plugin_catalog_metadata(req, req_len, out_ptr, out_len, err_ptr, err_len) -> i32
```

Same `**0` = success** / buffer contract as the compile exports (`plasm_plugin_free_buffer` releases host-visible buffers). The response frame is `**PluginCatalogMetadata`** (CBOR after the ABI version prefix), including:

- `**entry_id**`, `**version**` — publisher-assigned; the agent picks the **highest `version` per `entry_id`** when scanning a directory.
- `**cgs_hash**` — hex SHA-256 of canonical JSON for the embedded CGS (see `[CGS::catalog_cgs_hash_hex](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs)`); must match the parsed embedded CGS after load.
- `**target_triple**` — must match the **host** triple (agent records this at build time via its `build.rs`); mismatched artifacts are skipped with a warning.
- `**cgs_yaml`** — embedded CGS interchange bytes (YAML), including required `**http_backend**` on `[CGS](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs)`.
- Optional `**label**`, `**tags**` for discovery-style metadata.

## CGS catalog fields

Interchange `[CGS](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs)` carries optional `**entry_id**` and monotonic `**version**` (`u64`, default `0`) so the serialized catalog and the plugin metadata stay aligned. `**[CGS::catalog_cgs_hash_hex](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs)**` hashes the full interchange (JSON bytes) for integrity and session pinning.

## Runtime dispatch (`plasm-runtime`)

The executor does not thread function pointers through every internal call site. Instead:

- `[ExecuteOptions](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)` optionally carries `Arc` compile closures and a `**plugin_generation_id**` (observability).
- For each `[ExecutionEngine::execute](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)` and for `[auto_resolve_projection](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)`, the engine sets a Tokio **task-local** `[EXECUTION_PLUGIN_HOOKS](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)` (`PluginCompileHooks`: optional operation/query `Arc`s only; generation id stays on `[ExecuteOptions](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)`) for the duration of the work.
- Internal code calls `**compile_operation_dispatch`** / `**compile_query_dispatch**`, which read that task-local and invoke the plugin when set, otherwise fall back to in-tree `[plasm_compile::compile_operation](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-compile/src/lib.rs)` / `[compile_query](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-compile/src/lib.rs)`.

Thus execute sessions (and projection hydration) use the same pinned generation as the CLI would with equivalent options.

## Agent startup: build (`apis/`) vs runtime (`--plugin-dir`)


| Phase             | Tool                                                                                                            | What happens                                                                                                                                      |
| ----------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Authoring**     | Edit `**apis/<name>/domain.yaml`** + `**mappings.yaml**`                                                        | Source of truth in git.                                                                                                                           |
| **Pack (build)**  | `**plasm-pack-plugins`** (`[plasm_pack_plugins.rs](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent/src/bin/plasm_pack_plugins.rs)`) | One ABI v4 `**plasm-plugin-stub**` cdylib per package; embeds CGS interchange.                                                                    |
| **Runtime**       | `**plasm-mcp --plugin-dir <dir>`**                                                                              | `[plugin_catalog::load_registry_from_plugin_dir](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent-core/src/plugin_catalog.rs)` — highest `**version` per `entry_id**`. |
| **Single schema** | `**--schema <path>`**                                                                                           | One CGS (no plugin dir).                                                                                                                          |


**Mutual exclusion:** do not combine `**--plugin-dir`** with `**--schema**`. `**--compile-plugin**` remains orthogonal: optional dylib that overrides **compile** only (operation/query), not catalog discovery.

`[plugin_catalog](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent-core/src/plugin_catalog.rs)` builds an `[InMemoryCgsRegistry](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/discovery.rs)` and runs template validation across entries.

## Session reuse and pinning

- **Compile plugin:** `[SessionReuseKey](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent-core/src/execute_session.rs)` includes `**plugin_generation_id`** so a hot reload does not reuse a session tied to an older dylib generation.
- **Catalog:** the same key includes `**catalog_cgs_hash`** (and `[ExecuteSession](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent-core/src/execute_session.rs)` stores `**catalog_cgs_hash**`) so HTTP reuse paths do not silently reuse a session after the **pinned CGS** for that entry changes (e.g. new plugin artifact or edited `domain.yaml`).

## Build

```bash
cargo build -p plasm-plugin-stub
# macOS: target/debug/libplasm_plugin_stub.dylib (also under target/debug/deps/ when built as a dependency)
# Linux: target/debug/libplasm_plugin_stub.so
```

The stub crate uses `**crate-type = ["cdylib", "rlib"]**` so `cargo test -p plasm-plugin-host` rebuilds the dylib used by loader tests (fresh copy under `target/debug/deps/`).

## HTTP / MCP examples

**Compile-only override** (same catalog as YAML/schema path):

```bash
cargo run -p plasm-agent --bin plasm-mcp -- --schema apis/dnd5e --http --port 3000 \
  --compile-plugin target/debug/libplasm_plugin_stub.dylib
```

**Catalog from plugins** (no `--schema`):

```bash
cargo run -p plasm-agent --bin plasm-mcp -- --plugin-dir /path/to/plugins --http --port 3001 --mcp --mcp-port 3000
```

- **New** execute sessions pin the **current** plugin generation from `[PluginManager](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-host/src/lib.rs)` when `--compile-plugin` is set.
- **Session reuse** keys include `**plugin_generation_id`** and `**catalog_cgs_hash**` so reloads and catalog changes do not silently reuse old sessions.
- The host copies `[LoadedPluginGeneration](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-host/src/lib.rs)` closures into `[ExecuteOptions](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)` (with trait-object-compatible wrappers); the runtime task-local scope applies them to execute and to projection auto-resolution.
- Transport, replay, and decoding remain in the host `[ExecutionEngine](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/execution.rs)`.

## Deployment notes

Kubernetes Helm charts used by hosted deployments live outside this OSS repository. Run `**plasm-mcp`** with `**--plugin-dir**` pointing at a directory of ABI v4 `**cdylib**` artifacts (volume-mounted or synced).

**Independent plugin delivery:** Catalog plugins can **version on their own cadence**. The only contract with `plasm-mcp` is **a directory of ABI v4 `cdylib`s** visible at `**--plugin-dir`**.

**Reload:** The host builds `[InMemoryCgsRegistry](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/discovery.rs)` from disk at **startup** (`[load_registry_from_plugin_dir](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent-core/src/plugin_catalog.rs)`). Hosted stacks may expose `**POST /internal/plugin-registry/v1/reload`** with the control-plane secret header (implementation is product-specific). `**409**` if the process was started with `**--schema**` instead of `**--plugin-dir**`. Otherwise restart the process after replacing plugin binaries on disk.

## Genco direction (future)

A code generator would emit Rust sources that either:

- **Delegate** (like `plasm-plugin-stub`) for rapid iteration, or
- **Specialize** per capability (inline CML / filter logic) for lower latency,

then build a `cdylib` with the stable exports above. Register either:

- a single path via `**--compile-plugin`**, and/or
- a directory of versioned catalog plugins via `**--plugin-dir**`.

## Execute run artifacts (snapshots)

`[RunArtifactStore](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent-core/src/run_artifacts.rs)` backs `**GET /execute/.../artifacts/:run_id**` and MCP `**resources/read**` for `plasm://execute/…` URIs.


| Mode                                  | Configuration                                                                                                                                                                                                                                                 |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **In-memory** (default)               | No env; snapshots are process-local and lost on restart.                                                                                                                                                                                                      |
| **Object store**                      | `**PLASM_RUN_ARTIFACTS_URL`**: `[object_store::parse_url_opts](https://docs.rs/object_store/latest/object_store/fn.parse_url_opts.html)` URL (e.g. `s3://bucket/prefix`, `file:///path/to/dir`). Credentials and region follow each backend’s usual env vars. |
| **Time-based GC** (object store only) | `**PLASM_RUN_ARTIFACTS_RETENTION_SECS`** (default **604800** = 7 days): delete listed objects whose `**last_modified`** is older than this. `**PLASM_RUN_ARTIFACTS_GC_INTERVAL_SECS**` (default **300**): background sweep interval.                          |


Object keys: `{url-prefix}/execute/{prompt_hash}/{session_id}/{run_id}.json`.

## Persistent session graph cache (delta + snapshot)

`plasm-agent` can persist session graph/runtime artifacts in an object-store-friendly shape:


| Mode                                        | Configuration                                                                                                                               |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| **Disabled** (default)                      | No env; session graph state remains in-memory only.                                                                                         |
| **Object store deltas + snapshot manifest** | `**PLASM_GRAPH_CACHE_URL`**: `[object_store::parse_url_opts](https://docs.rs/object_store/latest/object_store/fn.parse_url_opts.html)` URL. |


Layout:

- Delta append objects: `{url-prefix}/v1/sessions/{prompt_hash}/{session_id}/delta/{seq}.bin`
- Snapshot object on session release: `{url-prefix}/v1/sessions/{prompt_hash}/{session_id}/snapshots/{through_seq}.bin`
- Snapshot manifest: `{url-prefix}/v1/sessions/{prompt_hash}/{session_id}/manifest.json`

Payloads are treated as arbitrary bytes with typed metadata (`content_type`, optional `content_encoding`, `schema_version`, `producer`); JSON is only one codec option.

## Operational backlog

- **PluginManager:** on each `**reload`**, the host clears prior entries from its generation map so dylibs can unload once no execute session still holds a pinned `Arc` to `[LoadedPluginGeneration](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-plugin-host/src/lib.rs)`.

