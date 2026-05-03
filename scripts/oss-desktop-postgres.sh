#!/usr/bin/env bash
# Idempotent Postgres for Plasm Desktop local dev (OSS appliance shell).
# Container: plasm_desktop_postgres · default host port 5433 (avoids monorepo plasm_web_postgres on 5432).
set -euo pipefail

CONTAINER="${OSS_DESKTOP_PG_CONTAINER:-plasm_desktop_postgres}"
IMAGE="${OSS_DESKTOP_PG_IMAGE:-postgres:16-alpine}"
HOST_PORT="${OSS_DESKTOP_PG_PORT:-5433}"
DB_NAME="${OSS_DESKTOP_PG_DATABASE:-plasm_desktop_dev}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OSS_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

wait_ready() {
  local i
  for i in $(seq 1 90); do
    if docker exec "${CONTAINER}" pg_isready -U postgres -d "${DB_NAME}" >/dev/null 2>&1; then
      echo "oss-desktop-postgres: ${CONTAINER} ready (${DB_NAME} on localhost:${HOST_PORT})"
      return 0
    fi
    sleep 1
  done
  echo "oss-desktop-postgres: timeout waiting for PostgreSQL" >&2
  return 1
}

if ! command -v docker >/dev/null 2>&1; then
  echo "oss-desktop-postgres: docker not found" >&2
  exit 1
fi

if docker container inspect "${CONTAINER}" >/dev/null 2>&1; then
  running="$(docker container inspect -f '{{.State.Running}}' "${CONTAINER}" 2>/dev/null || echo false)"
  if [[ "${running}" == "true" ]]; then
    echo "oss-desktop-postgres: ${CONTAINER} already running"
    wait_ready
    exit 0
  fi
  echo "oss-desktop-postgres: starting existing container ${CONTAINER}"
  docker start "${CONTAINER}"
  wait_ready
  exit 0
fi

echo "oss-desktop-postgres: creating ${CONTAINER} (image ${IMAGE}, host port ${HOST_PORT})"
docker run -d --name "${CONTAINER}" \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB="${DB_NAME}" \
  -p "${HOST_PORT}:5432" \
  -v plasm_desktop_pgdata:/var/lib/postgresql/data \
  "${IMAGE}"

wait_ready
