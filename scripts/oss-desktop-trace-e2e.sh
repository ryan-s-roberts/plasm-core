#!/usr/bin/env bash
# shellcheck shell=bash
#
# End-to-end: MCP (Streamable HTTP) opens `plasm_context`, then HTTP JWT verifies traces (list, detail, SSE snapshot).
#
# Prerequisites:
#   - Desktop Postgres container + migrations (see oss-desktop-postgres.sh / mix ecto.migrate).
#   - plasm-mcp on OSS_DESKTOP_AGENT_HTTP_PORT / MCP port (e.g. just oss-desktop-dev or oss-desktop-agent).
#   - Agent must run with PLASM_INCOMING_AUTH_MODE=optional (oss-desktop-dev.sh sets this) so Bearer JWT maps tenant for GET /v1/traces.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLASM_OSS_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-jwt-secret.sh"
# shellcheck source=/dev/null
source "${SCRIPT_DIR}/oss-desktop-control-plane-secret.sh"

HTTP_PORT="${OSS_DESKTOP_AGENT_HTTP_PORT:-3000}"
MCP_PORT="${OSS_DESKTOP_AGENT_MCP_PORT:-3001}"
PG_PORT="${OSS_DESKTOP_PG_PORT:-5433}"
HTTP_BASE="http://127.0.0.1:${HTTP_PORT}"
MCP_URL="http://127.0.0.1:${MCP_PORT}/mcp"
CONTAINER="${OSS_DESKTOP_PG_CONTAINER:-plasm_desktop_postgres}"
DB_NAME="${OSS_DESKTOP_PG_DATABASE:-plasm_desktop_dev}"

mint_jwt() {
  local tenant="$1"
  python3 - "${PLASM_AUTH_JWT_SECRET}" "${tenant}" <<'PY'
import sys, hmac, hashlib, base64, json, time

def b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).decode().rstrip("=")

secret, tenant = sys.argv[1], sys.argv[2]
hdr = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}, separators=(",", ":")).encode())
body = b64url(
    json.dumps(
        {"sub": "oss-trace-e2e", "tenant_id": tenant, "exp": int(time.time()) + 7200},
        separators=(",", ":"),
    ).encode()
)
msg = f"{hdr}.{body}"
sig = hmac.new(secret.encode(), msg.encode(), hashlib.sha256).digest()
print(f"{msg}.{b64url(sig)}")
PY
}

require_container() {
  if ! docker exec "${CONTAINER}" pg_isready -U postgres -d "${DB_NAME}" >/dev/null 2>&1; then
    echo "oss-desktop-trace-e2e: Postgres container ${CONTAINER} / ${DB_NAME} not ready" >&2
    exit 1
  fi
}

require_container

echo "oss-desktop-trace-e2e: waiting for ${HTTP_BASE} …"
_ok_http=0
for _ in $(seq 1 60); do
  if curl -sf --connect-timeout 1 --max-time 3 "${HTTP_BASE}/v1/registry" >/dev/null; then
    _ok_http=1
    break
  fi
  sleep 1
done
if [[ "${_ok_http}" -ne 1 ]]; then
  echo "oss-desktop-trace-e2e: agent not reachable at ${HTTP_BASE} (pack plugins + start plasm-mcp first)" >&2
  exit 1
fi

CID="$(docker exec "${CONTAINER}" psql -U postgres -d "${DB_NAME}" -tAc \
  "SELECT trim(value) FROM desktop_settings WHERE key = 'mcp_appliance_config_id' LIMIT 1;")"
if [[ -z "${CID}" ]]; then
  echo "oss-desktop-trace-e2e: no mcp_appliance_config_id in desktop_settings — open Desktop once or seed settings" >&2
  exit 1
fi

TENANT="$(docker exec "${CONTAINER}" psql -U postgres -d "${DB_NAME}" -tAc \
  "SELECT tenant_id FROM project_mcp_configs WHERE id = '${CID}'::uuid LIMIT 1;" | tr -d '[:space:]')"
if [[ -z "${TENANT}" ]]; then
  echo "oss-desktop-trace-e2e: no project_mcp_configs row for config ${CID}" >&2
  exit 1
fi

