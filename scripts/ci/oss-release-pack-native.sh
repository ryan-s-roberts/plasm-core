#!/usr/bin/env bash
# Pack three OSS release tarballs from a completed `cargo build --release` in the workspace root.
# Usage: oss-release-pack-native.sh <version> <rust-triple> <output-dir>
#
# Writes:
#   plasm-appliance-<ver>-<triple>.tar.gz  (plasm-server + plugins/)
#   plasm-<ver>-<triple>.tar.gz            (plasm client)
#   plasm-cgs-<ver>-<triple>.tar.gz        (plasm-cgs)

set -euo pipefail

ver="${1:?oss-release-pack-native: version required}"
triple="${2:?oss-release-pack-native: rust triple required}"
out_dir="${3:?oss-release-pack-native: output dir required}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"

release_dir="${CARGO_TARGET_DIR:-target}/release"
pack_root="$(mktemp -d)"
trap 'rm -rf "${pack_root}"' EXIT

pack_plugins() {
  local plugins_dir="${pack_root}/plugins"
  mkdir -p "${plugins_dir}"
  local list="${root}/scripts/oss-packaged-apis.txt"
  if [[ ! -f "${list}" ]]; then
    echo "oss-release-pack-native: missing ${list}" >&2
    exit 2
  fi
  cargo build --release -p plasm --bin plasm-pack-plugins
  cargo run --release -p plasm --bin plasm-pack-plugins -- \
    --workspace "${root}" \
    --apis-root "${root}/apis" \
    --output-dir "${plugins_dir}" \
    --package-list "${list}"
}

echo "oss-release-pack-native: packing plugins for ${triple}…"
pack_plugins

appliance="${pack_root}/appliance"
client="${pack_root}/client"
cgs="${pack_root}/cgs"
mkdir -p "${appliance}" "${client}" "${cgs}"

cp "${release_dir}/plasm-server" "${appliance}/"
if [[ -d "${pack_root}/plugins" ]] && find "${pack_root}/plugins" -maxdepth 1 \( -name 'libplasm_plugin_*.so' -o -name 'libplasm_plugin_*.dylib' \) | grep -q .; then
  cp -R "${pack_root}/plugins" "${appliance}/plugins"
fi
cp "${release_dir}/plasm" "${client}/"
cp "${release_dir}/plasm-cgs" "${cgs}/"

mkdir -p "${out_dir}"

appliance_out="${out_dir}/plasm-appliance-${ver}-${triple}.tar.gz"
client_out="${out_dir}/plasm-${ver}-${triple}.tar.gz"
cgs_out="${out_dir}/plasm-cgs-${ver}-${triple}.tar.gz"

tar -czf "${appliance_out}" -C "${appliance}" .
tar -czf "${client_out}" -C "${client}" plasm
tar -czf "${cgs_out}" -C "${cgs}" plasm-cgs

echo "oss-release-pack-native: ${appliance_out}"
echo "oss-release-pack-native: ${client_out}"
echo "oss-release-pack-native: ${cgs_out}"
