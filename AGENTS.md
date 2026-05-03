# Agent Instructions

This repository contains the OSS Plasm compiler/runtime workspace and curated API catalogs under `apis/`.

## API Authoring

When creating or editing an API catalog, follow the local authoring doctrine:

- `.cursor/skills/plasm-authoring/SKILL.md`
- `.cursor/skills/plasm-authoring/reference.md`
- `.cursor/agents/plasm-api-mapping-designer.md`

API authoring is semi-autonomous. Agents may read specs, design entities, edit YAML, run validation, test against mocks, and add eval cases, but `domain.yaml` is a semantic CGS model, not a deterministic OpenAPI dump.

Default loop:

```text
read spec/docs -> design graph -> author domain.yaml -> author mappings.yaml -> validate -> mock/live test -> eval coverage -> iterate
```

Do not add scripts or generator crates that mechanically emit canonical `domain.yaml` or `mappings.yaml` from a spec.

## Validation Commands

Use these commands as appropriate:

```bash
cargo run -p plasm-cli --bin plasm -- schema validate apis/<api>
cargo run -p plasm-cli --bin plasm -- validate --schema apis/<api> --spec path/to/openapi.json
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/<api> --help
cargo run -p plasm-eval -- coverage --schema apis/<api> --cases apis/<api>/eval/cases.yaml
```

Use Hermit for mock-backed transport checks when an OpenAPI spec is available:

```bash
hermit --specs path/to/openapi.json --port 9090 --use-examples
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/<api> --backend http://localhost:9090 <entity> query
```

## Core Boundaries

Prefer catalog edits over core runtime changes. If an API cannot be represented with current CGS/CML/runtime semantics, stop and describe the missing expressiveness before modifying core crates.

Keep secrets out of schema files. Catalog auth reads from environment variables or supported runtime secret providers.
