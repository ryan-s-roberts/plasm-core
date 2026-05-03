# Plasm OSS appliance dev: use `just oss-desktop-dev` — packs apis→plugins if needed, trace sink, plasm-mcp (--plugin-dir only), Phoenix.
# Run from plasm-core checkout root (this directory).

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

root := justfile_directory()
export PATH := env_var_or_default("PATH", "/usr/bin:/bin")

default:
	@just --list

# Same as `just oss-desktop-dev` — also available from monorepo root as `just dev`.
dev: oss-desktop-dev

# Docker Postgres on localhost:5433 → DB plasm_desktop_dev
oss-desktop-db:
	bash "{{root}}/scripts/oss-desktop-postgres.sh"

# Pack apis/* into target/plasm-plugins (release profile unless PLASM_OSS_RUST_DEBUG=1). Fails if no dylibs produced.
# Note: avoid bash "${array[@]}" here — Just treats `@` as recipe syntax.
oss-desktop-pack-plugins:
	bash -c 'set -euo pipefail; cd "{{root}}"; mkdir -p "{{root}}/target/plasm-plugins"; if [[ -z "$${PLASM_OSS_RUST_DEBUG:-}" ]]; then cargo run --release -p plasm-agent --bin plasm-pack-plugins -- --workspace "{{root}}" --apis-root "{{root}}/apis" --output-dir "{{root}}/target/plasm-plugins" --release; else cargo run -p plasm-agent --bin plasm-pack-plugins -- --workspace "{{root}}" --apis-root "{{root}}/apis" --output-dir "{{root}}/target/plasm-plugins" --release; fi; if ! find "{{root}}/target/plasm-plugins" -maxdepth 1 \( -name "libplasm_plugin_*.dylib" -o -name "libplasm_plugin_*.so" -o -name "libplasm_plugin_*.dll" \) | grep -q .; then echo "oss-desktop-pack-plugins: no dylibs in {{root}}/target/plasm-plugins — apis/ may be empty (init submodule / catalogs)." >&2; exit 1; fi'

# Foreground trace sink :7070 (Iceberg + desktop Postgres catalog). Pair with oss-desktop-agent for durable traces.
oss-desktop-trace-sink:
	bash "{{root}}/scripts/oss-desktop-trace-sink.sh"

# OSS plasm-mcp: HTTP :3000, Streamable MCP :3001 (override OSS_DESKTOP_AGENT_HTTP_PORT / OSS_DESKTOP_AGENT_MCP_PORT).
oss-desktop-agent:
	bash "{{root}}/scripts/oss-desktop-agent.sh"

# Phoenix desktop only (use oss-desktop-dev for sink + agent + web together).
oss-desktop-web:
	bash -c 'set -euo pipefail; source "{{root}}/scripts/oss-desktop-control-plane-secret.sh"; cd "{{root}}/desktop"; _pg="$${OSS_DESKTOP_PG_PORT:-5433}"; : "$${DATABASE_URL:=postgresql://postgres:postgres@127.0.0.1:$${_pg}/plasm_desktop_dev}"; : "$${PLASM_MCP_HTTP_BASE_URL:=http://127.0.0.1:3000}"; : "$${PLASM_MCP_UPSTREAM_URL:=http://127.0.0.1:3001}"; : "$${PLASM_MCP_PUBLIC_BASE_URL:=http://127.0.0.1:3001/mcp}"; export DATABASE_URL PLASM_MCP_HTTP_BASE_URL PLASM_MCP_UPSTREAM_URL PLASM_MCP_PUBLIC_BASE_URL; mix deps.get; mix ecto.migrate; mix phx.server'

# Postgres + trace sink + plasm-mcp + Phoenix (single terminal; Ctrl+C tears down sink + agent).
oss-desktop-dev:
	bash "{{root}}/scripts/oss-desktop-dev.sh"
