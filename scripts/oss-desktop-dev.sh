#!/usr/bin/env bash
# shellcheck shell=bash
#
# Appliance dev stack: Docker Postgres → Iceberg trace sink → OSS plasm-mcp (always --plugin-dir, packed from apis/) → Phoenix desktop.
# Fails fast if packing produces no plugin dylibs. Ctrl+C tears down sink + agent started here.
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

# Match `plasm-mcp --http --port P --mcp`: Streamable MCP defaults to P+1 unless `--mcp-port` is set.
HTTP_PORT="${OSS_DESKTOP_AGENT_HTTP_PORT:-3000}"
MCP_PORT="${OSS_DESKTOP_AGENT_MCP_PORT:-3001}"
TRACE_SINK_PORT="${PLASM_TRACE_SINK_PORT:-7070}"
OSS_DESKTOP_PG_PORT="${OSS_DESKTOP_PG_PORT:-5433}"

AGENT_PID=""
TRACE_SINK_PID=""
STARTED_AGENT=0
STARTED_TRACE_SINK=0

bash "${SCRIPT_DIR}/oss-desktop-postgres.sh"

# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-jwt-secret.sh"
# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-auth-storage-encryption-key.sh"
# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-control-plane-secret.sh"

# HTTP `/v1/traces*` resolves tenant from incoming JWT; MCP traces use `project_mcp_configs.tenant_id`.
# Without `optional`/`required`, middleware skips Bearer parsing and tenant traces are invisible over HTTP.
export PLASM_INCOMING_AUTH_MODE="${PLASM_INCOMING_AUTH_MODE:-optional}"

export DATABASE_URL="${DATABASE_URL:-postgresql://postgres:postgres@127.0.0.1:${OSS_DESKTOP_PG_PORT}/plasm_desktop_dev}"
export PLASM_MCP_HTTP_BASE_URL="${PLASM_MCP_HTTP_BASE_URL:-http://127.0.0.1:${HTTP_PORT}}"
export PLASM_MCP_UPSTREAM_URL="${PLASM_MCP_UPSTREAM_URL:-http://127.0.0.1:${MCP_PORT}}"
export PLASM_MCP_PUBLIC_BASE_URL="${PLASM_MCP_PUBLIC_BASE_URL:-http://127.0.0.1:${MCP_PORT}/mcp}"

cd "${PLASM_OSS_ROOT}/desktop"
mix deps.get
mix ecto.migrate
cd "${PLASM_OSS_ROOT}"

# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-export-plasm-trace-sink-catalog.sh"

_curl_health_get() {
  local port="$1"
  curl -sf --connect-timeout 2 --max-time 5 "http://127.0.0.1:${port}/v1/health" >/dev/null 2>&1
}

agent_health_ok() {
  _curl_health_get "${HTTP_PORT}"
}

trace_sink_health_ok() {
  _curl_health_get "${TRACE_SINK_PORT}"
}

