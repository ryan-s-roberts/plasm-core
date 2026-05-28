# MCP & credentials

This page is for **operators** running **`plasm-server`**: databases, secrets, **three credential planes**, and OAuth friction.

Quick install: [Appliance quick start](quickstart.md). TUI tour: [Control station](tui.md).

---

## Three credential planes (choose deliberately)

Most production incidents are mixing these up. Separate them up front:

| Plane | What it authenticates | Typical artifact |
|-------|----------------------|------------------|
| **MCP transport** | Caller ↔ **`plasm-server`** MCP listener | Bearer **API key** (required once tenant MCP rows exist; anonymous only when **no** MCP configs are loaded) |
| **Incoming execute identity** | Tenant/user for execute sessions | Optional JWT (`Authorization: Bearer`) and/or `X-API-Key`, MCP tool `plasm_incoming_auth` when enabled |
| **Outbound API credentials** | Appliance ↔ GitHub/Google/etc. | Env PATs, outbound-secret hooks, or OAuth refresh tokens |

Configure **transport keys** and **incoming auth** when you enforce multi-tenant boundaries; configure **outbound** when catalogs hit authenticated vendor APIs.

---

## What you are running

- **`plasm-server`** exposes MCP tools, HTTP discovery/execute, health checks, and optional **`/internal/*`** routes for MCP config/API keys when a SQL URL resolves. See [OSS appliance MCP persistence](../reference/oss-appliance-mcp-persistence.md).
- **Bearer keys:** MCP HTTP clients use **Bearer** API keys once tenant MCP rows exist. Provision keys in the **Keys** tab or via `plasm-server mcp keys add`.
- **OAuth providers:** Register in the **OAuth** tab (**`n`**) or `plasm-server oauth provider upsert`; bind accounts with **`d`** (device flow) or browser link flows per catalog.

---

## Outbound credentials: decision tree

### 1. Prefer PATs and API tokens when you can

Many catalogs accept **static credentials** (personal access tokens, API keys) via environment variables — often **less fragile** than registering your own OAuth client.

**Examples:**

- **GitHub:** set `GITHUB_TOKEN` and point `--backend` at `https://api.github.com`. See `apis/github/README.md` in the repository.
- **GitLab, Slack, and similar:** use vendor PAT/API-key flows on each **`apis/<name>/`** README.

### 2. Use OAuth when delegation or refresh is required

Choose OAuth when the vendor **requires** refreshable user delegation or when PAT scope is insufficient.

Configure providers in the TUI **OAuth** tab or [CLI reference](../reference/appliance-cli.md).

### 3. Google Workspace / Google Cloud OAuth is often painful

Self-hosted appliances frequently hit **Google Cloud OAuth consent screen** and **Workspace admin** constraints:

- **Testing vs production:** Apps in **Testing** only allow configured **test users**.
- **Admin approval:** Workspace admins can block unapproved apps.
- **Redirect URIs:** Must match **exactly** (scheme, host, path). Appliances behind reverse proxies must register the **public** callback URL.

If Google OAuth blocks progress after PAT is not an option, treat OAuth client registration as an **ops project**, not a five-minute step.

---

## Hosted option: Plasm Cloud for OAuth apps

**[platform.plasm.tools](https://platform.plasm.tools)** hosts OAuth provider registration and outbound connection flows for teams that prefer not to own every client ID. Use it when Workspace or cloud-console policy makes **self-service OAuth clients** impractical.

This OSS documentation site does **not** duplicate Plasm Cloud product docs.

---

## Related OSS references

- [OSS appliance MCP persistence](../reference/oss-appliance-mcp-persistence.md) — synthetic tenant, `project_mcp_*`
- [Incoming auth](../reference/plasm-mcp-incoming-auth.md) — optional JWT / API keys
- [Outgoing OAuth promotion](../reference/oss-outgoing-oauth-promotion.md) — hosted vs OSS responsibilities
- [Appliance CLI reference](../reference/appliance-cli.md) — `plasm-server mcp` / `oauth`
