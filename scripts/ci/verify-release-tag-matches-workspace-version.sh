#!/usr/bin/env bash
# Fail if the release tag (e.g. v0.2.0) does not match [workspace.package].version in this repo's Cargo.toml.
# Intended for GitHub Actions (`GITHUB_REF_NAME`) and CircleCI (`CIRCLE_TAG`) on tag pushes.
# plasm-core OSS checkout: this script lives under `plasm-oss/scripts/ci/` (workspace root = `plasm-oss/`).

set -euo pipefail

ref="${GITHUB_REF_NAME:-${CIRCLE_TAG:-}}"
if [[ -z "$ref" ]]; then
  echo "verify-release-tag: set GITHUB_REF_NAME or CIRCLE_TAG (e.g. v0.2.0)" >&2
  exit 2
fi

if [[ "$ref" != v* ]]; then
  echo "verify-release-tag: expected tag vX.Y.Z, got ${ref}" >&2
  exit 2
fi

ver="${ref#v}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cargo_toml="${root}/Cargo.toml"

if [[ ! -f "$cargo_toml" ]]; then
  echo "verify-release-tag: missing ${cargo_toml}" >&2
  exit 2
fi

# First `version = "..."` after [workspace.package] (workspace root Cargo.toml).
parsed="$(awk '
  $0 == "[workspace.package]" { inpkg=1; next }
  inpkg && $0 ~ /^version = / {
    gsub(/^version = "/, "");
    gsub(/"$/, "");
    print;
    exit
  }
  inpkg && $0 ~ /^\[/ { exit }
' "$cargo_toml")"

if [[ -z "$parsed" ]]; then
  echo "verify-release-tag: could not read [workspace.package] version from ${cargo_toml}" >&2
  exit 2
fi

if [[ "$parsed" != "$ver" ]]; then
  echo "verify-release-tag: tag ${ref} expects workspace version ${ver}, but Cargo.toml has ${parsed}" >&2
  exit 1
fi

echo "verify-release-tag: OK tag ${ref} matches workspace version ${parsed}"
