# Plasm Desktop (OSS Phoenix shell)

Single-user **desktop** web UI for OSS `plasm-mcp`: tool catalog (`/tools`), per-catalog tool model (`/tools/:entry_id`), and reverse proxies (`/plasm/mcp`, `/plasm/http/oauth/*`) matching the contract described in `docs/oss-core-ui-surface.md` (in the parent monorepo when this tree is vendored as a submodule).

## Boundaries

- **Data-plane HTTP only** — calls `PlasmDesktop.Mcp.DataPlane` (`GET /v1/registry`, `GET /v1/registry/:id/tool-model`). No `/internal/*` control-plane client.
- **Own DDL** — Ecto migrations live under `priv/repo/migrations/` and create schema `plasm_desktop` plus `desktop_schema_migrations` (public) / application tables under prefix `plasm_desktop`. This is independent of SaaS Phoenix migrations.

## Local development

Requires PostgreSQL and a running OSS `plasm-mcp` (`--http` / `--mcp`).

From the `**plasm-oss/`** checkout root:

**Single terminal (recommended):** Postgres, Iceberg trace sink, `plasm-mcp`, and Phoenix together — durable MCP traces ingest to the sink like monorepo `just local-web`.

```bash
just oss-desktop-dev
```

**Split terminals** (same ports as above):

When running `**just oss-desktop-agent`** or `**just oss-desktop-dev**`, scripts auto-export `**AUTH_STORAGE_ENCRYPTION_KEY**` (stored under `**plasm-oss/.plasm/**`, gitignored) whenever `**DATABASE_URL**` points at Postgres—required for encrypted auth KV / durable MCP API keys.

```bash
just oss-desktop-db
just oss-desktop-pack-plugins   # once; populates ../target/plasm-plugins
just oss-desktop-trace-sink     # terminal A — PLASM_TRACE_SINK_URL http://127.0.0.1:7070
just oss-desktop-agent          # terminal B — HTTP :3000, Streamable MCP :3001 (JWT + trace sink URL)
just oss-desktop-web            # terminal C — Phoenix on :4000 (see config/dev.exs)
```

Defaults assume Postgres on `**127.0.0.1:5433**` (`OSS_DESKTOP_PG_PORT`), matching `config/dev.exs` via `DESKTOP_PG_PORT` / `OSS_DESKTOP_PG_PORT`. Override with `DATABASE_URL` if needed.

Connection URLs and credentials can be edited in the UI at `**/settings**` (persisted in `plasm_desktop.desktop_settings`). Runtime `**PLASM_MCP_***` and `**PLASM_DESKTOP_***` env vars override saved values.

Manual Mix-only workflow (agent + optional trace sink must already be running):

```bash
cd desktop
mix deps.get
mix assets.build
mix ecto.create && mix ecto.migrate
export PLASM_MCP_HTTP_BASE_URL=http://127.0.0.1:3000
export PLASM_MCP_UPSTREAM_URL=http://127.0.0.1:3001
export PLASM_MCP_PUBLIC_BASE_URL=http://127.0.0.1:3001/mcp
mix phx.server
```

Open `http://127.0.0.1:4000/` by default (tool explorer, traces, settings). Override with `**PORT**` — `config/dev.exs` reads `System.get_env("PORT")`.

## Production release

```bash
MIX_ENV=prod mix release plasm_desktop
```

Runtime env (see `config/runtime.exs`): `DATABASE_URL`, `SECRET_KEY_BASE`, `PORT`, `PHX_HOST`, `PLASM_MCP_HTTP_BASE_URL`, `PLASM_MCP_UPSTREAM_URL`, `PLASM_MCP_PUBLIC_BASE_URL`, optional `PLASM_DESKTOP_BEARER_TOKEN`.

Migrate:

```bash
bin/plasm_desktop eval PlasmDesktop.Release.migrate
```

