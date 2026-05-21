# plasm-discovery

Stepwise typed discovery over CGS catalogs: intent decomposition, phrase / lexical indexes, graph-aware qualifier checks, and clarification gates (`AgentDiscovery`).

OSS release binaries are **lexical-only** (`enable_embeddings` defaults to **false**). Optional Cargo feature **`local-embeddings`** enables CPU `fastembed` rerank on existing lexical hits (requires ONNX at build time). When the feature is off, `enable_embeddings: true` is a no-op.

See repository docs for HTTP `/v1/discover-typed` and MCP `discover_capabilities` with `typed: true`.
