# OSS appliance MCP persistence — synthetic tenant + sqlx tables

This document fixes **where MCP policy lives** for the single-user **OSS appliance** (`plasm-server`): **no parallel policy store** in bespoke schemas; **one synthetic tenant row** in the **`project_mcp_*` tables** the appliance reads via sqlx.

**Related:** [Outgoing OAuth promotion](oss-outgoing-oauth-promotion.md), [Appliance surface inventory](appliance-surface-inventory.md).

---

## Canonical store

| Concern | Location |
|--------|----------|
| MCP allowlists, capability/auth bindings, API key hashes | **`project_mcp_*`** tables applied by **`plasm-agent-core` migrations** (e.g. `project_mcp_configs`, `project_mcp_allowed_graphs`, related auth rows). |
| Runtime reads | **`plasm-server`** process via sqlx (`McpRuntimeConfig` → `mcp_policy`). |

The appliance **does not introduce** a second logical model (no duplicated allowlist columns in `desktop_settings`, no separate “appliance policy” table).

---

## Single synthetic tenant

SaaS binds MCP config to `(tenant_id, workspace_slug, project_slug)`. The appliance uses **exactly one** intended configuration:

- **Stable identifiers** — defaults: `PLASM_APPLIANCE_MCP_TENANT_ID` (`appliance-local`), `PLASM_APPLIANCE_MCP_WORKSPACE_SLUG` / `PLASM_APPLIANCE_MCP_PROJECT_SLUG` (`default`). Every upsert must reuse the same triple and config UUID.
- **Invariant** — at most **one** active appliance MCP policy row for that triple; the TUI and `plasm-server mcp` CLI edit **that** row’s graphs and secrets.

This keeps Elixir and Rust aligned with existing uniqueness indexes (e.g. tenant/workspace/project) without inventing a new key space.

---

## Physical database topology

| Component | Rule |
|-----------|------|
| **`plasm-server`** | Uses `DATABASE_URL` (or embedded Postgres autostart) → **same Postgres** that holds `project_mcp_*`. |
| **Policy writes** | Target **`project_mcp_*` only** via the TUI, `plasm-server mcp …`, or secure `/internal/*` upserts when listeners are enabled. |

---

## Secure upsert path

When `PLASM_MCP_CONFIG_DATABASE_URL` / `PLASM_AUTH_STORAGE_URL` / `DATABASE_URL` resolves, **`plasm-server`** connects to **`project_mcp_*`**, may mount `/internal/mcp-config/v1/*` and `/internal/mcp-api-key/v1/*`, and wires MCP transport API keys (Postgres auth KV when available, otherwise **in-memory** keys only until restart). Apply migrations with **`plasm-server mcp migrate-db`** or **`--migrate-mcp-config-db`**.

**Guards for `/internal/*` upserts:**

- **Shared secret** header (`X-Plasm-Control-Plane-Secret`) **or**
- **Loopback-only** listener **or**
- Both — defense in depth for a machine-local appliance.

---

## Duplication rule (explicit)

| Allowed | Rejected |
|---------|----------|
| One row in `project_mcp_configs` (+ children) for the synthetic triple | Mirroring allowlists into ad-hoc local config files |
| Agent migrations as DDL authority | A second “appliance allowlist” table maintained by hand |

---

## Summary

**Canonical MCP policy for the appliance = sqlx `project_mcp_*`, keyed by one synthetic tenant triple, updated through the TUI, `plasm-server mcp` CLI, or secure `/internal/*` upserts — never a parallel policy model.**

---

## Operator notes (appliance schema)

The OSS appliance ships **one** idempotent sqlx migration covering `project_mcp_*`, discovery embeddings, and `oauth_provider_apps`. After a schema squash on unreleased builds, wipe embedded data once:

```bash
rm -rf ~/.plasm/appliance/postgres
plasm-server
```

`plasm-server mcp migrate-db` is safe to re-run on a healthy database (idempotent `IF NOT EXISTS` DDL).
