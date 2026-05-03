# CLI flags and environment (index)

!!! tip "Read this when…"

    You need an env var, migration flag, trace hub knob, or execute/MCP switch and want the repo-truth pointer.

**What to know first:** [Start here](../getting-started.md) for ports and basic `plasm-mcp` invocation.

**Practical takeaway:** This page is an **index**—single source of truth remains **`AGENTS.md`** on the branch you ship.

---

Authoritative, maintained lists live in the repository root **`AGENTS.md`**:

- **HTTP / MCP** ports, `--plugin-dir`, `--schema`, execute `Accept` negotiation, MCP tools.
- **Trace hub** caps: `PLASM_TRACE_HUB_*`.
- **Run artifacts:** `PLASM_RUN_ARTIFACTS_URL`, retention and GC intervals.
- **MCP config DB:** `DATABASE_URL`, `PLASM_MCP_CONFIG_DATABASE_URL`, migrate flag.
- **Incoming auth:** `PLASM_INCOMING_AUTH_MODE`, `PLASM_AUTH_JWT_SECRET`, `PLASM_AUTH_API_KEYS_FILE`.

This page is an **index only** so the published site stays in sync with the repo without duplication.

**Links**

- [`AGENTS.md` (main branch)](https://github.com/ryan-s-roberts/plasm-core/blob/main/AGENTS.md)
- [Incoming authentication](plasm-mcp-incoming-auth.md)
- [OSS appliance MCP persistence](oss-appliance-mcp-persistence.md)
- [Trace artifacts](oss-core-trace-artifacts.md)
