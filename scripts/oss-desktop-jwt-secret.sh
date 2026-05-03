#!/usr/bin/env bash
# shellcheck shell=bash
#
# Single-user JWT secret for OSS desktop dev when running plasm-mcp with auth-framework.
# Writes plasm-oss/.plasm/oss-desktop-jwt-secret (gitignored) and exports PLASM_AUTH_JWT_SECRET.
#
# Usage (from plasm-oss/):
#   source scripts/oss-desktop-jwt-secret.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLASM_OSS_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SECRET_DIR="${PLASM_OSS_ROOT}/.plasm"
SECRET_FILE="${SECRET_DIR}/oss-desktop-jwt-secret"

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  echo "source this file from your shell (do not execute directly):" >&2
  echo "  source scripts/oss-desktop-jwt-secret.sh" >&2
  exit 1
fi

mkdir -p "${SECRET_DIR}"

if [[ ! -f "${SECRET_FILE}" ]]; then
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -base64 48 | tr -d '\n' >"${SECRET_FILE}"
  else
    head -c 48 /dev/urandom | base64 | tr -d '\n' >"${SECRET_FILE}"
  fi
  chmod 600 "${SECRET_FILE}" || true
fi

export PLASM_AUTH_JWT_SECRET="$(cat "${SECRET_FILE}")"
