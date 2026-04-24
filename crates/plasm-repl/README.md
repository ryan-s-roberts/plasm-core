# plasm-repl

Interactive REPL for exploring expressions against a live or mock backend.

## Design boundary: no domain leakage

Plasm is a **general-purpose language and runtime for API mapping** (schema, expressions, CML, execution). **Domain-specific knowledge is forbidden in this crate:** no branches on particular CGS entity or capability names from `apis/…`, no field-alias or env-key hacks for one vendor’s HTTP templates, and no special transport cases tied to a single product.

Catalog behavior belongs in **`apis/<name>/`**, fixtures, and optional **plugins**—expressed as data and schema-driven rules. Code here stays **agnostic**, driven only by loaded CGS and generic IR/types.

## LLM mode

The default workspace build keeps `:llm` support compiled against the non-LLM `plasm-eval` fallback.
Using `:llm` at runtime requires the generated BAML client:

```bash
baml-cli generate
cargo run -p plasm-repl --features llm -- --schema apis/<name>
```

Install `protoc` before building with `--features llm`; the BAML dependency compiles protobuf
definitions during its build.

See the repository README for workspace build commands.
