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

**Targets:** `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin` (**9** tarballs per release). **Linux arm64** (`aarch64-unknown-linux-gnu`) is not published — use the x86_64 Linux build on amd64 hosts/containers.

**Discovery:** prebuilt `plasm-server` / `plasm` binaries use **lexical-only** typed discovery (no ONNX / `fastembed` in the release graph). Optional local embedding rerank requires building from source with `--features local-embeddings` on `plasm-agent-core` / `plasm-discovery` (ONNX dev setup required).

The legacy unified `plasm-oss-*.tar.gz` is **no longer published**.

## Cut a release

1. Update **`[workspace.package] version`** in `Cargo.toml` (and keep the parent monorepo root `Cargo.toml` `[workspace.package] version` in sync if you ship from both trees).
2. Update **`CHANGELOG.md`** under `[Unreleased]` → move notes under a `## [X.Y.Z]` heading with the release date.
3. Commit and push, then create an **annotated tag** `vX.Y.Z` pointing at that commit (`git tag -a vX.Y.Z -m "Release vX.Y.Z"`).
4. **Push the tag** to GitHub:
   - **plasm-core** (OSS repo): workflow [`.github/workflows/release.yml`](.github/workflows/release.yml) builds all four triples natively and publishes three tarballs per triple + `SHA256SUMS`. When `PLASM_MONOREPO_DISPATCH_TOKEN` is configured, it also triggers the monorepo **OSS install site** workflow so `plasm.tools/get` updates without a manual step.
   - **Private monorepo (`plasm`)** with CircleCI: on tag `v*.*.*`, after `validate` and `appliance_tui_pty`:
     - **`oss_release_linux`** — Docker Buildx `linux/amd64` via [`docker/plasm-stack.Dockerfile`](../docker/plasm-stack.Dockerfile) `--target oss-release-bundle`.
     - **`oss_release_macos`** — native `cargo` on a **Darwin** machine runner (host triple only).
     - Both run [`scripts/ci/circle-oss-release.sh`](../scripts/ci/circle-oss-release.sh) and **merge** `SHA256SUMS` into the same GitHub release (`--clobber` uploads).
     - **`oss_publish_install_site`** (after both OSS release jobs) — [`scripts/ci/publish-oss-install-site.sh`](../scripts/ci/publish-oss-install-site.sh): regenerate `oss-release.json`, commit to `main`, push **`plasm-portal`** so [plasm.tools/get](https://plasm.tools/get/) matches the release.
     - **`release_build_and_push_vultr`** — full image bake from updated `main` (includes portal with fresh manifest).
5. **Manual fallback** (if CI dispatch is unavailable):

```bash
bash scripts/ci/publish-oss-install-site.sh vX.Y.Z --git --portal
```

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

## Install UX (plasm.tools)

Install plane is deployed from **[`portal/`](../portal/)** (Kubernetes `plasm-portal` chart). Manifest sources are generated in this monorepo at [`get-plasm-tools/`](../get-plasm-tools/) and copied into `portal/public/install/`.

| URL | Path in repo |
|-----|----------------|
| `https://plasm.tools/get/` | `portal/public/get/index.html` |
| `https://plasm.tools/install/install.sh` | `portal/public/install/install.sh` |
| `https://plasm.tools/install/oss-release.json` | `portal/public/install/oss-release.json` |

After CI finishes the GitHub release, **`oss_publish_install_site`** (CircleCI tag pipeline) or the monorepo GHA workflow **OSS install site** (dispatched from plasm-core) runs [`scripts/ci/publish-oss-install-site.sh`](../scripts/ci/publish-oss-install-site.sh) automatically.

Manual fallback:

```bash
# From monorepo root (requires gh + python3):
bash scripts/ci/publish-oss-install-site.sh vX.Y.Z --git --portal
```

Install examples:

```bash
# Default: plasm-server + plugins, plasm, and plasm-cgs
curl -fsSL https://plasm.tools/install/install.sh | bash

# Single product (optional)
curl -fsSL https://plasm.tools/install/install.sh | bash -s -- --product client
curl -fsSL https://plasm.tools/install/install.sh | bash -s -- --product cgs
```

Platform notes: [`docs/oss-binary-platforms.md`](../docs/oss-binary-platforms.md).

## Native packaging (local / CI)

[`scripts/ci/oss-release-pack-native.sh`](scripts/ci/oss-release-pack-native.sh) — `cargo build --release` then pack three tarballs (used by Circle macOS and GHA).
