# Genco / compile-plugin pipeline

**Architecture context:** saas-architecture.md (hosted product; not published in OSS docs) (catalog modes, auth boundaries).

This document describes how **generated** (e.g. genco-style) or hand-written **`cdylib`** artifacts integrate with Plasm’s compile-only plugin path, how **catalog metadata** (ABI v4) supports **startup without a checked-in registry YAML**, and how **`--plugin-dir`** loads multi-entry catalogs from disk.

## ABI versions

**Current ABI:** ``PLASM_PLUGIN_ABI_VERSION`` **= 4**.

- **v3** introduced **`plasm_plugin_catalog_metadata`**; **v4** aligns catalog interchange with **required `CGS.http_backend`** (HTTP origin lives in embedded CGS YAML, not in `PluginCatalogMetadata`).
- Request/response bodies for compile calls remain: **4-byte little-endian ABI `u32`** + **CBOR** payload (see ``plasm-plugin-abi``).

## Artifacts

1. **`plasm-plugin-abi`** (``plasm-oss/crates/plasm-plugin-abi``) — typed compile request/response structs, **CBOR** (`ciborium`) payloads on the wire (prefix: little-endian `PLASM_PLUGIN_ABI_VERSION` `u32`), plus C symbol names (`plasm_plugin_compile_operation`, `plasm_plugin_compile_query`, **`plasm_plugin_catalog_metadata`**, `plasm_plugin_free_buffer`).
2. **`plasm-plugin-stub`** (``plasm-oss/crates/plasm-plugin-stub``) — reference `cdylib` (+ `rlib` for test builds) that forwards compile calls to ``plasm_compile`` and serves embedded catalog metadata (used to validate the host loader).
3. **Host** — ``plasm-plugin-host`` loads the library and builds closures for ``plasm_runtime::ExecuteOptions`` (`compile_operation_fn`, `compile_query_fn`). ``load_catalog_metadata`` performs a short `dlopen` to read **`plasm_plugin_catalog_metadata`** without keeping the library resident.

## Catalog metadata export (`plasm_plugin_catalog_metadata`)

ABI **v4** plugins export:

```text
plasm_plugin_catalog_metadata(req, req_len, out_ptr, out_len, err_ptr, err_len) -> i32
```

Same **`0` = success** / buffer contract as the compile exports (`plasm_plugin_free_buffer` releases host-visible buffers). The response frame is **`PluginCatalogMetadata`** (CBOR after the ABI version prefix), including:

- **`entry_id`**, **`version`** — publisher-assigned; the agent picks the **highest `version` per `entry_id`** when scanning a directory.
- **`cgs_hash`** — hex SHA-256 of canonical JSON for the embedded CGS (see ``CGS::catalog_cgs_hash_hex``); must match the parsed embedded CGS after load.
- **`target_triple`** — must match the **host** triple (agent records this at build time via its `build.rs`); mismatched artifacts are skipped with a warning.
- **`cgs_yaml`** — embedded CGS interchange bytes (YAML), including required **`http_backend`** on ``CGS``.
- Optional **`label`**, **`tags`** for discovery-style metadata.

## CGS catalog fields

Interchange ``CGS`` carries optional **`entry_id`** and monotonic **`version`** (`u64`, default `0`) so the serialized catalog and the plugin metadata stay aligned. **``CGS::catalog_cgs_hash_hex``** hashes the full interchange (JSON bytes) for integrity and session pinning.

## Runtime dispatch (`plasm-runtime`)

The executor does not thread function pointers through every internal call site. Instead:

- ``ExecuteOptions`` optionally carries `Arc` compile closures and a **`plugin_generation_id`** (observability).
- For each ``ExecutionEngine::execute`` and for ``auto_resolve_projection``, the engine sets a Tokio **task-local** ``EXECUTION_PLUGIN_HOOKS`` (`PluginCompileHooks`: optional operation/query `Arc`s only; generation id stays on ``ExecuteOptions``) for the duration of the work.
- Internal code calls **`compile_operation_dispatch`** / **`compile_query_dispatch`**, which read that task-local and invoke the plugin when set, otherwise fall back to in-tree ``plasm_compile::compile_operation`` / ``compile_query``.

Thus HTTP/MCP execute sessions (and projection hydration) use the same pinned generation as the CLI would with equivalent options.

## Agent startup: build (`apis/`) vs runtime (`--plugin-dir`)

| Phase | Tool | What happens |
|------|------|----------------|
| **Authoring** | Edit **`apis/<name>/domain.yaml`** + **`mappings.yaml`** | Source of truth in git. |
| **Pack (build)** | **`plasm-pack-plugins`** (``plasm_pack_plugins.rs``) | One ABI v4 **`plasm-plugin-stub`** cdylib per package; embeds CGS interchange. |
| **Runtime** | **`plasm-server`** | Loads ABI v4 cdylibs from `{appliance}/plugins` or **`--plugin-dir`** — highest **`version` per `entry_id`**. |
| **Single schema** | **`--schema <path>`** | One CGS (no plugin dir). |

