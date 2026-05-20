# Changelog

All notable changes to this OSS workspace are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-20

### Added

- **OSS release train:** three tarballs per platform — `plasm-appliance` (server + API plugins), `plasm` (HTTP client), `plasm-cgs` (dev CLI) — on [GitHub Releases](https://github.com/PlasmTools/plasm-core/releases).
- **Install microsite** sources at `get.plasm.tools` (`install.sh`, `oss-release.json`); generator `scripts/ci/generate-oss-release-json.sh`.
- **CI:** GitHub Actions `release.yml` matrix; CircleCI `oss_release_linux` + `oss_release_macos` (monorepo).

### Changed

- **Binary names:** remote HTTP terminal is now **`plasm`** (`plasm`, `--bin plasm`); the local appliance binary is **`plasm-server`** (Cargo package **`plasm-server`**, directory `crates/plasm-server`; formerly **`plasm-appliance`**); the dev/schema CLI is **`plasm-cgs`** (`plasm-cli`, `--bin plasm-cgs`). Former names: `plasm-cgs` (agent), `plasm-appliance`, `plasm` (cli).
- **Workspace versions:** all `plasm-oss` crates use `version.workspace = true` with a single `[workspace.package] version` in the root `Cargo.toml`.
- **Deprecated:** unified `plasm-oss-*.tar.gz` release archives (replaced by product-specific tarballs above).
