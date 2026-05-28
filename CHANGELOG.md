# Changelog

All notable changes to this OSS workspace are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.46] - 2026-05-28

### Added

- **`plasm-server`:** auto headless mode when stdout or stdin is not a TTY; `--tui` forces the Ratatui control station; `--no-tui` unchanged.

## [0.1.45] - 2026-05-28

### Added

- **Remote `plasm` CLI:** compressed workspace layout (`.plasm/hosts/<8hex>/`, `.plasm/s/<8hex>/`) and append-only `out/NNNN-{search,context,plan,run}/` mirror archive with dual `body.json` + `body.txt` (and `artifact.*` on live runs).
- **Remote `plasm` CLI:** device OAuth login (`plasm login`, platform `plasm init`), pwd-local profiles, typed `context`/`run` flags, `incoming_auth_device` HTTP helpers.

### Changed

- **Breaking (local state):** drop `.plasm/cgs/` tree — remove `.plasm` once after upgrade; no migration.

## [0.1.44] - 2026-05-28

### Fixed

- **Release CI (monorepo):** `verify-vultr-release-images` accepts manifest-list/OCI index media types (buildx pushes) and polls until tags appear in Vultr CR.

## [0.1.43] - 2026-05-28

### Fixed

- **Release packaging:** monorepo `deploy/packaged-apis.txt` drops nonexistent `teams` entry and duplicate `outlook` so `plasm-pack-plugins` completes in Docker release builds.

## [0.1.42] - 2026-05-27

### Fixed

- **apis/fibery:** restore complete `domain.yaml` (entities, capabilities, views, `schema_overlay`) so `plasm-pack-plugins` can load the catalog in release Docker builds.

### Added

- **Runtime schema overlay:** unified `schema_overlay:` spec in `domain.yaml` — host fetches workspace schema at execute session open and merges typed entities/columns into the session CGS (`effective_catalog_cgs_hash_hex`).
- **API-driven multi-fetch pipeline:** `source.steps` with `collect` → `for_each` (row-driven `bind`) → `merge` for scoped schema endpoints (ClickUp `team_query` → `custom_field_query`; Jira `project_query` → `issue_createmeta_get`).
- **Projection modes:** `per_scope_entity` (Fibery, Notion, Jira) and `augment_base` (ClickUp custom fields on `Task`); Minijinja filters `join_sanitize`, `sanitize_identifier`.
- **Catalog overlays:** Fibery, Notion, ClickUp, Jira `schema_overlay` blocks; Linear overlay deferred (no public custom-field definition query).
- **Session resolver:** `schema_overlay_session` wired at HTTP execute, MCP `plasm_context`, federated attach, and local `plasm-repl`.

### Changed

- **Overlay configuration is API-only:** removed HTTP `overlay_scope`, MCP seed `scope`, and client/env `source.bind` for overlay — session auth + catalog-declared pipeline only.
- **Authoring skills / catalog READMEs:** document API-driven overlay pattern and multi-fetch for scoped schema APIs.

## [0.1.41] - 2026-05-27

### Fixed

- **Release CI (monorepo):** semver `publish-release vX.Y.Z` is tag-only; `release_ship` verifies Vultr images exist before bumping `deploy/values/dev/images.yaml`, avoiding `ImagePullBackOff` when deploy refs lead the registry.

## [0.1.40] - 2026-05-27

### Fixed

- **apis/linear (v10):** `IssueContext` / `issue_navigation_link` view Gets; `comment_by_issue_query` filters by issue UUID or identifier; `Issue` / `IssueContext.comments` relation materialize; `team_get` GraphQL `key` variable.
- **Runtime:** parameterless view Gets (`user_viewer` / `MyWorkSnapshot`); Get bind `id` aliases entity `id_field`.
- **Parser / planner:** `Issue.search(…)` sugar; brace filters on Search-only entities resolve to `issue_search`; surface parse normalizes `capability_name` so dry-run plans match live execution.
- **MCP:** no-op `plasm_context` expand includes compact expression-syntax hints.

## [0.1.39] - 2026-05-27

### Added

