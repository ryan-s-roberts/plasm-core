---

## name: plasm-authoring
description: Author and validate Plasm API catalogs. Use when creating or editing apis//domain.yaml, apis//mappings.yaml, eval cases, or mock-backed API validation.

# Plasm Authoring

Plasm API authoring is semi-autonomous, not fully automatic. An agent can read specs, propose entity boundaries, write YAML, run validators, test against mocks, and iterate, but `domain.yaml` is a semantic model. OpenAPI, GraphQL SDL, and vendor docs do not uniquely determine the right CGS.

The two authored files are:

- `domain.yaml`: CGS, the domain model. It declares entities, fields, relations, capability kinds, typed parameters, auth, and side-effect/projection semantics.
- `mappings.yaml`: CML, the transport model. It maps each capability to HTTP/GraphQL requests, query/body/path templates, response shape, and pagination.

Runtime semantics are derived from those files. Pagination comes from CML `pagination` blocks. Hydration happens when an entity has both query and get capabilities, unless the caller asks for summary output.

## Core Doctrine

- Do not build scripts, generators, or bulk templates that emit canonical `domain.yaml` / `mappings.yaml` from a spec as if the mapping were deterministic.
- Do not mirror every RPC operation mechanically. Compress the API into entities, relations, projections, and capabilities that are useful for agents.
- Keep HTTP details out of CGS descriptions. Put paths, methods, status codes, and wire notes in `mappings.yaml` comments or external docs.
- Prefer typed fields and parameters: `select`, `date` with `value_format`, `entity_ref`, `array.items`, `blob`, and explicit `string_semantics`. In split `domain.yaml`, declare shapes under **`values:`** and use **`value_ref`** on fields/parameters; keys are **semantic slots** (sharing vs splitting is judgement, not wire-type dedupe).
- Every semantic catalog change increments top-level `version`.
- `kind: action` must declare either non-empty `provides:` or `output: { type: side_effect, description: "..." }`.

## The Loop

```text
1. READ spec/docs
2. DESIGN entity graph and capability families
3. AUTHOR domain.yaml
4. AUTHOR mappings.yaml
5. VALIDATE with the compiler
6. TEST against Hermit or a live/replay backend
7. ADD eval cases for model conformance
8. ITERATE until validation, mock tests, and coverage pass
```

## Authoring With Agents

For Cursor, use the companion agent:

- `.cursor/agents/plasm-api-mapping-designer.md`

For Claude Code, Codex, or another coding agent, point it at:

- `AGENTS.md`
- `CLAUDE.md`
- `.cursor/skills/plasm-authoring/SKILL.md`
- `.cursor/skills/plasm-authoring/reference.md`

Give the agent a small scope and a spec path or docs URL. Ask it to proceed in phases: plan first, then author, then validate, then test, then eval. Do not paste a separate per-API playbook unless that API has genuinely unusual semantics.

## Required Commands

Use these as the default validation rites:

```bash
cargo run -p plasm-cli --bin plasm -- schema validate apis/<api>
cargo run -p plasm-cli --bin plasm -- validate --schema apis/<api> --spec path/to/openapi.json
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/<api> --help
cargo run -p plasm-eval -- coverage --schema apis/<api> --cases apis/<api>/eval/cases.yaml
```

For mock-backed transport testing:

```bash
hermit --specs path/to/openapi.json --port 9090 --use-examples
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/<api> --backend http://localhost:9090 <entity> query
```

Use the real server only after schema validation and mock tests are clean, especially for write capabilities.

## Eval Cases

Natural-language eval cases live in:

```text
apis/<api>/eval/cases.yaml
```

Each case should describe a user goal, not a REST endpoint. Cover:

- get/query/search/update/create/delete/action shapes
- relation traversal and scoped sub-resource queries
- pagination intent
- projection intent
- confusing near-miss wording
- write actions and side effects

Run deterministic coverage before any LLM conformance run:

```bash
cargo run -p plasm-eval -- coverage --schema apis/<api> --cases apis/<api>/eval/cases.yaml
```

