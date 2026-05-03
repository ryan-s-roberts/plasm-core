# Start here

**Outcome:** From a **[plasm-core](https://github.com/ryan-s-roberts/plasm-core)** checkout you will (1) prove a catalog validates, (2) run one successful REPL query against a public HTTP API, and (3) know how to start `plasm-mcp` with separate health/discovery and MCP listeners—then pick your next path.

**Prerequisites:** Rust toolchain (`cargo`), repo cloned with `apis/` populated.

---

## Step 1 — Validate a catalog

```bash
cargo run -p plasm-cli -- schema validate apis/dnd5e/domain.yaml
```

**Verify:** Exit code `0` and no validation errors. Swap `dnd5e` for another catalog under `apis/<name>/` when exploring.

---

## Step 2 — REPL: one live read against the wire

Examples below use **public HTTP** backends only:

```bash
cargo run -p plasm-agent --bin plasm-repl -- \
  --schema apis/dnd5e \
  --backend https://www.dnd5eapi.co
```

At the `repl>` prompt, run a simple query aligned with that catalog (for example list spells or read one spell—see the catalog’s DOMAIN or README if unsure).

**Verify:** You see decoded rows (no transport errors). That confirms **CGS + CML + runtime + backend** agree for this catalog.

Other quick sandboxes:

```bash
cargo run -p plasm-agent --bin plasm-repl -- \
  --schema apis/rickandmorty \
  --backend https://rickandmortyapi.com/api
```

**Authenticated APIs** (e.g. GitHub): set the token env vars described in `apis/<name>/README.md` before passing `--backend`.

---

## Step 3 — Static CLI smoke test (`plasm-cgs`)

Optional one-shot command without the REPL:

```bash
cargo run -p plasm-agent --bin plasm-cgs -- \
  --schema apis/pokeapi \
  --backend https://pokeapi.co \
  pokemon pikachu
```

**Verify:** Printed JSON (or table-style output) for the requested capability—confirms compile + HTTP path without MCP.

---

## Step 4 — Start `plasm-mcp` (two listeners)

Build once, then run **both** transports:

```bash
cargo build -p plasm-agent --release --bin plasm-mcp
./target/release/plasm-mcp --schema apis/dnd5e --http --port 3001 --mcp --mcp-port 3000
```

**Verify:**


| Listener              | Default in this example | Quick check                                                                         |
| --------------------- | ----------------------- | ----------------------------------------------------------------------------------- |
| Health/discovery      | `--port` → **3001**     | `curl -sS http://127.0.0.1:3001/v1/health` → JSON `{"status":"ok"}`                  |
| MCP (Streamable HTTP) | `--mcp-port` → **3000** | Point an MCP client at the configured path (often `/mcp`); see repo `**AGENTS.md`** |


**Important:** `--http` and `--mcp` must **not** share one port; raise `--mcp-port` if it collides.

Full flag matrix, execute `Accept` negotiation, MCP tool semantics, and env vars are maintained in repository `**AGENTS.md`** (too large to mirror here).

---

## Step 5 — Multi-entry catalogs (plugins)

Pack YAML catalogs to ABI v4 `cdylib`s and run with `**--plugin-dir`**:

- Catalog index: [Catalogs](reference/apis-readme.md)
- Packing and loader behavior: `**AGENTS.md**`

**Verify:** `GET` discovery lists multiple `entry_id`s when several plugins load.

---

## Docker appliance

**One container:** embedded PostgreSQL, OSS **`plasm-mcp`** (**HTTP + MCP**), and **Phoenix Desktop**—built from `docker/oss-appliance.Dockerfile` in **[plasm-core](https://github.com/ryan-s-roberts/plasm-core)**.

**Prerequisites:** Docker with Buildx; clone **plasm-core** with `desktop/` and `elixir/plasm_ui_core` present (the image copies both).

### Build and run

From the **repository root** of plasm-core:

```bash
docker buildx build -f docker/oss-appliance.Dockerfile -t plasm-oss-appliance:local \
  --platform linux/arm64 --load .

docker run --rm \
  -p 4000:4000 -p 3001:3001 -p 3000:3000 \
  -v plasm-oss-data:/data \
  plasm-oss-appliance:local
```

Multi-arch bake, CI publishing, and Zig/cross-compile notes live in **`docker/README.md`** in the repo.

### Published ports

| Port | Service |
|------|---------|
| **4000** | Plasm Desktop (Phoenix); bind `PHX_HOST` / `PORT` if you change defaults |
| **3001** | Agent HTTP plane (discovery, execute, `/internal/*` when configured) |
| **3000** | MCP Streamable HTTP (path **`/mcp`**); **`GET /health`** for liveness |

Quick checks from the host:

```bash
curl -sS http://127.0.0.1:3001/v1/health
curl -sS http://127.0.0.1:3000/health
```

### Storage on `/data` (mount a volume or bind-mount)

Persist **`/data`** so Postgres, trace archives, run snapshots, and generated secrets survive container restarts.

| Path (inside container) | Role |
|-------------------------|------|
| **`/data/postgres`** | PostgreSQL **cluster directory** (`PGDATA`). Holds the on-disk database files for the embedded server. |
| **`postgresql://…/plasm_appliance`** | Logical database **`plasm_appliance`** (created on first boot). Shared by Phoenix (Ecto) and `plasm-mcp` (`DATABASE_URL` / `project_mcp_*` sqlx). Not a separate folder—it lives under the cluster above. |
| **`/data/plasm/trace-archive`** | **`PLASM_TRACE_ARCHIVE_DIR`** — durable **trace** history on disk (completed traces under `traces/{tenant_id}/{trace_id}/`). See [Trace & run artifacts](reference/oss-core-trace-artifacts.md). |
| **`/data/plasm/run-artifacts`** | **`PLASM_RUN_ARTIFACTS_DIR`** — filesystem-backed **execute run snapshots** / plan material when you are not using `PLASM_RUN_ARTIFACTS_URL` object storage. Same reference doc. |
| **`/data/plasm/secrets/`** | Auto-generated on first boot if unset: `secret_key_base`, `plasm_auth_jwt_secret` (and optional overrides you inject via env). |

You may override directories with **`PLASM_TRACE_ARCHIVE_DIR`**, **`PLASM_RUN_ARTIFACTS_DIR`**, **`DATABASE_URL`**, **`PLASM_AUTH_STORAGE_URL`**, and **`SECRET_KEY_BASE`** / **`PLASM_AUTH_JWT_SECRET`** as described in **`docker/README.md`** and [Run the MCP appliance](appliance/onboarding.md).

---

## Choose your next path


| Goal                                           | Go to                                                                       |
| ---------------------------------------------- | --------------------------------------------------------------------------- |
| Docker image, `/data` layout, env overrides    | **Start here** → [Docker appliance](#docker-appliance); repo `docker/README.md` |
| Credentials, OAuth vs PAT, Postgres MCP policy | [Run the MCP appliance](appliance/onboarding.md)                            |
| Mental model (CGS / CML / Plasm / runtime)     | [Concepts](concepts.md)                                                     |
| Author `domain.yaml` + `mappings.yaml`         | [Connect an API](authoring/index.md)                                        |
| Incoming JWT / API keys                        | [Incoming auth](reference/plasm-mcp-incoming-auth.md)                       |
| `project_mcp_`* persistence                    | [OSS appliance MCP persistence](reference/oss-appliance-mcp-persistence.md) |


