# Changelog

All notable changes to this OSS workspace are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.12] - 2026-05-24

### Fixed

- **`plasm-runtime`:** `PaginationConfig` unit tests set `response_next_url_field` (OData `nextLink` field).
- **`plasm-trace-sink`:** `http_iceberg_integration` passes segment-projection TTL args to `PersistedTraceSink::connect`.
- **`apis/cloudflare`**, **`apis/grafana`:** declare scope parameters on `zone_get` / `dashboard_get` used by view node binds.
- **`fixtures/plasm_prompt_matrix`:** mirror `zone_get` `zone_id` for `security_overview` CGS validation.

### Changed

- **`plasm-core`:** refresh Linear full-prompt insta snapshot (cycle board view, issue URL view, teaching-table symbols).

## [0.1.11] - 2026-05-24

### Added

- **`plasm-trace-sink`:** Postgres `trace_segments` projection for hot trace detail reads with configurable TTL/GC.
- **`plasm-trace-sink`:** Head-guided `year_month_bucket` Iceberg pruning on cold detail reads (`event_kind` filter + empty-scan retry).
- **API catalogs:** Microsoft Graph–backed Gmail, Jira, and Linear packages with OData `nextLink` pagination.
- **`plasm-runtime` / `plasm-core`:** View origin injection and inner-node template binds; language-matrix conformance for computed view fields.

### Changed

- **`plasm-agent-core`:** Shared `reqwest::Client` for trace-sink HTTP proxy calls.

## [0.1.10] - 2026-05-23

### Fixed

- **CI / quality:** `cargo clippy --workspace --all-targets -- -D warnings` clean (integration Postgres keep-alive holder, TUI `UiMsg::Admin` boxing, assorted clippy nits).

## [0.1.9] - 2026-05-23

### Fixed

- **Release:** declare `rayon`, `criterion`, and `aho-corasick` in the OSS workspace `Cargo.toml` so standalone `plasm-core` CI builds succeed (v0.1.8 tag missed these entries).

## [0.1.8] - 2026-05-23

### Added

- **Performance:** Criterion benches for CGS load and typed discovery index (`plasm-core/benches/schema_load`, `plasm-discovery/benches/index_build`).
- **Performance:** `CatalogIndexCache` on the agent host; OTEL `plasm.discovery.index_cache_total`.
- **Performance:** `PLASM_CGS_FAST_LOAD=1` skips expression-surface DOMAIN bundle synthesis at load (structural validate only).
- **Performance:** `PLASM_DISCOVERY_EMBED_CONCURRENCY` env for shared ONNX embedder pool sizing.

### Changed

- **Performance:** Cache `catalog_cgs_hash_hex` on `CGS` via `OnceLock`; store hash in registry metadata at insert.
- **Performance:** Aho-Corasick substring scan for typed discovery; parallel entity index build (`rayon`).
- **Performance:** Incremental Postgres embedding reconcile (missing-line upsert + stale-line delete, no full delete/refill).
- **Performance:** Move capability mappings at assemble time (`swap_remove`); single `finalize_cgs_load` in pack-plugins.
- **Performance:** Parallel legacy capability scoring per catalog entry.

## [0.1.7] - 2026-05-23

### Fixed

- **`plasm-server`:** squash `plasm-agent-core` sqlx to one idempotent migration (`20260601000000_plasm_agent_schema`); drop ledger repair so fresh embedded Postgres boots cleanly.
- **`plasm-server`:** typed MCP policy attach (`McpPolicyAttachOutcome`), appliance bootstrap gate, and scrollable Overview with `config_surface_from_host` at RUN handoff (no garbled trace-hub / `enabledts` overlap).

## [0.1.6] - 2026-05-23

### Added

- **`apis/grafana`:** v5 catalog (core API, RBAC, datasource explorers, Sift/Incident/OnCall plugins, assembled deeplink `url`, panel render/query).
- **`plasm-core` / `plasm-runtime`:** view `output` bindings with `kind: computed` (Minijinja); optional `views.scope` `required:`; `wire_temporal_value` and view-template filters (`wire_time`, `urlencode`, `wire_query_suffix`, …).
- **Conformance:** `plasm_language_matrix_views` computed field `echo_slug`.

