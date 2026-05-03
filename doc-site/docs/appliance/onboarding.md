# Run the MCP appliance

This page is for **operators** wiring the OSS **`plasm-mcp`** binary (Streamable MCP plus local health/discovery endpoints) to real teams: databases, secrets, **three credential planes**, and OAuth friction.

---

## Three credential planes (choose deliberately)

Most production incidents are mixing these up. Separate them up front:

| Plane | What it authenticates | Typical artifact |
|-------|----------------------|------------------|
| **MCP transport** | Caller ↔ `plasm-mcp` MCP listener | Bearer **API key** (required once tenant MCP rows exist; anonymous only when **no** MCP configs are loaded—see repo **`AGENTS.md`**) |
| **Incoming execute identity** | Tenant/user for execute sessions | JWT (`Authorization: Bearer`) and/or `X-API-Key`, MCP tool `plasm_incoming_auth` when enabled |
| **Outbound API credentials** | Appliance ↔ GitHub/Google/etc. | Env PATs, outbound-secret hooks, or OAuth refresh tokens |

Configure **transport keys** and **incoming auth** when you enforce multi-tenant boundaries; configure **outbound** when catalogs hit authenticated vendor APIs.

---

## What you are running

- **`plasm-mcp`** from the **`plasm-agent`** crate exposes MCP tools, discovery, health checks, and optional **`/internal/*`** routes for MCP config/API keys when a SQL URL resolves. See [OSS appliance MCP persistence](../reference/oss-appliance-mcp-persistence.md).
- **Bearer keys:** MCP HTTP clients use **Bearer** API keys once tenant MCP rows exist; behavior when **no** configs are loaded is documented in **`AGENTS.md`** (local anonymous mode).

---

## Outbound credentials: decision tree

### 1. Prefer PATs and API tokens when you can

Many catalogs accept **static credentials** (personal access tokens, API keys, service account tokens) via environment variables or outbound-secret hooks—often **less fragile** than registering your own OAuth client.

**Examples:**

- **GitHub:** set `GITHUB_TOKEN` (classic or fine-grained PAT with needed scopes) and point `--backend` at `https://api.github.com`. No OAuth app registration on your side. See `apis/github/README.md` in the repository.
- **GitLab, Slack, and similar:** use vendor PAT/API-key flows documented on each **`apis/<name>/`** README where applicable.

### 2. Use OAuth when delegation or refresh is required

Choose OAuth link flows when the vendor **requires** refreshable user delegation or when PAT scope is insufficient—not as the default path for a first integration.

### 3. Google Workspace / Google Cloud OAuth is often painful

Self-hosted appliances frequently hit **Google Cloud OAuth consent screen** and **Workspace admin** constraints:

- **Testing vs production:** Apps in **Testing** only allow configured **test users**; every user must be listed. Moving to **In production** may require **verification** if you use sensitive or restricted scopes.
- **Internal vs external user type:** “Internal” restricts sign-in to your Google Workspace organization but still requires correct consent configuration.
- **Admin approval:** Workspace admins can block unapproved apps or require **domain-wide delegation** patterns that do not match a generic MCP appliance OAuth client.
- **Redirect URIs:** Must match **exactly** (scheme, host, path). Appliances behind reverse proxies must register the **public** callback URL the browser hits, not only loopback.
- **Organization policies:** Policies can deny OAuth client creation or restrict which projects users may use—common in enterprises.

If Google OAuth blocks progress after PAT is not an option, treat OAuth client registration as an **ops project**, not a five-minute step.

---

## Hosted option: Plasm Cloud for OAuth apps

**[platform.plasm.tools](https://platform.plasm.tools)** hosts OAuth provider registration and outbound connection flows for teams that prefer not to own every client ID. Use it when:

- Workspace or cloud-console policy makes **self-service OAuth clients** impractical.
- You want **centralized** provider apps and scope management instead of each appliance operator registering Google/GitHub/Microsoft apps.

This OSS documentation site does **not** duplicate Plasm Cloud product docs; use the platform for live OAuth onboarding.

---

## Related OSS references

- [OSS appliance MCP persistence](../reference/oss-appliance-mcp-persistence.md) — synthetic tenant, `project_mcp_*`, `/internal/mcp-config/v1/*`
- [Incoming auth](../reference/plasm-mcp-incoming-auth.md) — JWT / API keys and MCP `plasm_incoming_auth`
- [Outgoing OAuth promotion](../reference/oss-outgoing-oauth-promotion.md) — hosted vs OSS responsibilities
