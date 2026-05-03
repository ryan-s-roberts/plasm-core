#!/usr/bin/env bash
# shellcheck shell=bash
#
# Source after `oss-desktop-postgres.sh`. Creates schema plasm_iceberg_catalog in the desktop
# dev database and sets PLASM_TRACE_SINK_CATALOG_URL (same pattern as monorepo
# scripts/export-plasm-trace-sink-catalog.sh).
#
# Requires: bash; optional python3 for URL-encoding DATABASE_USER / DATABASE_PASSWORD.

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  echo "oss-export-plasm-trace-sink-catalog.sh: source this file, do not execute it directly." >&2
  exit 1
fi

SCHEMA="${PLASM_TRACE_SINK_CATALOG_SCHEMA:-plasm_iceberg_catalog}"
CONTAINER="${OSS_DESKTOP_PG_CONTAINER:-plasm_desktop_postgres}"
DB_NAME="${OSS_DESKTOP_PG_DATABASE:-plasm_desktop_dev}"
DB_USER="${DATABASE_USER:-postgres}"
DB_PASS="${DATABASE_PASSWORD:-postgres}"
DB_HOST="${DATABASE_HOST:-127.0.0.1}"
DB_PORT="${OSS_DESKTOP_PG_PORT:-5433}"

_plasm_ts_url_enc() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1], safe=''))" "$1"
  else
    printf '%s' "$1"
  fi
}

if docker container inspect "${CONTAINER}" >/dev/null 2>&1; then
  running="$(docker container inspect -f '{{.State.Running}}' "${CONTAINER}" 2>/dev/null || echo false)"
  if [[ "${running}" == "true" ]]; then
    docker exec "${CONTAINER}" psql -U "${DB_USER}" -d "${DB_NAME}" -v ON_ERROR_STOP=1 \
      -c "CREATE SCHEMA IF NOT EXISTS \"${SCHEMA}\";" >/dev/null
    if [[ -z "${PLASM_TRACE_SINK_CATALOG_URL:-}" ]]; then
      U="$(_plasm_ts_url_enc "${DB_USER}")"
      P="$(_plasm_ts_url_enc "${DB_PASS}")"
      export PLASM_TRACE_SINK_CATALOG_URL="postgresql://${U}:${P}@${DB_HOST}:${DB_PORT}/${DB_NAME}?options=-c%20search_path%3D${SCHEMA}%2Cpublic"
    fi
  fi
fi

return 0