### Changed

- **`apis/cloudflare`:** derive `security_surface_status` in `views.security_overview` (domain v13); remove `SecurityOverview` hardcoded derivation from `view_execution`.

## [0.1.5] - 2026-05-23

### Fixed

- **`plasm-server`:** reconcile appliance DB env after `.env` load so embedded PostgreSQL autostarts and `project_mcp_*` sqlx migrations run on first launch (no manual `mcp migrate-db` when a cwd `.env` sets `DATABASE_URL`).
- **`plasm-server`:** fatal bootstrap when embedded PG started but MCP policy store did not attach; Status tab shows concrete errors (ASCII markers, no stderr corruption during alternate-screen TUI).
- **Embedded Postgres:** set `PLASM_MCP_CONFIG_DATABASE_URL` alongside `DATABASE_URL` / `PLASM_AUTH_STORAGE_URL` on autostart.

### Changed

- **`plasm-runtime`:** apply request-identity override for entity decoders when a row id is present (not only `implicit_request_identity` entities).

## [0.1.4] - 2026-05-21

### Changed

- **`plasm-server`:** default appliance root to `~/.plasm/appliance` (or `PLASM_APPLIANCE_DIR`); auto `--plugin-dir` when `{appliance}/plugins` exists so `plasm-server` runs without flags after the OSS installer layout.

## [0.1.3] - 2026-05-21

### Changed

- **OSS release binaries:** typed discovery is **lexical-only** (`fastembed` / ONNX behind Cargo feature `local-embeddings`; not linked in CI release builds). `enable_embeddings` defaults **false**; release MCP schema documents the constraint.
- **Release CI:** remove ONNX Runtime `brew install` from GHA and Circle macOS Intel legs (no longer required for packaging).

## [0.1.2] - 2026-05-21

### Changed

- **OSS release platforms:** `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin` (drop Linux arm64). Intel macOS links ONNX Runtime via Homebrew in CI.

## [0.1.1] - 2026-05-21

### Fixed

- **Release CI:** Docker `BUILDPLATFORM`/`TARGETPLATFORM` for Circle Linux builds; native pack uses monorepo `target/` when built from the private repo.
- **GHA:** drop Intel macOS prebuilds (`ort` has no `x86_64-apple-darwin` ONNX); publish aarch64 Apple Silicon only.

### Changed

- **Release asset names:** drop SemVer from tarball filenames (version is the Git tag only), e.g. `plasm-appliance-x86_64-unknown-linux-gnu.tar.gz`.

### Note

- **v0.1.0** shipped only GitHub source archives (no product binaries); use **v0.1.1** or later for downloads.

## [0.1.0] - 2026-05-20

### Added

- **OSS release train:** three tarballs per platform — `plasm-appliance` (server + API plugins), `plasm` (HTTP client), `plasm-cgs` (dev CLI) — on [GitHub Releases](https://github.com/PlasmTools/plasm-core/releases).
- **Install microsite** sources at `get.plasm.tools` (`install.sh`, `oss-release.json`); generator `scripts/ci/generate-oss-release-json.sh`.
- **CI:** GitHub Actions `release.yml` matrix; CircleCI `oss_release_linux` + `oss_release_macos` (monorepo).

### Changed

- **Binary names:** remote HTTP terminal is now **`plasm`** (`plasm`, `--bin plasm`); the local appliance binary is **`plasm-server`** (Cargo package **`plasm-server`**, directory `crates/plasm-server`; formerly **`plasm-appliance`**); the dev/schema CLI is **`plasm-cgs`** (`plasm-cli`, `--bin plasm-cgs`). Former names: `plasm-cgs` (agent), `plasm-appliance`, `plasm` (cli).
- **Workspace versions:** all `plasm-oss` crates use `version.workspace = true` with a single `[workspace.package] version` in the root `Cargo.toml`.
- **Deprecated:** unified `plasm-oss-*.tar.gz` release archives (replaced by product-specific tarballs above).
