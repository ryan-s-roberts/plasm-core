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

Plus **`SHA256SUMS`** and **`oss-release.json`** (install manifest for `install.sh` and GitHub `releases/latest/download`).

**Targets:** `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` (**6** tarballs per release). **Linux arm64** and **Intel macOS** are not published — use Linux amd64 or Apple Silicon macOS builds, or build from source.

**Discovery:** prebuilt `plasm-server` / `plasm` binaries use **lexical-only** typed discovery (no ONNX / `fastembed` in the release graph). Optional local embedding rerank requires building from source with `--features local-embeddings` on `plasm-agent-core` / `plasm-discovery` (ONNX dev setup required).

The legacy unified `plasm-oss-*.tar.gz` is **no longer published**.

## Cut a release (single pipeline)

**Binaries** are published only by **CircleCI** on a **monorepo** annotated tag. **plasm-core** GHA [`.github/workflows/release.yml`](.github/workflows/release.yml) only verifies that the tag matches `[workspace.package] version` (optional tag on the OSS repo for the same check).

1. Bump **`[workspace.package] version`** in `plasm-oss/Cargo.toml` and the monorepo root `Cargo.toml`; update **`CHANGELOG.md`**.
2. Commit and push **plasm-oss** `main` (submodule pointer in monorepo).
3. Commit and push **monorepo** `main` with the updated submodule + version.
4. Run full CI on `main` (Circle **`ci`** workflow) before tagging.
5. Create and push **one tag** on the **monorepo** only:

   ```bash
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```

6. Watch Circle **`oss_release`** to completion:
   - **`oss_release_linux`** + **`oss_release_macos`** → [`circle-oss-release.sh`](../scripts/ci/circle-oss-release.sh) uploads tarballs + `SHA256SUMS` to [PlasmTools/plasm-core](https://github.com/PlasmTools/plasm-core/releases).
   - **`oss_publish_install_site`** → generates `oss-release.json`, uploads it to the GitHub release, pushes **plasm-portal** image, commits manifest to `main` (`[skip ci]`), optional cluster rollout, **live verify** (requires **`GH_TOKEN`**, **`VULTR_CONTAINER_KEY`**, optional **`KUBECONFIG`**).
   - **`release_build_and_push_vultr`** → remaining stack images (skips **plasm-portal**; already published).

7. Confirm install plane:

   ```bash
   curl -fsSL https://github.com/PlasmTools/plasm-core/releases/latest/download/oss-release.json | jq .version
   curl -fsSL https://plasm.tools/install/install.sh | bash -s -- --dry-run
   ```

**Manual recovery** (if a step failed):

```bash
PLASM_INSTALL_SITE_PUSH=1 PLASM_INSTALL_PORTAL_PUSH=1 PLASM_INSTALL_VERIFY_LIVE=1 \
  bash scripts/ci/publish-oss-install-site.sh vX.Y.Z --git --portal
bash scripts/k8s/rollout-plasm-portal.sh   # when KUBECONFIG is set
```

Or re-run monorepo GHA [`.github/workflows/oss-install-site.yml`](../.github/workflows/oss-install-site.yml) via **workflow_dispatch** (requires **`VULTR_CONTAINER_KEY`** secret).

## CircleCI (monorepo tag pipelines)

Monorepo CircleCI uses two workflows: **`ci`** (branch pushes — `validate` + `appliance_tui_pty` + Vultr bake) and **`oss_release`** (version tags only — no full test suite). Install-manifest commits from `publish-oss-install-site.sh` include **`[skip ci]`** so post-release JSON sync does not re-run `validate`. Tag pushes should not duplicate the same `cargo nextest` + `mix test` job that already ran on `main` before the release tag.

Configure a **project or context** environment variable:

- **`GH_TOKEN`** — PAT with **Contents** + **Releases** on `PlasmTools/plasm-core` (release tarballs + `oss-release.json` upload).
- **`PLASM_MONOREPO_GH_TOKEN`** (recommended) — PAT with **Contents** write on `ryan-s-roberts/plasm` for `[skip ci]` install manifest commits. Without it, Circle sets `PLASM_INSTALL_GIT_PUSH_OPTIONAL=1` (GitHub release manifest + portal image still publish).

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

After tarballs land on GitHub Releases, Circle **`oss_publish_install_site`** runs [`scripts/ci/publish-oss-install-site.sh`](../scripts/ci/publish-oss-install-site.sh) (manifest → GitHub asset → portal image → git commit). **`install.sh`** defaults to the GitHub release manifest so installers work even before the portal image rolls out; `https://plasm.tools/install/oss-release.json` is kept in sync via the portal image bake.

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
