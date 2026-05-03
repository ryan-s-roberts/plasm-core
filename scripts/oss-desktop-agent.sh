#!/usr/bin/env bash
# shellcheck shell=bash
#
# Run OSS plasm-mcp for appliance dev (HTTP + MCP + plugin-dir only). Invoked by `just oss-desktop-agent`.
# Exits non-zero if target/plasm-plugins has no packed dylibs — developer should fix apis/ or packing, not run broken.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLASM_OSS_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-jwt-secret.sh"
# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-auth-storage-encryption-key.sh"
# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-control-plane-secret.sh"

export PLASM_INCOMING_AUTH_MODE="${PLASM_INCOMING_AUTH_MODE:-optional}"

_pg="${OSS_DESKTOP_PG_PORT:-5433}"
: "${DATABASE_URL:=postgresql://postgres:postgres@127.0.0.1:${_pg}/plasm_desktop_dev}"
export DATABASE_URL

_ts="${PLASM_TRACE_SINK_PORT:-7070}"
export PLASM_TRACE_SINK_URL="${PLASM_TRACE_SINK_URL:-http://127.0.0.1:${_ts}}"

_pack_dir="${PLASM_OSS_ROOT}/target/plasm-plugins"
mkdir -p "${_pack_dir}"

_have_plugins() {
  find "${_pack_dir}" -maxdepth 1 \( -name 'libplasm_plugin_*.dylib' -o -name 'libplasm_plugin_*.so' -o -name 'libplasm_plugin_*.dll' \) 2>/dev/null | grep -q .
}

if ! _have_plugins; then
  echo "oss-desktop-agent: packing plugins into ${_pack_dir}"
  if [[ -z "${PLASM_OSS_RUST_DEBUG:-}" ]]; then
    (cd "${PLASM_OSS_ROOT}" && cargo run --release -p plasm-agent --bin plasm-pack-plugins -- --workspace "${PLASM_OSS_ROOT}" --apis-root "${PLASM_OSS_ROOT}/apis" --output-dir "${_pack_dir}" --release)
  else
    (cd "${PLASM_OSS_ROOT}" && cargo run -p plasm-agent --bin plasm-pack-plugins -- --workspace "${PLASM_OSS_ROOT}" --apis-root "${PLASM_OSS_ROOT}/apis" --output-dir "${_pack_dir}" --release)
  fi
fi

if ! _have_plugins; then
  echo "oss-desktop-agent: no plugin dylibs in ${_pack_dir} after pack — check plasm-oss/apis (submodule / catalogs) and re-run." >&2
  exit 1
fi

_http="${OSS_DESKTOP_AGENT_HTTP_PORT:-3000}"
_mcp="${OSS_DESKTOP_AGENT_MCP_PORT:-3001}"

if [[ -z "${PLASM_OSS_RUST_DEBUG:-}" ]]; then
  exec cargo run --release -p plasm-agent --bin plasm-mcp -- \
    --plugin-dir "${_pack_dir}" \
    --http --port "${_http}" --mcp --mcp-port "${_mcp}"
else
  exec cargo run -p plasm-agent --bin plasm-mcp -- \
    --plugin-dir "${_pack_dir}" \
    --http --port "${_http}" --mcp --mcp-port "${_mcp}"
fi
