#!/usr/bin/env bash
# Generate get.plasm.tools/oss-release.json from a GitHub Release on plasm-core.
#
# Usage:
#   generate-oss-release-json.sh <tag> [output.json]
#
# Requires: gh, python3
# Env: GH_TOKEN or gh auth; PLASM_OSS_RELEASE_REPO (default PlasmTools/plasm-core)

set -euo pipefail

tag="${1:?generate-oss-release-json: tag required (e.g. v0.2.0)}"
out="${2:-}"

REPO="${PLASM_OSS_RELEASE_REPO:-PlasmTools/plasm-core}"
ver="${tag#v}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "${tmpdir}"' EXIT

gh release download "${tag}" --repo "${REPO}" -p SHA256SUMS -D "${tmpdir}" 2>/dev/null || true
gh release view "${tag}" --repo "${REPO}" --json assets,publishedAt >"${tmpdir}/release.json"

python3 - "${ver}" "${tag}" "${REPO}" "${tmpdir}/SHA256SUMS" "${tmpdir}/release.json" "${out}" <<'PY'
import json, re, sys, os
from collections import defaultdict

ver, tag, repo, sums_path, release_path, out_path = sys.argv[1:7]
release = json.load(open(release_path))
published_at = release.get("publishedAt", "")
assets = release.get("assets") or []

checksums = {}
if os.path.isfile(sums_path):
    for line in open(sums_path):
        line = line.strip()
        if not line:
            continue
        parts = line.split(None, 1)
        if len(parts) == 2:
            checksums[parts[1].strip()] = parts[0].strip()

triple_re = re.compile(
    r"^plasm(?:-appliance|-cgs)?-" + re.escape(ver) + r"-(.+)\.tar\.gz$"
)

by_triple = defaultdict(dict)

for a in assets:
    name = a.get("name") or ""
    m = triple_re.match(name)
    if not m:
        continue
    triple = m.group(1)
    if name.startswith("plasm-appliance-"):
        product = "appliance"
    elif name.startswith("plasm-cgs-"):
        product = "cgs"
    elif name.startswith(f"plasm-{ver}-"):
        product = "client"
    else:
        continue
    by_triple[triple][product] = name
    by_triple[triple].setdefault("sha256", {})[product] = checksums.get(name, "")

asset_rows = []
for triple in sorted(by_triple.keys()):
    row = {"triple": triple, **by_triple[triple]}
    asset_rows.append(row)

manifest = {
    "version": ver,
    "tag": tag,
    "repo": repo,
    "published_at": published_at,
    "products": {
        "appliance": {
            "prefix": "plasm-appliance",
            "bins": ["plasm-server"],
            "dirs": ["plugins"],
        },
        "client": {"prefix": "plasm", "bins": ["plasm"]},
        "cgs": {"prefix": "plasm-cgs", "bins": ["plasm-cgs"]},
    },
    "assets": asset_rows,
    "checksums_file": "SHA256SUMS",
    "install_script": "https://get.plasm.tools/install.sh",
    "download_base": f"https://github.com/{repo}/releases/download/{tag}",
}

text = json.dumps(manifest, indent=2) + "\n"
if out_path:
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(text)
    print(f"generate-oss-release-json: wrote {out_path}", file=sys.stderr)
else:
    sys.stdout.write(text)
PY
