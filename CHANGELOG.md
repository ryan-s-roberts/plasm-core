# Changelog

All notable changes to this OSS workspace are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
