#!/usr/bin/env bash
# Fetch Linear's GraphQL schema via introspection (vendor-owned; do not hand-author).
# Requires: LINEAR_API_TOKEN (personal API key), npx (Node.js).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${ROOT}/schema.graphql"
TOKEN="${LINEAR_API_TOKEN:?set LINEAR_API_TOKEN}"
npx -y get-graphql-schema https://api.linear.app/graphql \
  -h "Authorization: ${TOKEN}" \
  -o "${OUT}.tmp"
{
  echo "# Linear API — introspected from https://api.linear.app/graphql (vendor schema; not Plasm-owned)."
  echo "# Regenerate: apis/linear/scripts/refresh_schema.sh"
  echo ""
  cat "${OUT}.tmp"
} > "${OUT}"
rm -f "${OUT}.tmp"
echo "Wrote ${OUT}"
