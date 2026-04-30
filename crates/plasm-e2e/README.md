# plasm-e2e

End-to-end integration tests (Hermit and related harnesses) for the Plasm stack.

## Design boundary: no domain leakage

Plasm is a **general-purpose language and runtime for API mapping** (schema, expressions, CML, execution). **Domain-specific knowledge is forbidden in this crate:** no branches on particular CGS entity or capability names from `apis/…`, no field-alias or env-key hacks for one vendor’s HTTP templates, and no special transport cases tied to a single product.

Catalog behavior belongs in **`apis/<name>/`**, fixtures, and optional **plugins**—expressed as data and schema-driven rules. Code here stays **agnostic**, driven only by loaded CGS and generic IR/types.

Plasm **surface language + DAG semantics** for real multi-line programs live in [`tests/plasm_language_matrix.rs`](tests/plasm_language_matrix.rs) (Hermit-backed live runs against [`fixtures/schemas/plasm_language_matrix`](../../fixtures/schemas/plasm_language_matrix) / [`fixtures/real_openapi_specs/plasm_language_matrix.yaml`](../../fixtures/real_openapi_specs/plasm_language_matrix.yaml)). Prefer extending that matrix over adding scattered story-style compiler tests elsewhere.

See [AGENTS.md](../../AGENTS.md) for workspace layout and commands.
