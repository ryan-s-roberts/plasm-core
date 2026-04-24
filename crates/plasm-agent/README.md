# plasm-agent (compatibility wrapper)

This crate is a thin wrapper around:

- `plasm-agent-core` — open-source host engine
- `plasm-saas` — SaaS / Phoenix control-plane HTTP surface (`/internal/*`)

The historical `plasm_agent` library name is preserved here for stable imports and binary names. See [docs/oss-saas-boundary.md](../../docs/oss-saas-boundary.md) for the full OSS vs SaaS split.