- **preflight:** typed capability `preflight` steps (full cutover from `invoke_preflight`) — `hydrate_invoke_target`, `hydrate_entity_ref_param`, `query_pick`, `label_ids_delta`; runtime press on create/invoke before CML merge.
- **apis/linear:** task-oriented catalog (v9) aligned with [linear/linear#1035](https://github.com/linear/linear/issues/1035) — `issue_search` with team/state/assignee **names**, `IssueContext` / `MyWorkSnapshot` views, consolidated `issue_create` / `issue_update` with name→ID preflight, `user_search`, unified comment `issue` entity_ref.
- **Catalog:** Gmail, Google Drive, Discord, and Grafana capabilities migrated to `preflight` hydrate steps.

### Changed

- **plasm-server:** typed Logs tab (level colors, compact timestamps); Clients tab MCP JSON display and copy.

## [0.1.38] - 2026-05-27

### Fixed

- **MCP sqlx migrate:** prune squashed `_sqlx_migrations` ledger rows (e.g. `20260216120000`) before embedded migrate so init containers succeed on upgraded clusters.

## [0.1.37] - 2026-05-27

### Fixed

- **Docker bake:** post-push ELF verify works in `debian:*-slim` images (no `file(1)` dependency).

## [0.1.36] - 2026-05-27

### Fixed

- **Docker bake:** export `PLASM_HOST_TARGET_TRIPLE` for `plasm-agent-core` when `cargo chef` skips `build.rs` output.

## [0.1.35] - 2026-05-27

### Fixed

- **Docker cross bake:** restore multiarch OpenSSL sysroot for `auth-framework` reqwest native-tls; `oauth2` / `opentelemetry-otlp` use rustls.

## [0.1.34] - 2026-05-26

### Fixed

- **Docker bake:** canonical cargo-chef order (cook deps before app source); rustls-only `reqwest`; cross arm64→amd64 on M-series Mac CI without OpenSSL cross deps.

## [0.1.33] - 2026-05-26

### Fixed

- **Docker bake:** reject stub `plasm-mcp` / `plasm-trace-sink` ELFs (size + `file(1)` arch) in `rust-builder` and post-bake verify; harden cross-compile artifact paths after `cargo chef cook`.

## [0.1.32] - 2026-05-26

### Fixed

- **Release CI:** forbid k3d `plasm-argocd-sync` job push to `localhost:5000` on CircleCI (force Argo Git sync / kubectl mode).

## [0.1.31] - 2026-05-26

### Fixed

- **Release CI:** `portal-release-finalize.sh` always bakes/pushes/rollouts portal after Argo sync; tag guard rejects broken `v0.1.30` release_ship checkouts.

## [0.1.30] - 2026-05-25

### Fixed

- **plasm-server TUI:** Clear each frame, handle terminal resize, taller tab rail, display-width clipping for catalogue/API rows; bootstrap supervisor messages go to Logs tab (tracing) instead of painting over the footer.

## [0.1.29] - 2026-05-25

### Fixed

- **plasm.tools/get:** release pill reads GitHub `oss-release.json`; portal image cache bust on manifest version.

## [0.1.28] - 2026-05-25

### Fixed

- **CI:** `git-checkout-main.sh` stashes install manifest before checkout (tag release ship phase 3).

## [0.1.27] - 2026-05-25

### Fixed

- **CI:** Source `ensure-kubeconfig-env` (subshell dropped KUBECONFIG under zsh -il); reject EKS/cicd-cluster.

## [0.1.26] - 2026-05-25

### Fixed

- **CI:** Flat Circle config (no custom commands); use workflow **release** / job **release_ship**, not legacy **oss_publish_install_site**.

## [0.1.25] - 2026-05-25

### Fixed

- **CI:** Circle `zsh_run` command parameters (`step_name` / `run_command`; `name`/`command` are reserved).

## [0.1.24] - 2026-05-25

### Changed

- **CI:** Consolidated Circle workflows (`ci` + `release`) and orchestrator scripts (`circle-test`, `circle-dev-deploy`, `circle-release-ship`).

## [0.1.23] - 2026-05-25

### Fixed

- **CI:** `rollout-plasm-portal` bootstraps Argo `plasm-portal` when Deployment is missing; sanity-check VKE cluster.

## [0.1.22] - 2026-05-25

### Fixed

- **CI:** `ensure-kubeconfig-env` prefers `~/.kube/plasm-vke.yaml` over default `~/.kube/config` for portal rollout.

## [0.1.21] - 2026-05-25

### Changed

- **CI:** SaaS deploy on tag (`saas_publish_deploy_ref`); kubeconfig discovery on self-hosted runner; always run tests on `main`.

## [0.1.20] - 2026-05-25

### Changed

- **CI:** CircleCI project re-linked for **PlasmTools/plasm** (`oss_release` on tag).

## [0.1.19] - 2026-05-25

### Changed

- **Docs:** Canonical release process on **PlasmTools/plasm** [`RELEASING.md`](https://github.com/PlasmTools/plasm/blob/main/RELEASING.md); this repo stubs only.

## [0.1.18] - 2026-05-25

### Changed

- **CI:** `publish_portal_site` in `ci` workflow on `main`; Circle project docs for **PlasmTools/plasm**.

## [0.1.17] - 2026-05-24

### Changed

- **CI / docs:** Monorepo canonical GitHub org is **`PlasmTools/plasm`** (install publish, Argo `track.json`, release secrets).

## [0.1.16] - 2026-05-24

### Fixed

- **CI:** Portal image publish uses `docker build` + Vultr push retries instead of buildx bake (504 blob upload timeouts).

## [0.1.15] - 2026-05-24

### Fixed

- **CI:** Install publish requires `PLASM_MONOREPO_GH_TOKEN` (no optional git-push skip on Circle).

## [0.1.14] - 2026-05-24

### Fixed

- **CI:** Circle install publish uses `PLASM_MONOREPO_GH_TOKEN` for monorepo git push (avoids 403 from plasm-core-only `GH_TOKEN`).

## [0.1.13] - 2026-05-24

### Fixed

- **CI:** `circle-oss-release` no longer `cp` SHA256SUMS onto itself when refreshing an existing release (macOS `cp` exit 1).
- **CI:** Linux OSS release job skips checksum upload; macOS job runs after Linux and merges `SHA256SUMS` once (avoids parallel clobber).

### Changed

- **CI:** Coherent monorepo-tag install pipeline (GitHub manifest default, Circle `oss_publish` gate, GHA install recovery only).

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
