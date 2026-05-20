# Releasing plasm-core (OSS)

Versions are **one SemVer** for the entire `plasm-oss` workspace (`[workspace.package] version` in the repo-root `Cargo.toml`).

## Release artifacts

Each supported Rust triple gets **three** tarballs on [GitHub Releases](https://github.com/PlasmTools/plasm-core/releases):

| Asset | Contents |
|-------|----------|
| `plasm-appliance-{triple}.tar.gz` | `plasm-server` + `plugins/` (ABI v4 cdylibs from [`scripts/oss-packaged-apis.txt`](scripts/oss-packaged-apis.txt)) |
| `plasm-{triple}.tar.gz` | `plasm` remote HTTP terminal client |
| `plasm-cgs-{triple}.tar.gz` | `plasm-cgs` schema/dev CLI |

SemVer is on the **Git tag** only (e.g. `v0.1.1`), not repeated in asset filenames.

Plus **`SHA256SUMS`** for all assets.

**Targets:** `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin` (**9** tarballs per release). Intel macOS (`x86_64-apple-darwin`) is not published — `ort`/ONNX has no prebuilt for that triple; use the Apple Silicon build under Rosetta or build from source.

The legacy unified `plasm-oss-*.tar.gz` is **no longer published**.

## Cut a release

1. Update **`[workspace.package] version`** in `Cargo.toml` (and keep the parent monorepo root `Cargo.toml` `[workspace.package] version` in sync if you ship from both trees).
2. Update **`CHANGELOG.md`** under `[Unreleased]` → move notes under a `## [X.Y.Z]` heading with the release date.
3. Commit and push, then create an **annotated tag** `vX.Y.Z` pointing at that commit (`git tag -a vX.Y.Z -m "Release vX.Y.Z"`).
4. **Push the tag** to GitHub:
   - **plasm-core** (OSS repo): workflow [`.github/workflows/release.yml`](.github/workflows/release.yml) builds all four triples natively and publishes three tarballs per triple + `SHA256SUMS`.
   - **Private monorepo (`plasm`)** with CircleCI: on tag `v*.*.*`, after `validate` and `appliance_tui_pty`:
     - **`oss_release_linux`** — Docker Buildx `linux/amd64` + `linux/arm64` via [`docker/plasm-stack.Dockerfile`](../docker/plasm-stack.Dockerfile) `--target oss-release-bundle` (same rust-builder graph as production images).
     - **`oss_release_macos`** — native `cargo` on a **Darwin** machine runner (host triple only; use a second runner or rely on GHA for the other macOS arch).
     - Both run [`scripts/ci/circle-oss-release.sh`](../scripts/ci/circle-oss-release.sh) and **merge** `SHA256SUMS` into the same GitHub release (`--clobber` uploads).
5. **Install microsite:** regenerate and deploy [`get-plasm-tools/`](../get-plasm-tools/) (see below).

## CircleCI (monorepo tag pipelines)

Configure a **project or context** environment variable:

- **`GH_TOKEN`** — PAT with **Contents** + **Releases** on the OSS repo (default: `PlasmTools/plasm-core`).

Optional:

- **`PLASM_OSS_RELEASE_REPO`** — `owner/repo` if releases should go elsewhere.
- **`PLASM_OSS_RELEASE_SCOPE`** — `linux` | `macos` | `all` (set by split jobs; default `all` when invoking the script manually).

Runner requirements:

- **`gh`** CLI on all release jobs.
- **Linux job:** Docker with **buildx**.
- **macOS job:** Darwin host (job no-ops on Linux runners).

## Tag / version guard

[`scripts/ci/verify-release-tag-matches-workspace-version.sh`](scripts/ci/verify-release-tag-matches-workspace-version.sh) fails the release job if `vA.B.C` ≠ `[workspace.package] version`.

## Install UX (get.plasm.tools)

Static install content lives in the monorepo at [`get-plasm-tools/`](../get-plasm-tools/) (deploy to **`https://get.plasm.tools`**):

| URL | File |
|-----|------|
| `/` | `index.html` |
| `/install.sh` | `install.sh` |
| `/oss-release.json` | `oss-release.json` |

After CI finishes the GitHub release:

```bash
# From monorepo root (requires gh + python3):
bash scripts/ci/generate-oss-release-json.sh vX.Y.Z get-plasm-tools/oss-release.json

# From plasm-core checkout only:
bash scripts/ci/generate-oss-release-json.sh vX.Y.Z /path/to/oss-release.json
```

Commit and deploy the updated manifest. Install examples:

```bash
# Appliance (default): plasm-server + plugins
curl -fsSL https://get.plasm.tools/install.sh | bash

# Remote HTTP client only
curl -fsSL https://get.plasm.tools/install.sh | bash -s -- --product client

# Schema CLI
curl -fsSL https://get.plasm.tools/install.sh | bash -s -- --product cgs
```

Platform notes: [`docs/oss-binary-platforms.md`](../docs/oss-binary-platforms.md).

## Native packaging (local / CI)

[`scripts/ci/oss-release-pack-native.sh`](scripts/ci/oss-release-pack-native.sh) — `cargo build --release` then pack three tarballs (used by Circle macOS and GHA).