SECRET="${PLASM_MCP_CONTROL_PLANE_SECRET}"
# Avoid cluttering the Desktop key list: reuse one automation label + reveal, or pass PLASM_TRACE_E2E_MCP_BEARER.
E2E_LABEL="${PLASM_TRACE_E2E_KEY_LABEL:-trace-e2e-automation}"
if [[ -n "${PLASM_TRACE_E2E_MCP_BEARER:-}" ]]; then
  MCP_TRANSPORT_KEY="${PLASM_TRACE_E2E_MCP_BEARER}"
else
  MCP_TRANSPORT_KEY="$(python3 - "${HTTP_BASE}" "${SECRET}" "${CID}" "${E2E_LABEL}" <<'PY'
import json, sys, urllib.error, urllib.parse, urllib.request

base, secret, cid, label = sys.argv[1:5]

def req(method, path, data=None):
    url = base.rstrip("/") + path
    h = {"X-Plasm-Control-Plane-Secret": secret}
    body = None
    if data is not None:
        h["Content-Type"] = "application/json"
        body = json.dumps(data).encode()
    r = urllib.request.Request(url, data=body, headers=h, method=method)
    with urllib.request.urlopen(r, timeout=30) as resp:
        return resp.read()

# List keys
raw = req("GET", "/internal/mcp-api-key/v1/keys?" + urllib.parse.urlencode({"config_id": cid}))
rows = json.loads(raw.decode())
key_id = None
for row in rows:
    if (row.get("label") or "") == label:
        key_id = row.get("key_id")
        break
if key_id:
    q = urllib.parse.urlencode({"config_id": cid, "key_id": str(key_id)})
    raw2 = req("GET", "/internal/mcp-api-key/v1/reveal?" + q)
    out = json.loads(raw2.decode())
    print(out["api_key"])
    sys.exit(0)
# Provision once
raw3 = req(
    "POST",
    "/internal/mcp-api-key/v1/provision",
    {"config_id": cid, "label": label},
)
out3 = json.loads(raw3.decode())
print(out3["api_key"])
PY
)"
fi

JWT="$(mint_jwt "${TENANT}")"

INIT_BODY='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"trace-e2e","version":"0"}}}'
curl -sS -D /tmp/oss_trace_e2e_hdr.txt -o /tmp/oss_trace_e2e_body.txt -X POST "${MCP_URL}" \
  -H "Authorization: Bearer ${MCP_TRANSPORT_KEY}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d "${INIT_BODY}"
MCP_SID="$(grep -i '^mcp-session-id:' /tmp/oss_trace_e2e_hdr.txt | awk '{print $2}' | tr -d '\r')"
if [[ -z "${MCP_SID}" ]]; then
  echo "oss-desktop-trace-e2e: missing mcp-session-id from initialize" >&2
  exit 1
fi

curl -sS -o /dev/null -X POST "${MCP_URL}" \
  -H "Authorization: Bearer ${MCP_TRANSPORT_KEY}" \
  -H "MCP-Session-Id: ${MCP_SID}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'

CONTEXT_BODY="$(python3 - <<'PY'
import json
args = {
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
        "name": "plasm_context",
        "arguments": {
            "intent": "trace-e2e-comic",
            "seeds": [{"api": "xkcd", "entity": "Comic"}],
        },
    },
}
print(json.dumps(args))
PY
)"

CTX_SSE="$(curl -sS -X POST "${MCP_URL}" \
  -H "Authorization: Bearer ${MCP_TRANSPORT_KEY}" \
  -H "MCP-Session-Id: ${MCP_SID}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d "${CONTEXT_BODY}")"

LOGICAL_LS="$(printf '%s' "${CTX_SSE}" | python3 -c "
import json, sys
raw = sys.stdin.read()
for line in raw.splitlines():
    if not line.startswith('data: '):
        continue
    try:
        j = json.loads(line[6:])
    except json.JSONDecodeError:
        continue
    if j.get('id') != 2:
        continue
    meta = (j.get('result') or {}).get('_meta') or {}
    plasm = meta.get('plasm') or {}
    ls = plasm.get('logical_session_id')
    if ls:
        print(ls)
        sys.exit(0)
