# OSS appliance: PostgreSQL + OSS plasm-mcp + PlasmDesktop Phoenix release.
#
# Multi-arch / Buildx: Rust stage uses `BUILDPLATFORM` (fast compile host). When it matches
# `TARGETPLATFORM`, use plain `cargo build`. Otherwise cross-link with `cargo zigbuild`
# plus Debian multiarch `libssl-dev:<arch>` (openssl-sys headers/libs for the target).
#
# Single-arch:
#   docker buildx build -f docker/oss-appliance.Dockerfile -t plasm-oss-appliance:local --load .
# Multi-arch manifest (push-capable):
#   docker buildx bake -f docker/oss-appliance-bake.hcl --push
#
#syntax=docker/dockerfile:1.6

# --- Rust: cross-compile OSS plasm-mcp + plugin pack (host arch = fast compile) ---
FROM --platform=$BUILDPLATFORM rust:1.91-bookworm AS rust-builder
ARG TARGETPLATFORM

# Bookworm apt often has no `zig` on arm64; pin Zig from ziglang.org so zigbuild works on every BUILDPLATFORM.
ARG ZIG_VERSION=0.13.0
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates xz-utils pkg-config protobuf-compiler libssl-dev \
    && rm -rf /var/lib/apt/lists/* \
    && ARCH=$(uname -m) \
    && curl -fsSL "https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${ARCH}-${ZIG_VERSION}.tar.xz" -o /tmp/zig.txz \
    && tar -xJf /tmp/zig.txz -C /opt \
    && ln -sf "/opt/zig-linux-${ARCH}-${ZIG_VERSION}/zig" /usr/local/bin/zig \
    && rm /tmp/zig.txz

RUN cargo install cargo-zigbuild --locked \
    && rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apis ./apis

# Cache downloaded crates only — do not cache `target/` here: a shared target cache breaks
# multi-platform builds (build-script / openssl-sys artifacts collide across TARGETPLATFORM).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    case "$TARGETPLATFORM" in \
      linux/amd64) RUST_TARGET=x86_64-unknown-linux-gnu ;; \
      linux/arm64) RUST_TARGET=aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported TARGETPLATFORM=$TARGETPLATFORM (expected linux/amd64 or linux/arm64)" >&2; exit 1 ;; \
    esac; \
    BUILD_M=$(uname -m); \
    NATIVE=0; \
    case "$BUILD_M:$TARGETPLATFORM" in \
      x86_64:linux/amd64|aarch64:linux/arm64) NATIVE=1 ;; \
    esac; \
    if [ "$NATIVE" = 1 ]; then \
      cargo build --release -p plasm-agent --bin plasm-mcp --bin plasm-pack-plugins; \
      OUT=/build/target/release; \
    else \
      apt-get update; \
      apt-get install -y --no-install-recommends dpkg-dev; \
      case "$TARGETPLATFORM" in \
        linux/arm64) \
          dpkg --add-architecture arm64; \
          apt-get update; \
          apt-get install -y --no-install-recommends libssl-dev:arm64; \
          export OPENSSL_INCLUDE_DIR=/usr/include/aarch64-linux-gnu OPENSSL_LIB_DIR=/usr/lib/aarch64-linux-gnu; \
          if [ -e /usr/include/openssl ]; then mv /usr/include/openssl /usr/include/openssl.plasm-host; fi; \
          cp -a /usr/include/aarch64-linux-gnu/openssl /usr/include/openssl; \
          ;; \
        linux/amd64) \
          dpkg --add-architecture amd64; \
          apt-get update; \
          apt-get install -y --no-install-recommends libssl-dev:amd64; \
          export OPENSSL_INCLUDE_DIR=/usr/include/x86_64-linux-gnu OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu; \
          if [ -e /usr/include/openssl ]; then mv /usr/include/openssl /usr/include/openssl.plasm-host; fi; \
          cp -a /usr/include/x86_64-linux-gnu/openssl /usr/include/openssl; \
          ;; \
      esac; \
      cargo zigbuild --release -p plasm-agent --bin plasm-mcp --bin plasm-pack-plugins --target "${RUST_TARGET}"; \
      OUT="/build/target/${RUST_TARGET}/release"; \
    fi; \
    mkdir -p /out/plugins; \
    "${OUT}/plasm-pack-plugins" \
      --workspace /build \
      --apis-root /build/apis \
      --output-dir /out/plugins \
      --release; \
    cp "${OUT}/plasm-mcp" /out/plasm-mcp; \
    rm -rf /var/lib/apt/lists/*

# --- Phoenix desktop release (target arch; implicit TARGETPLATFORM under buildx) ---
FROM hexpm/elixir:1.18.4-erlang-27.3.4.6-debian-bookworm-20260316 AS elixir-builder
RUN apt-get update && apt-get install -y --no-install-recommends build-essential git \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# Path dependency in desktop/mix.exs: {:plasm_ui_core, path: "../elixir/plasm_ui_core"}
COPY elixir/plasm_ui_core /elixir/plasm_ui_core
COPY desktop/mix.exs desktop/mix.lock ./
RUN mix local.hex --force && mix local.rebar --force && mix deps.get --only prod
COPY desktop/config ./config
COPY desktop/lib ./lib
COPY desktop/priv ./priv
ENV MIX_ENV=prod
RUN mix compile --max-jobs 1 && mix release

# --- Runtime (target arch) ---
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    postgresql-15 \
    curl \
    libssl3 ca-certificates gettext-base procps \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 1000 -m -s /bin/bash plasm

COPY --from=rust-builder /out/plasm-mcp /usr/local/bin/plasm-mcp
COPY --from=rust-builder /out/plugins /app/plugins
COPY --from=elixir-builder /app/_build/prod/rel/plasm_desktop /app/plasm_desktop

COPY docker/oss-appliance-entrypoint.sh /usr/local/bin/oss-appliance-entrypoint.sh
RUN chmod +x /usr/local/bin/oss-appliance-entrypoint.sh \
    && chown -R plasm:plasm /app/plugins /app/plasm_desktop

ENV PGDATA=/data/postgres \
    PG_MAJOR=15 \
    LANGUAGE=en_US.UTF-8 \
    LANG=en_US.UTF-8

EXPOSE 4000 3000 3001

ENTRYPOINT ["/usr/local/bin/oss-appliance-entrypoint.sh"]
