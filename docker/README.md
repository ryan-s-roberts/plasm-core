# OSS appliance container

Independent distribution image built **only** from this repository (plasm-core):

- **PostgreSQL 15** — data directory on `/data/postgres`
- **OSS `plasm-mcp`** — packed ABI plugins from `./apis`, HTTP `:3001`, MCP `:3000`
- **Plasm Desktop** — Phoenix release from `./desktop`, HTTP `:4000`

Trace/run persistence on the mounted volume (archive-only, no trace-sink):

| Host mount path | Purpose |
|-----------------|--------|
| `/data/postgres` | PostgreSQL cluster (`PGDATA`) |
| `/data/plasm/trace-archive` | `PLASM_TRACE_ARCHIVE_DIR` |
| `/data/plasm/run-artifacts` | `PLASM_RUN_ARTIFACTS_DIR` |

First-boot secrets (`SECRET_KEY_BASE`, `PLASM_AUTH_JWT_SECRET`) are generated under `/data/plasm/secrets/` if not supplied.

## Build

From the **repository root** (`plasm-core` checkout). The image expects `desktop/` plus the path dependency **`elixir/plasm_ui_core`** (copied by `docker/oss-appliance.Dockerfile`).

**Recommended — Buildx + cross-compiled Rust:** the Dockerfile pins the Rust stage to `BUILDPLATFORM` and uses **`cargo zigbuild`** so `linux/arm64` images are produced without QEMU-emulating the full Cargo graph. The Phoenix release and Debian runtime stages use **`TARGETPLATFORM`** so BEAM matches each slice.

Ensure a Buildx builder exists (once):

```bash
docker buildx create --name plasm-oss --driver docker-container --bootstrap --use 2>/dev/null || docker buildx use plasm-oss
```

**Multi-arch manifest** (typical CI / registry push; compiles **two** slices — native `cargo build` per matching host, **`cargo zigbuild`** when the slice arch differs from `BUILDPLATFORM`):

```bash
docker buildx bake -f docker/oss-appliance-bake.hcl --push TAG=ghcr.io/<org>/plasm-oss-appliance:v0.1.0
```

**Single platform load into local Docker** (`--load` allows one platform only):

```bash
docker buildx build -f docker/oss-appliance.Dockerfile -t plasm-oss-appliance:local \
  --platform linux/arm64 --load .
```

If Debian `apt-get` or `curl` (Zig tarball) fails with transient HTTP/mirror errors, retry the build.

Zig is fetched from `ziglang.org` in the Rust stage because Bookworm’s `apt` `zig` package is missing on some architectures; the final image does not include Zig.

## Run

```bash
docker run --rm \
  -p 4000:4000 -p 3001:3001 -p 3000:3000 \
  -v plasm-oss-data:/data \
  plasm-oss-appliance:local
```

Optional overrides:

- `DATABASE_URL` — default `postgresql://postgres@127.0.0.1:5432/plasm_appliance`
- `PLASM_AUTH_STORAGE_URL` — defaults to `DATABASE_URL`
- `PLASM_DESKTOP_BEARER_TOKEN` — session bearer for agent HTTP when inbound auth is enabled
- `PLASM_INCOMING_AUTH_MODE` — default `optional`
- `PUBLIC_WEB_ORIGIN` — influences default `PLASM_MCP_PUBLIC_BASE_URL` (default `http://127.0.0.1:${PORT}`)

## GHCR / CI

Publish with your registry prefix, for example:

```bash
docker tag plasm-oss-appliance:local ghcr.io/<org>/plasm-oss-appliance:v0.1.0
docker push ghcr.io/<org>/plasm-oss-appliance:v0.1.0
```

Wire this command in plasm-core GitHub Actions (not included here) on release tags.