print('missing logical_session_id in plasm_context _meta', file=sys.stderr)
sys.exit(1)
")"

echo "oss-desktop-trace-e2e: tenant=${TENANT} logical_session_id=${LOGICAL_LS}"

TRACE_NS="018fb8d5-4e9a-73a7-b0e1-3f2c1a8b09d4"
TRACE_ID="$(python3 - "${TENANT}" "${LOGICAL_LS}" "${TRACE_NS}" <<'PY'
import uuid, sys
tenant, ls, ns = sys.argv[1], sys.argv[2], sys.argv[3]
name = f"{tenant}\nlogical:{ls}"
print(uuid.uuid5(uuid.UUID(ns), name))
PY
)"

echo "oss-desktop-trace-e2e: expected trace_id (v5 logical) ${TRACE_ID}"

LIST_CODE="$(curl -sS -o /tmp/oss_trace_list.json -w "%{http_code}" -H "Authorization: Bearer ${JWT}" "${HTTP_BASE}/v1/traces?limit=25")"
if [[ "${LIST_CODE}" != "200" ]]; then
  echo "oss-desktop-trace-e2e: GET /v1/traces HTTP ${LIST_CODE}" >&2
  head -c 500 /tmp/oss_trace_list.json >&2 || true
  exit 1
fi
python3 -c "
import json, sys
want = sys.argv[1]
data = json.load(open('/tmp/oss_trace_list.json'))
ids = [t['trace_id'] for t in data.get('traces', [])]
if want not in ids:
    print('trace list:', ids[:10], '...', file=sys.stderr)
    print(
        'hint: tenant-scoped traces need HTTP incoming JWT. '
        'Restart plasm-mcp with PLASM_INCOMING_AUTH_MODE=optional and the same PLASM_AUTH_JWT_SECRET as this script (oss-desktop-jwt-secret.sh).',
        file=sys.stderr,
    )
    sys.exit(f'expected trace_id {want} in GET /v1/traces')
print('GET /v1/traces: ok (contains trace)')
" "${TRACE_ID}"

DETAIL="$(curl -sS -o /tmp/oss_trace_detail.json -w "%{http_code}" -H "Authorization: Bearer ${JWT}" "${HTTP_BASE}/v1/traces/${TRACE_ID}")"
if [[ "${DETAIL}" != "200" ]]; then
  echo "oss-desktop-trace-e2e: GET /v1/traces/${TRACE_ID} HTTP ${DETAIL}" >&2
  head -c 400 /tmp/oss_trace_detail.json >&2 || true
  exit 1
fi

python3 - <<'PY'
import json
d = json.load(open("/tmp/oss_trace_detail.json"))
recs = d.get("records") or []
kinds = [r.get("kind") for r in recs if isinstance(r, dict)]
if "plasm_context" not in kinds:
    print("records kinds:", kinds, file=__import__("sys").stderr)
    raise SystemExit("expected at least one plasm_context trace segment")
print("GET /v1/traces/:id: ok (plasm_context segment present)")
PY

SSE_TMP="$(mktemp)"
set +e
curl -sS -N --max-time 5 -H "Authorization: Bearer ${JWT}" \
  "${HTTP_BASE}/v1/traces/${TRACE_ID}/stream" -o "${SSE_TMP}"
curl_ec=$?
set -e
if [[ "${curl_ec}" -ne 0 && "${curl_ec}" -ne 28 ]]; then
  echo "oss-desktop-trace-e2e: trace SSE curl exit ${curl_ec}" >&2
  exit 1
fi

python3 -c "
import json, sys
path = sys.argv[1]
raw = open(path).read()
found = False
for line in raw.splitlines():
    if line.startswith('data: '):
        payload = line[6:]
        try:
            j = json.loads(payload)
            if j.get('kind') == 'snapshot':
                found = True
                break
        except json.JSONDecodeError:
            pass
if not found:
    sys.stderr.write(raw[:1400] + '\n')
    sys.exit('SSE: expected data line with JSON kind snapshot')
print('GET /v1/traces/:id/stream: ok (snapshot event)')
" "${SSE_TMP}"
rm -f "${SSE_TMP}"

echo "oss-desktop-trace-e2e: all checks passed"