**Mutual exclusion:** do not combine **`--plugin-dir`** with **`--schema`**. **`--compile-plugin`** remains orthogonal: optional dylib that overrides **compile** only (operation/query), not catalog discovery.

``plugin_catalog`` builds an ``InMemoryCgsRegistry`` and runs template validation across entries.

## Session reuse and pinning

- **Compile plugin:** ``SessionReuseKey`` includes **`plugin_generation_id`** so a hot reload does not reuse a session tied to an older dylib generation.
- **Catalog:** the same key includes **`catalog_cgs_hash`** (and ``ExecuteSession`` stores **`catalog_cgs_hash`**) so HTTP reuse paths do not silently reuse a session after the **pinned CGS** for that entry changes (e.g. new plugin artifact or edited `domain.yaml`).

## Build

```bash
cargo build -p plasm-plugin-stub
# macOS: target/debug/libplasm_plugin_stub.dylib (also under target/debug/deps/ when built as a dependency)
# Linux: target/debug/libplasm_plugin_stub.so
```

The stub crate uses **`crate-type = ["cdylib", "rlib"]`** so `cargo test -p plasm-plugin-host` rebuilds the dylib used by loader tests (fresh copy under `target/debug/deps/`).

## HTTP / MCP examples

**Compile-only override** (same catalog as YAML/schema path):

```bash
cargo run -p plasm-server --release -- \
  --plugin-dir target/plasm-plugins \
  --compile-plugin target/debug/libplasm_plugin_stub.dylib
```

**Catalog from plugins** (from source; omit `--plugin-dir` when using the installer's `{appliance}/plugins`):

```bash
cargo run -p plasm-server --release -- --plugin-dir target/plasm-plugins
```

- **New** execute sessions pin the **current** plugin generation when `--compile-plugin` is set.
- **Session reuse** keys include **`plugin_generation_id`** and **`catalog_cgs_hash`** so reloads and catalog changes do not silently reuse old sessions.

## Plugin reload

The host builds an in-memory registry from disk at **startup**. After you replace `.so` files in `--plugin-dir`:

- **Hot reload (HTTP):** `POST /internal/plugin-registry/v1/reload` with header **`X-Plasm-Control-Plane-Secret`** when internal routes are enabled. Returns **`200`** after load + validation. **`409`** if the process was started with **`--schema`** instead of **`--plugin-dir`**.
- **Cold reload:** restart **`plasm-server`** so the process loads the directory again at startup.

## Genco direction (future)

A code generator would emit Rust sources that either:

- **Delegate** (like `plasm-plugin-stub`) for rapid iteration, or
- **Specialize** per capability (inline CML / filter logic) for lower latency,

then build a `cdylib` with the stable exports above. Register either:

- a single path via **`--compile-plugin`**, and/or
- a directory of versioned catalog plugins via **`--plugin-dir`**.

## Execute run artifacts (snapshots)

``RunArtifactStore`` backs **`GET /execute/.../artifacts/:run_id`** and MCP **`resources/read`** for `plasm://execute/…` URIs.

| Mode | Configuration |
|------|----------------|
| **In-memory** (default) | No env; snapshots are process-local and lost on restart. |
| **Object store** | **`PLASM_RUN_ARTIFACTS_URL`**: [`object_store::parse_url_opts`](https://docs.rs/object_store/latest/object_store/fn.parse_url_opts.html) URL (e.g. `s3://bucket/prefix`, `file:///path/to/dir`). Credentials and region follow each backend’s usual env vars. |
| **Time-based GC** (object store only) | **`PLASM_RUN_ARTIFACTS_RETENTION_SECS`** (default **604800** = 7 days): delete listed objects whose **`last_modified`** is older than this. **`PLASM_RUN_ARTIFACTS_GC_INTERVAL_SECS`** (default **300**): background sweep interval. |

Object keys: `{url-prefix}/execute/{prompt_hash}/{session_id}/{run_id}.json`.

## Persistent session graph cache (delta + snapshot)

`plasm` can persist session graph/runtime artifacts in an object-store-friendly shape:

|Mode|Configuration|
|----|-------------|
|**Disabled** (default)|No env; session graph state remains in-memory only.|
|**Object store deltas + snapshot manifest**|**`PLASM_GRAPH_CACHE_URL`**: [`object_store::parse_url_opts`](https://docs.rs/object_store/latest/object_store/fn.parse_url_opts.html) URL.|

Layout:

- Delta append objects: `{url-prefix}/v1/sessions/{prompt_hash}/{session_id}/delta/{seq}.bin`
- Snapshot object on session release: `{url-prefix}/v1/sessions/{prompt_hash}/{session_id}/snapshots/{through_seq}.bin`
- Snapshot manifest: `{url-prefix}/v1/sessions/{prompt_hash}/{session_id}/manifest.json`

Payloads are treated as arbitrary bytes with typed metadata (`content_type`, optional `content_encoding`, `schema_version`, `producer`); JSON is only one codec option.

## Operational backlog

- **PluginManager:** on each **`reload`**, the host clears prior entries from its generation map so dylibs can unload once no execute session still holds a pinned `Arc` to ``LoadedPluginGeneration``.
