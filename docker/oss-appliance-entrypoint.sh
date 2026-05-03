#!/usr/bin/env bash
# OSS appliance: init Postgres under /data, migrate Phoenix desktop schema, start plasm-mcp + release.
set -euo pipefail

PG_BIN="/usr/lib/postgresql/${PG_MAJOR}/bin"
export PATH="${PG_BIN}:${PATH}"

mkdir -p /data/postgres /data/plasm/trace-archive /data/plasm/run-artifacts /data/plasm/secrets
chown -R postgres:postgres /data/postgres
chown -R plasm:plasm /data/plasm/trace-archive /data/plasm/run-artifacts /data/plasm/secrets

if [[ ! -s "${PGDATA}/PG_VERSION" ]]; then
  echo "[oss-appliance] initializing Postgres in ${PGDATA}"
  runuser -u postgres -- initdb -D "${PGDATA}" --locale=C.UTF-8 --encoding=UTF8 --auth-local=trust --auth-host=trust
  {
    echo "listen_addresses = '127.0.0.1'"
    echo "unix_socket_directories = '/tmp'"
  } >>"${PGDATA}/postgresql.conf"
fi

cleanup() {
  if [[ -n "${AGENT_PID:-}" ]]; then
    kill "${AGENT_PID}" 2>/dev/null || true
    wait "${AGENT_PID}" 2>/dev/null || true
  fi
  runuser -u postgres -- env PGDATA="${PGDATA}" pg_ctl -D "${PGDATA}" stop -m fast 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "[oss-appliance] starting Postgres"
runuser -u postgres -- pg_ctl -D "${PGDATA}" -w start

until runuser -u postgres -- pg_isready -h 127.0.0.1 -p 5432 >/dev/null 2>&1; do
  sleep 0.2
done

runuser -u postgres -- psql -h 127.0.0.1 -d postgres -tc "SELECT 1 FROM pg_database WHERE datname = 'plasm_appliance'" | grep -q 1 \
  || runuser -u postgres -- createdb -h 127.0.0.1 plasm_appliance

export DATABASE_URL="${DATABASE_URL:-postgresql://postgres@127.0.0.1:5432/plasm_appliance}"
export PLASM_AUTH_STORAGE_URL="${PLASM_AUTH_STORAGE_URL:-$DATABASE_URL}"

if [[ ! -f /data/plasm/secrets/secret_key_base ]]; then
  openssl rand -base64 64 >/data/plasm/secrets/secret_key_base
  chmod 600 /data/plasm/secrets/secret_key_base
fi
export SECRET_KEY_BASE="${SECRET_KEY_BASE:-$(cat /data/plasm/secrets/secret_key_base)}"

if [[ ! -f /data/plasm/secrets/plasm_auth_jwt_secret ]]; then
  openssl rand -base64 48 >/data/plasm/secrets/plasm_auth_jwt_secret
  chmod 600 /data/plasm/secrets/plasm_auth_jwt_secret
fi
export PLASM_AUTH_JWT_SECRET="${PLASM_AUTH_JWT_SECRET:-$(cat /data/plasm/secrets/plasm_auth_jwt_secret)}"

export PLASM_TRACE_ARCHIVE_DIR="${PLASM_TRACE_ARCHIVE_DIR:-/data/plasm/trace-archive}"
export PLASM_RUN_ARTIFACTS_DIR="${PLASM_RUN_ARTIFACTS_DIR:-/data/plasm/run-artifacts}"
export PLASM_INCOMING_AUTH_MODE="${PLASM_INCOMING_AUTH_MODE:-optional}"

# Debian slim images often lack generated UTF-8 locales; avoids BEAM latin1 name-encoding warnings.
export ELIXIR_ERL_OPTIONS="${ELIXIR_ERL_OPTIONS:-+fnu}"

export PLASM_MCP_HTTP_BASE_URL="${PLASM_MCP_HTTP_BASE_URL:-http://127.0.0.1:3001}"
export PLASM_MCP_UPSTREAM_URL="${PLASM_MCP_UPSTREAM_URL:-http://127.0.0.1:3000}"
export PORT="${PORT:-4000}"
export PHX_HOST="${PHX_HOST:-0.0.0.0}"

PUB="${PUBLIC_WEB_ORIGIN:-http://127.0.0.1:${PORT}}"
export PLASM_MCP_PUBLIC_BASE_URL="${PLASM_MCP_PUBLIC_BASE_URL:-${PUB}/plasm}"

if [[ -n "${PLASM_DESKTOP_BEARER_TOKEN:-}" ]]; then
  export PLASM_DESKTOP_BEARER_TOKEN
fi

echo "[oss-appliance] starting plasm-mcp (OSS)"
runuser -u plasm -- /usr/local/bin/plasm-mcp \
  --plugin-dir /app/plugins \
  --http --port 3001 \
  --mcp --mcp-port 3000 &
AGENT_PID=$!

for _ in $(seq 1 150); do
  if curl -sf "http://127.0.0.1:3001/v1/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

echo "[oss-appliance] running Phoenix migrations"
runuser -u plasm -- env \
  DATABASE_URL="${DATABASE_URL}" \
  SECRET_KEY_BASE="${SECRET_KEY_BASE}" \
  PORT="${PORT}" \
  PHX_HOST="${PHX_HOST}" \
  PLASM_MCP_HTTP_BASE_URL="${PLASM_MCP_HTTP_BASE_URL}" \
  PLASM_MCP_UPSTREAM_URL="${PLASM_MCP_UPSTREAM_URL}" \
  PLASM_MCP_PUBLIC_BASE_URL="${PLASM_MCP_PUBLIC_BASE_URL}" \
  PLASM_DESKTOP_BEARER_TOKEN="${PLASM_DESKTOP_BEARER_TOKEN:-}" \
  PHX_SERVER="${PHX_SERVER:-true}" \
  /app/plasm_desktop/bin/plasm_desktop eval PlasmDesktop.Release.migrate

echo "[oss-appliance] starting Plasm Desktop (Phoenix)"
runuser -u plasm -- env \
  DATABASE_URL="${DATABASE_URL}" \
  SECRET_KEY_BASE="${SECRET_KEY_BASE}" \
  PORT="${PORT}" \
  PHX_HOST="${PHX_HOST}" \
  PLASM_MCP_HTTP_BASE_URL="${PLASM_MCP_HTTP_BASE_URL}" \
  PLASM_MCP_UPSTREAM_URL="${PLASM_MCP_UPSTREAM_URL}" \
  PLASM_MCP_PUBLIC_BASE_URL="${PLASM_MCP_PUBLIC_BASE_URL}" \
  PLASM_DESKTOP_BEARER_TOKEN="${PLASM_DESKTOP_BEARER_TOKEN:-}" \
  PHX_SERVER="${PHX_SERVER:-true}" \
  /app/plasm_desktop/bin/plasm_desktop start &
PHX_PID=$!
wait "${PHX_PID}"
