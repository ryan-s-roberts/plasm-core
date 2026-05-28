# Incoming authentication (optional)

Optional **JWT** (`Authorization: Bearer`) and/or **API keys** (`X-API-Key`) for HTTP discovery/execute routes and MCP tools when **`plasm-server`** is started with incoming auth enabled.

Outbound CGS/API credentials are unchanged (`AuthResolver`, `AuthScheme` in YAML).

## Environment variables

| Variable | Values | Meaning |
|----------|--------|---------|
| `PLASM_INCOMING_AUTH_MODE` | `off` (default), `optional`, `required` | Whether requests must present credentials. |
| `PLASM_AUTH_JWT_SECRET` | string | HMAC key for **HS256** JWTs. |
| `PLASM_AUTH_JWT_ISSUER` | optional string | If set, JWT `iss` must match. |
| `PLASM_AUTH_JWT_AUDIENCE` | optional string | If set, JWT `aud` must match. |
| `PLASM_AUTH_API_KEYS_FILE` | path to JSON file | API keys for inbound auth (see format). |

Startup fails fast if `PLASM_INCOMING_AUTH_MODE=required` but neither `PLASM_AUTH_JWT_SECRET` nor `PLASM_AUTH_API_KEYS_FILE` is set.

## JWT claims (HS256)

Required:

- `sub` — subject
- `tenant_id` (alias `tid`) — tenant scope for execute sessions

Standard `exp` is validated.

## API key file format

JSON array of objects:

```json
[
  { "key": "pk_example", "tenant_id": "tenant-a", "subject": "key-a" }
]
```

Keys are compared in **constant time** against the raw `X-API-Key` header value.

## HTTP

Protected routes: `/v1/registry`, `/v1/registry/:id`, `/v1/discover`, `/v1/incoming-auth/context`, `/execute`, `/execute/...`

Public: `GET /v1/health`

Execute sessions are keyed by **tenant scope** from the principal; cross-tenant access to an existing session returns **403**.

## MCP

**Tenant MCP transport** (graph allowlists for the appliance) is separate: use a provisioned **API key** as `Authorization: Bearer <api_key>` on Streamable HTTP. See [Appliance quick start](../appliance/quickstart.md) and [OSS appliance MCP persistence](oss-appliance-mcp-persistence.md).

**Incoming (inbound) auth** for execute sessions: Streamable HTTP does not pass `Authorization` to tool handlers, so clients must call the tool **`plasm_incoming_auth`** once per MCP transport session with **exactly one** of:

- `bearer_token` — raw JWT string
- `api_key` — raw API key string

When `PLASM_INCOMING_AUTH_MODE=required`, other tools fail until `plasm_incoming_auth` succeeds.

## Dev JWT helper

From the repo root (requires `PLASM_AUTH_JWT_SECRET`):

```bash
./scripts/plasm-dev-auth.sh mint-jwt --tenant my-tenant --sub my-user
```

## Phoenix `web/` UI

For local development, **`just local-web`** exports **`PLASM_WEB_DEV_AUTO_BEARER_TOKEN`** so Phoenix seeds the **browser session** (see `PlasmWebWeb.DevAutoIncomingAuth`) — no manual paste page. With **`PLASM_WEB_INCOMING_LOGIN_MODE=oauth_github`**, sign in via **`/login`** (GitHub OAuth).

The SaaS shell resolves tenant/workspace/project from **`GET /v1/incoming-auth/context`** (Rust-owned principal + workspace/project list). Phoenix does not maintain parallel user or membership tables for that flow.

Configure `PLASM_MCP_HTTP_BASE_URL` (see `web/config/runtime.exs`, default `http://127.0.0.1:3000` with `just local-web`; see [../reference/cli-and-env.md](../reference/cli-and-env.md)).
