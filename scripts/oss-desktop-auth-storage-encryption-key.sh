#!/usr/bin/env bash
# shellcheck shell=bash
#
# AUTH_STORAGE_ENCRYPTION_KEY for OSS desktop dev when DATABASE_URL enables Postgres auth KV
# (encrypted kv_store / MCP API keys). Must be a base64-encoded 32-byte key (auth-framework).
# Writes plasm-oss/.plasm/oss-desktop-auth-storage-encryption-key (gitignored).
#
# Usage (from plasm-oss/):
#   source scripts/oss-desktop-auth-storage-encryption-key.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLASM_OSS_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SECRET_DIR="${PLASM_OSS_ROOT}/.plasm"
SECRET_FILE="${SECRET_DIR}/oss-desktop-auth-storage-encryption-key"

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  echo "source this file from your shell (do not execute directly):" >&2
  echo "  source scripts/oss-desktop-auth-storage-encryption-key.sh" >&2
  exit 1
fi

mkdir -p "${SECRET_DIR}"

if [[ ! -f "${SECRET_FILE}" ]]; then
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -base64 32 | tr -d '\n' >"${SECRET_FILE}"
  else
    head -c 32 /dev/urandom | base64 | tr -d '\n' >"${SECRET_FILE}"
  fi
  chmod 600 "${SECRET_FILE}" || true
fi

export AUTH_STORAGE_ENCRYPTION_KEY="$(cat "${SECRET_FILE}")"
