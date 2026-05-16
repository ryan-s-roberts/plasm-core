# Releasing plasm-core (OSS)

Versions are **one SemVer** for the entire `plasm-oss` workspace (`[workspace.package] version` in the repo-root `Cargo.toml`).

## Cut a release

1. Update **`[workspace.package] version`** in `Cargo.toml` (and keep the parent monorepo root `Cargo.toml` `[workspace.package] version` in sync if you ship from both trees).
2. Update **`CHANGELOG.md`** under `[Unreleased]` → move notes under a `## [X.Y.Z]` heading with the release date.
3. Commit and push, then create an **annotated tag** `vX.Y.Z` pointing at that commit (`git tag -a vX.Y.Z -m "Release vX.Y.Z"`).
4. **Push the tag** to GitHub:
   - **plasm-core** (OSS submodule root): the **`release`** workflow ([`.github/workflows/release.yml`](.github/workflows/release.yml)) builds **`plasm`**, **`plasm-server`**, and **`plasm-cgs`** for Linux and macOS (x86_64 + aarch64) and uploads tarballs + `SHA256SUMS`.
   - **Private monorepo (`plasm`)** with CircleCI: on the same `v*.*.*` tag, after `validate` and `appliance_tui_pty` succeed, the **`oss_release_publish`** job runs **`scripts/ci/circle-oss-release.sh`** at the monorepo root. When **Docker Buildx** is on the runner, it exports **`linux/amd64`** and **`linux/arm64`** gnu tarballs via **`docker/plasm-stack.Dockerfile`** (`--target oss-release-bundle`, same rust-builder cross setup as image builds). On **macOS** runners it also builds the **host** triple with native **cargo**. If Docker is missing, it falls back to **one** native tarball. Assets upload to the OSS repo with **`SHA256SUMS`** merged so GitHub Actions–built artifacts stay listed.

## CircleCI (monorepo tag pipelines)

Configure a **project or context** environment variable:

- **`GH_TOKEN`** — classic PAT or fine-grained token with **Contents** + **Releases** on the OSS repo (default target: `ryan-s-roberts/plasm-core`).

Optional:

- **`PLASM_OSS_RELEASE_REPO`** — `owner/repo` if releases should go elsewhere.

The machine runner must have the **`gh`** CLI installed (the script uses `GH_TOKEN` from the environment). For the **Linux gnu pair**, install **Docker** with **buildx**; otherwise only the host `cargo` triple is published.

## Tag / version guard

`scripts/ci/verify-release-tag-matches-workspace-version.sh` fails the release job if `vA.B.C` ≠ `[workspace.package] version`.

## Install UX

Published binaries are listed on GitHub Releases. The marketing site ([plasm.tools](https://plasm.tools)) serves **`/install/oss-release.json`** and **`/install/install.sh`** (see the `plasm-portal` repo); update `oss-release.json` when cutting a release if portal automation is not wired yet.
