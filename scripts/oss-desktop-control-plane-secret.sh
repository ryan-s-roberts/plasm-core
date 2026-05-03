#!/usr/bin/env bash
# shellcheck shell=bash
#
# Single-user control-plane secret for OSS appliance dev (agent `/internal/*` + Phoenix desktop).
# When unset, uses the same default as `plasm-agent-core` `DEV_PLANE_SECRET_FALLBACK`
# (`control_plane_http.rs`). Override `PLASM_MCP_CONTROL_PLANE_SECRET` for production.
#
# Usage (from plasm-oss/):
#   source scripts/oss-desktop-control-plane-secret.sh
#
set -euo pipefail

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  echo "source this file from your shell (do not execute directly):" >&2
  echo "  source scripts/oss-desktop-control-plane-secret.sh" >&2
  exit 1
fi

: "${PLASM_MCP_CONTROL_PLANE_SECRET:=dev-plasm-mcp-control-plane-secret-32chars-min!!}"
export PLASM_MCP_CONTROL_PLANE_SECRET
