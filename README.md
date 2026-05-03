# Plasm

[![GitHub Actions](https://github.com/PlasmTools/plasm-core/actions/workflows/docs.yml/badge.svg?branch=main)](https://github.com/PlasmTools/plasm-core/actions/workflows/docs.yml)

**[Documentation (GitHub Pages)](https://plasmtools.github.io/plasm-core/)** · **[Source](https://github.com/PlasmTools/plasm-core)**

Plasm is a **typed capability graph** (CGS), **wire mappings** (CML), and a **path-expression language** agents use against real APIs: validate before transport, compact session symbols, HTTP and MCP hosts, and curated catalogs under `apis/`. Deep dives—architecture, `plasm-mcp`, execute semantics, authoring, and env flags—live in the docs site above, not in this file.

### Why this exists

Most agent stacks still center on **ad hoc JSON tools**: large schemas in context, fragile emitted payloads, and no shared model of entities and relations across vendors. If you want the **motivation and framing** for a typed interaction layer instead—one graph-shaped contract, validation before wire calls, and a path language that stays stable as you federate catalogs—read **[Plasm: a typed interaction layer for agents working across APIs](https://medium.com/@ryansroberts/plasm-a-typed-interaction-layer-for-agents-working-across-apis-38d9d90066a7)** (Medium). This repo and the docs site are where that story meets the implementation.

## Quick start

Prerequisites: **Docker**, **Elixir**, **Just** and **Rust** (`cargo`). Clone this repo and work from its root.

**OSS appliance (Docker)** — PostgreSQL, packed `apis/` plugins, `plasm-mcp`, and Plasm Desktop in one image ([`docker/README.md`](docker/README.md) for Buildx setup, multi-arch builds, and env overrides):

```bash
docker buildx build -f docker/oss-appliance.Dockerfile -t plasm-oss-appliance:local --load .
docker run --rm \
  -p 4000:4000 -p 3001:3001 -p 3000:3000 \
  -v plasm-oss-data:/data \
  plasm-oss-appliance:local
```

After start: **Desktop** `http://127.0.0.1:4000/`, **HTTP** (health, discovery, execute) on **:3001**, **MCP** (Streamable HTTP) on **:3000**.

**Local desktop dev** — same rough layout from a source checkout: Postgres, trace sink, plugin pack, `plasm-mcp`, and Phoenix via **`just`** ([`desktop/README.md`](desktop/README.md) for split terminals, ports, and Mix-only flows):

```bash
just oss-desktop-dev
```

Full flags, `/execute`, MCP tools, plugins, and catalog workflows are covered in **[the documentation](https://plasmtools.github.io/plasm-core/)**; contributor-oriented commands and boundaries are summarized in [`AGENTS.md`](AGENTS.md). Doc sources: [`doc-site/`](doc-site/README.md).

## API catalogs

Split CGS + CML trees live under [`apis/`](apis/README.md). The links below point at catalogs whose **own README** is the source of truth for **how to run**, **auth env vars**, and **stated scope**—many also spell out **`plasm-eval coverage`** or **`plasm schema validate`** flows. They are the usual “complete enough to trust the README” set, not an exhaustive inventory (see the **[full catalog table](apis/README.md#catalog)** for every directory). **Capability counts** in parentheses are the number of entries under `capabilities:` in that catalog’s `domain.yaml` (what the runtime loads).

**Public (no API key):** [dnd5e](apis/dnd5e/README.md) (60) · [pokeapi](apis/pokeapi/README.md) (97) · [graphqlzero](apis/graphqlzero/README.md) (15) · [hackernews](apis/hackernews/README.md) (8) · [openbrewerydb](apis/openbrewerydb/README.md) (5) · [rickandmorty](apis/rickandmorty/README.md) (6) · [xkcd](apis/xkcd/README.md) (1) · [rawg](apis/rawg/README.md) (2; optional key for rate limits) · [openmeteo](apis/openmeteo/README.md) (1)

**Auth’d integrations (README + eval / validate where noted):** [github](apis/github/README.md) (91) · [clickup](apis/clickup/README.md) (85) · [notion](apis/notion/README.md) (14) · [linear](apis/linear/README.md) (27; [`COVERAGE.md`](apis/linear/COVERAGE.md)) · [gitlab](apis/gitlab/README.md) (42) · [slack](apis/slack/README.md) (57) · [discord](apis/discord/README.md) (135) · [spotify](apis/spotify/README.md) (17) · [reddit](apis/reddit/README.md) (11) · [twitter / X](apis/twitter/README.md) (15) · [tavily](apis/tavily/README.md) (5) · [musixmatch](apis/musixmatch/README.md) (7)

**Google Workspace** (OAuth; each README lists scopes and coverage commands): [gmail](apis/gmail/README.md) (30) · [calendar](apis/google-calendar/README.md) (4) · [docs](apis/google-docs/README.md) (3) · [drive](apis/google-drive/README.md) (45) · [sheets](apis/google-sheets/README.md) (17)

**On-chain (native transport):** [evm-erc20](apis/evm-erc20/README.md) (2) — **intentionally narrow** (ERC-20 balance + `Transfer` logs): it exists to **demonstrate the interface** when the mapping target is **native EVM** (JSON-RPC to a chain URL), not OpenAPI/GraphQL over HTTP. Plasm is **not** HTTP-only—HTTP and GraphQL are the common catalog shapes today; this catalog shows the **same CGS → CML → runtime path** on a **non-HTTP wire**. Broader on-chain surfaces are orthogonal to proving that transport seam. Enable the **`evm`** Cargo feature (see README).

## License

Plasm is licensed under the [Business Source License 1.1](LICENSE). The Change
License is Apache License 2.0 on the Change Date stated in the license.
