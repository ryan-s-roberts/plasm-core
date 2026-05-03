#!/usr/bin/env bash
# shellcheck shell=bash
#
# Foreground Iceberg plasm-trace-sink for OSS desktop dev (split-terminal workflow).
# Ensures Docker Postgres + SqlCatalog schema, then execs the sink (logs to this terminal).
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLASM_OSS_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

_RUST_CARGO_PROFILE_FLAG=(--release)
_PLASM_TARGET_SUBDIR=release
if [[ -n "${PLASM_OSS_RUST_DEBUG:-}" ]]; then
  _RUST_CARGO_PROFILE_FLAG=()
  _PLASM_TARGET_SUBDIR=debug
fi

TRACE_SINK_PORT="${PLASM_TRACE_SINK_PORT:-7070}"

bash "${SCRIPT_DIR}/oss-desktop-postgres.sh"

# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-export-plasm-trace-sink-catalog.sh"

_curl_health_get() {
  curl -sf --connect-timeout 2 --max-time 5 "http://127.0.0.1:${TRACE_SINK_PORT}/v1/health" >/dev/null 2>&1
}

if _curl_health_get; then
  echo "oss-desktop-trace-sink: already healthy on :${TRACE_SINK_PORT} — nothing to do"
  exit 0
fi
if command -v nc >/dev/null 2>&1 && nc -z 127.0.0.1 "${TRACE_SINK_PORT}" 2>/dev/null; then
  echo "oss-desktop-trace-sink: port ${TRACE_SINK_PORT} is in use but GET /v1/health failed — free the port" >&2
  exit 1
fi

echo "oss-desktop-trace-sink: building (${_PLASM_TARGET_SUBDIR})"
(cd "${PLASM_OSS_ROOT}" && cargo build "${_RUST_CARGO_PROFILE_FLAG[@]}" -p plasm-trace-sink)

_ts_data="${PLASM_OSS_ROOT}/var/plasm-trace-sink"
mkdir -p "${_ts_data}"
echo "oss-desktop-trace-sink: listening 127.0.0.1:${TRACE_SINK_PORT}; warehouse ${_ts_data}/iceberg_warehouse (Ctrl+C to stop)"

exec env PLASM_TRACE_SINK_LISTEN="127.0.0.1:${TRACE_SINK_PORT}" \
  PLASM_TRACE_SINK_DATA_DIR="${_ts_data}" \
  PLASM_TRACE_SINK_ICEBERG=1 \
  "${PLASM_OSS_ROOT}/target/${_PLASM_TARGET_SUBDIR}/plasm-trace-sink"