cleanup() {
  if [[ "${STARTED_AGENT}" -eq 1 ]] && [[ -n "${AGENT_PID}" ]]; then
    echo "oss-desktop-dev: stopping plasm-mcp (pid ${AGENT_PID})"
    kill "${AGENT_PID}" 2>/dev/null || true
    wait "${AGENT_PID}" 2>/dev/null || true
  fi
  if [[ "${STARTED_TRACE_SINK}" -eq 1 ]] && [[ -n "${TRACE_SINK_PID}" ]]; then
    echo "oss-desktop-dev: stopping plasm-trace-sink (pid ${TRACE_SINK_PID})"
    kill "${TRACE_SINK_PID}" 2>/dev/null || true
    wait "${TRACE_SINK_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

export PLASM_TRACE_SINK_URL="${PLASM_TRACE_SINK_URL:-http://127.0.0.1:${TRACE_SINK_PORT}}"

if trace_sink_health_ok; then
  echo "oss-desktop-dev: plasm-trace-sink already healthy on :${TRACE_SINK_PORT} — not starting another"
elif command -v nc >/dev/null 2>&1 && nc -z 127.0.0.1 "${TRACE_SINK_PORT}" 2>/dev/null; then
  echo "oss-desktop-dev: port ${TRACE_SINK_PORT} is in use but trace sink health check failed — free the port" >&2
  exit 1
else
  echo "oss-desktop-dev: building plasm-trace-sink (${_PLASM_TARGET_SUBDIR})"
  (cd "${PLASM_OSS_ROOT}" && cargo build "${_RUST_CARGO_PROFILE_FLAG[@]}" -p plasm-trace-sink)
  _ts_data="${PLASM_OSS_ROOT}/var/plasm-trace-sink"
  mkdir -p "${_ts_data}"
  _ts_log="${_ts_data}/oss-desktop-trace-sink.log"
  echo "oss-desktop-dev: starting plasm-trace-sink on :${TRACE_SINK_PORT} (data ${_ts_data}, log ${_ts_log})"
  : >"${_ts_log}"
  PLASM_TRACE_SINK_LISTEN="127.0.0.1:${TRACE_SINK_PORT}" \
    PLASM_TRACE_SINK_DATA_DIR="${_ts_data}" \
    PLASM_TRACE_SINK_ICEBERG=1 \
    "${PLASM_OSS_ROOT}/target/${_PLASM_TARGET_SUBDIR}/plasm-trace-sink" >>"${_ts_log}" 2>&1 &
  TRACE_SINK_PID=$!
  STARTED_TRACE_SINK=1
  _ts_wait_secs="${PLASM_OSS_DESKTOP_TRACE_SINK_WAIT_SECS:-90}"
  _ts_step=0.5
  _ok_ts=0
  _ts_iters=$(awk -v s="${_ts_wait_secs}" -v step="${_ts_step}" 'BEGIN { printf "%d", s / step + 0.5 }')
  for _ in $(seq 1 "${_ts_iters}"); do
    if trace_sink_health_ok; then
      _ok_ts=1
      break
    fi
    sleep "${_ts_step}"
  done
  if [[ "${_ok_ts}" -ne 1 ]]; then
    echo "oss-desktop-dev: plasm-trace-sink did not become healthy within ${_ts_wait_secs}s (override: PLASM_OSS_DESKTOP_TRACE_SINK_WAIT_SECS)" >&2
    if kill -0 "${TRACE_SINK_PID}" 2>/dev/null; then
      echo "oss-desktop-dev: trace-sink pid ${TRACE_SINK_PID} still running — try: curl -v http://127.0.0.1:${TRACE_SINK_PORT}/v1/health" >&2
    else
      echo "oss-desktop-dev: trace-sink process exited; last log lines from ${_ts_log}:" >&2
      tail -n 50 "${_ts_log}" >&2 || true
    fi
    exit 1
  fi
fi

_pack_dir="${PLASM_OSS_ROOT}/target/plasm-plugins"
mkdir -p "${_pack_dir}"

_have_pack_plugins() {
  find "${_pack_dir}" -maxdepth 1 \( -name 'libplasm_plugin_*.dylib' -o -name 'libplasm_plugin_*.so' -o -name 'libplasm_plugin_*.dll' \) 2>/dev/null | grep -q .
}

if agent_health_ok; then
  echo "oss-desktop-dev: plasm-mcp HTTP already healthy on :${HTTP_PORT} — not starting another"
elif command -v nc >/dev/null 2>&1 && nc -z 127.0.0.1 "${HTTP_PORT}" 2>/dev/null; then
  echo "oss-desktop-dev: port ${HTTP_PORT} is in use but GET /v1/health failed — free the port or fix the process" >&2
  exit 1
else
  if ! _have_pack_plugins; then
    echo "oss-desktop-dev: packing plugins from apis/ (first run can take a long time)"
    if [[ -z "${PLASM_OSS_RUST_DEBUG:-}" ]]; then
      (cd "${PLASM_OSS_ROOT}" && cargo build "${_RUST_CARGO_PROFILE_FLAG[@]}" -p plasm-agent --bin plasm-pack-plugins)
      (cd "${PLASM_OSS_ROOT}" && cargo run "${_RUST_CARGO_PROFILE_FLAG[@]}" -p plasm-agent --bin plasm-pack-plugins -- --workspace "${PLASM_OSS_ROOT}" --apis-root "${PLASM_OSS_ROOT}/apis" --output-dir "${_pack_dir}" --release)
    else
      (cd "${PLASM_OSS_ROOT}" && cargo run -p plasm-agent --bin plasm-pack-plugins -- --workspace "${PLASM_OSS_ROOT}" --apis-root "${PLASM_OSS_ROOT}/apis" --output-dir "${_pack_dir}" --release)
    fi
  else
    echo "oss-desktop-dev: building plasm-mcp (plugins already in ${_pack_dir})"
    (cd "${PLASM_OSS_ROOT}" && cargo build "${_RUST_CARGO_PROFILE_FLAG[@]}" -p plasm-agent --bin plasm-mcp)
  fi
  if ! _have_pack_plugins; then
    echo "oss-desktop-dev: still no plugin dylibs in ${_pack_dir} — fix plasm-oss/apis (catalog checkout) and retry." >&2
    exit 1
  fi
  echo "oss-desktop-dev: starting plasm-mcp MCP on :${MCP_PORT}/mcp, HTTP on :${HTTP_PORT} (PLASM_TRACE_SINK_URL=${PLASM_TRACE_SINK_URL})"
  "${PLASM_OSS_ROOT}/target/${_PLASM_TARGET_SUBDIR}/plasm-mcp" \
    --plugin-dir "${_pack_dir}" \
    --http --port "${HTTP_PORT}" --mcp --mcp-port "${MCP_PORT}" &
  AGENT_PID=$!
  STARTED_AGENT=1
  echo "oss-desktop-dev: waiting for http://127.0.0.1:${HTTP_PORT}/v1/health …"
  ok=0
  for _ in $(seq 1 180); do
    if agent_health_ok; then
      ok=1
      break
    fi
    sleep 1
  done
  if [[ "${ok}" -ne 1 ]]; then
    echo "oss-desktop-dev: agent did not become healthy in time" >&2
    exit 1
  fi
fi

echo "oss-desktop-dev: starting Phoenix desktop (${PLASM_MCP_HTTP_BASE_URL}, PLASM_TRACE_SINK_URL=${PLASM_TRACE_SINK_URL})"
cd "${PLASM_OSS_ROOT}/desktop"
mix phx.server
