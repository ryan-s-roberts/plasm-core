# Plasm

[![GitHub Actions](https://github.com/PlasmTools/plasm-core/actions/workflows/docs.yml/badge.svg?branch=main)](https://github.com/PlasmTools/plasm-core/actions/workflows/docs.yml)

**[Documentation (GitHub Pages)](https://plasmtools.github.io/plasm-core/)** · **[Source](https://github.com/PlasmTools/plasm-core)**

Plasm is a **typed capability graph** (CGS), **wire mappings** (CML), and a **path-expression language** agents use against real APIs: validate before transport, compact session symbols, HTTP and MCP hosts, and curated catalogs under `apis/`. Deep dives—architecture, `plasm-mcp`, execute semantics, authoring, and env flags—live in the docs site above, not in this file.

### Why this exists

Most agent stacks still center on **ad hoc JSON tools**: large schemas in context, fragile emitted payloads, and no shared model of entities and relations across vendors. If you want the **motivation and framing** for a typed interaction layer instead—one graph-shaped contract, validation before wire calls, and a path language that stays stable as you federate catalogs—read **[Plasm: a typed interaction layer for agents working across APIs](https://medium.com/@ryansroberts/plasm-a-typed-interaction-layer-for-agents-working-across-apis-38d9d90066a7)** (Medium). This repo and the docs site are where that story meets the implementation.

## Quick start

Prerequisites: **Rust** (`cargo`). Clone this repo and work from its root.

```bash
cargo build --workspace
cargo run -p plasm-cli --bin plasm -- schema validate apis/dnd5e
cargo run -p plasm-repl -- --schema apis/dnd5e --backend https://www.dnd5eapi.co
```

**`plasm-mcp`** (HTTP + Streamable HTTP MCP on separate ports):

```bash
cargo build -p plasm-agent --release --bin plasm-mcp
./target/release/plasm-mcp --schema apis/dnd5e --http --port 3001 --mcp --mcp-port 3000
```

Then `curl -sS http://127.0.0.1:3001/v1/health` should report `ok`.

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

## License

Plasm is licensed under the [Business Source License 1.1](LICENSE). The Change
License is Apache License 2.0 on the Change Date stated in the license.
