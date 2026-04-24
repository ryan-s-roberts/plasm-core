# plasm-core (open source)

Rust workspace for the Plasm language: schema / CGS (`plasm-core`), compile + CML (`plasm-compile`, `plasm-cml`), runtime, plugin host, and the **OSS** `plasm-mcp` / `plasm-agent` data plane (HTTP discovery + execute + Streamable HTTP MCP, without the hosted `/internal/*` control-plane stack).

The private product monorepo composes this tree as a **git submodule** and adds `plasm-saas` for Phoenix-facing control-plane routes.

## Build

Prerequisites:

- Rust stable with Cargo.
- `protoc` only when building LLM/BAML tooling with `--features llm`.

```bash
cargo build --workspace --locked
cargo test --workspace --locked
cargo fmt --all -- --check
```

CI runs the default formatting, locked metadata, and locked workspace checks on pull requests and
pushes to `main`. It also generates BAML clients and checks the LLM eval/REPL feature path.

The default workspace build does not require generated BAML sources. To use LLM eval or REPL mode,
install `protoc`, run `baml-cli generate` from the repository root, then build the relevant crate with
the `llm` feature.

## `plasm-mcp` (OSS)

`plasm-mcp` in this repository uses the OSS HTTP stack from `plasm-agent-core` (no `plasm-saas`). For the full hosted router, build from the private super-repo.

## License

See [LICENSE](LICENSE).
