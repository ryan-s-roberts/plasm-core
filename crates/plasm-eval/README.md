# plasm-eval

Deterministic and LLM-backed evaluation harnesses over Plasm schemas and case files.

## Design boundary: no domain leakage

Plasm is a **general-purpose language and runtime for API mapping** (schema, expressions, CML, execution). **Domain-specific knowledge is forbidden in this crate:** no branches on particular CGS entity or capability names from `apis/…`, no field-alias or env-key hacks for one vendor’s HTTP templates, and no special transport cases tied to a single product.

Catalog behavior belongs in **`apis/<name>/`**, fixtures, and optional **plugins**—expressed as data and schema-driven rules. Code here stays **agnostic**, driven only by loaded CGS and generic IR/types.

**LLM eval (`plasm-eval` default run):** all cases execute **in YAML order** on **one BAML `TranslatePlan` transcript** (DOMAIN/schema only in the first user turn; each case appends a `--- GOAL ---` turn, mirroring `plasm-repl` `:llm`). There is no parallel “job” mode.

## Build notes

Default workspace builds do not require generated BAML sources or `protoc`. Coverage, scaffolding,
and dry-run code paths compile against the default `plasm-eval` crate.

LLM-backed eval requires the generated BAML client:

```bash
baml-cli generate
cargo run -p plasm-eval --features llm -- --schema apis/<name> --cases apis/<name>/eval/cases.yaml
```

Install `protoc` before building with `--features llm`; the BAML dependency compiles protobuf
definitions during its build.

See the repository README for workspace build commands.
