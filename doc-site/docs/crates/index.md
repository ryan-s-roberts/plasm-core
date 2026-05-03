# OSS crates

Workspace layout for **[plasm-core](https://github.com/ryan-s-roberts/plasm-core)**. Crate names link to **docs.rs** for the latest published release when available.

| Crate | Role |
|-------|------|
| [**plasm-core**](https://docs.rs/plasm-core) | CGS, AST, typecheck, discovery, DOMAIN rendering — **catalog-agnostic**. |
| [**plasm-cml**](https://docs.rs/plasm-cml) | CML AST and transport parsing (shared with compile). |
| [**plasm-compile**](https://docs.rs/plasm-compile) | Predicates, decoding, template validation. |
| [**plasm-runtime**](https://docs.rs/plasm-runtime) | Execution engine, cache, replay, auth resolution. |
| [**plasm-agent-core**](https://docs.rs/plasm-agent-core) | MCP host, sessions, traces, MCP sqlx metadata, and local service endpoints. |
| [**plasm-agent**](https://docs.rs/plasm-agent) | Re-exports core; ships **`plasm-mcp`**, **`plasm-cgs`**, **`plasm-pack-plugins`**. |
| [**plasm-plugin-abi**](https://docs.rs/plasm-plugin-abi) / **plasm-plugin-host** / **plasm-plugin-stub** | Compile-only plugin ABI and loader. |
| **plasm-repl**, **plasm-cli**, **plasm-eval**, **plasm-e2e**, **plasm-mock** | Tooling and test harnesses (see workspace `Cargo.toml`). |

**Dependency direction (simplified):** `plasm-core` ← `plasm-compile` ← `plasm-runtime` ← `plasm-agent-core` ← `plasm-agent`.

Binaries of interest:

- **`plasm-mcp`** — Streamable HTTP MCP + REST discovery/execute.
- **`plasm-cgs`** — schema-generated CLI.
- **`plasm-repl`** — interactive path expressions.
- **`plasm-pack-plugins`** — pack `apis/<name>/` to `cdylib` plugins.

Optional **features** exist in the workspace; this site does not enumerate experimental feature flags — see each crate’s `Cargo.toml` on GitHub.
