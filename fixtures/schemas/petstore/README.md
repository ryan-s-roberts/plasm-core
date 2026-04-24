# Swagger Petstore — Plasm CGS Schema

A [Plasm](../../README.md) domain model for the [Swagger Petstore](https://petstore.swagger.io/) sample API. This tree lives under **`fixtures/schemas/`** (not **`apis/`**): it is a **test and demo slice**; `plasm-pack-plugins` / `just build-plugins` only pack **`apis/<name>/`**, so Petstore is not a default multi-entry plugin artifact.

```bash
cargo run --bin plasm-agent -- \
  --schema fixtures/schemas/petstore \
  --backend https://petstore.swagger.io/v2 \
  --repl
```

OpenAPI demo service; suitable for integration tests and predicate experiments.
