# CGS / CML Authoring Reference

This is the compact OSS reference for authoring Plasm API catalogs. The compiler and runtime are the ground truth; this file exists to keep agents and humans aligned.

## Authoring vs Determinism

Writing `domain.yaml` is a semantic design task. Specs describe RPCs; CGS describes a domain graph. Two reasonable models can differ on entity boundaries, relation shape, capability grouping, which operations belong in the prompt-facing surface, and **which `values:` keys exist** (including whether fields share one `value_ref` vs use distinct slots).

After YAML is authored, validation and compilation are deterministic:

- CGS validation checks entity, field, relation, capability, parameter, auth, versioning, and output contracts.
- CML parsing and compilation deterministically produce transport requests.
- Runtime execution deterministically decodes rows, creates refs, hydrates where configured, and formats output.
- `plasm-eval coverage` deterministically checks whether eval cases cover the required expression forms and entities.

## Files

```text
apis/<api>/
  domain.yaml
  mappings.yaml
  eval/cases.yaml
```

`domain.yaml` is the semantic model. `mappings.yaml` is the wire model. Eval cases are natural-language model conformance probes.

## Domain YAML Checklist

- Top-level `version: <n>` is present and greater than zero.
- `http_backend` is set when a catalog has a default backend.
- `auth` is explicit. Use `scheme: none` for public APIs.
- Every entity has `id_field`, fields, and optional relations.
- `id_field` exists in `fields`, unless `id_from` provides stable identity.
- Every relation target exists.
- Every `select` / `multi_select` has non-empty `allowed_values`.
- Every `array` has `items`.
- Every `date` has `value_format`.
- Every `entity_ref` has `target`.
- Every query parameter is a real API input, not copied from entity fields.
- Pagination parameters live in `mappings.yaml`, not `domain.yaml`.
- `kind: action` declares `provides:` or `output.type: side_effect`.
- CGS descriptions use domain prose, not REST paths or status codes.
- **Split catalogs:** top-level **`values:`** holds wire types and gloss; entity **`fields:`** and capability **`parameters:`** use **`value_ref:`** only (no inline `field_type` on fields). Each `values` key is a **semantic slot**, not dedupe-by-wire-type; sharing one key across sites is an authoring judgement—default to **distinct keys** unless the domain intentionally reuses one value space.

## Field and Parameter Types

Use strong types where possible:

```text
string       short / markdown / document / html / json_text via string_semantics
integer
number
boolean
select       allowed_values required
multi_select allowed_values required
date         value_format required: rfc3339, iso8601_date, unix_ms, unix_sec
array        items required
entity_ref   target required
blob         opaque bytes / base64 / attachment payloads
```

Use `entity_ref` for foreign keys, owner/repo-like scope objects where the target is modeled, and self-referential parent/child links. Do not use it for counts, limits, opaque external ids with no target entity, or pagination cursors.

## Capabilities

Capability kinds:

```text
query   collection filters or scoped lists
search  full-text relevance search
get     fetch one entity by id/key
create  create a new entity
update  modify an existing entity
delete  delete/remove an entity
action  side-effect or non-CRUD operation
```

Capability `parameters:` are typed API inputs. Common roles:

```text
filter
search
sort
sort_direction
response_control
scope
```

Use required `role: scope` for parent-scoped sub-resource queries. The generated CLI will create named subcommands for scoped queries.

## Relations

Use relations for graph navigation. For list-style sub-resources, add `materialize`:

```yaml
relations:
  comments:
    target: Comment
    cardinality: many
    materialize:
      kind: query_scoped_bindings
      capability: comment_query
      bindings:
        owner: owner
        repo: repo
        issue_number: number
```

Use `query_scoped` for one target parameter and `query_scoped_bindings` for multiple target parameters.

## Provides and Side Effects

`provides:` declares which entity fields a capability response populates. It powers prompt projection teaching, field providers, and projection hydration.

Use `provides:` when an action returns entity-shaped data:

```yaml
page_get_markdown:
  kind: action
  entity: Page
  provides: [id, markdown, truncated]
```

Use side-effect output when the response is empty, opaque, or not modeled as fields:

```yaml
workflow_run_cancel:
  kind: action
  entity: WorkflowRun
  output:
    type: side_effect
    description: Cancel the workflow run.
```

## Mapping YAML Checklist

- One CML entry exists for every capability.
- Path variables match capability/entity key variables.
- Optional query/body fields use `if exists ... else null`; CML object construction omits null keys.
- Arrays use repeated keys by default; use `join` for CSV or pipe formats.
- Response shape matches the API: `single`, `bare_list`, `items`, or the relevant item path.
- Pagination is declared only on list/query mappings.
- Wire details may be documented in comments here.

## Pagination

Pagination is a CML concern:

```yaml
pagination:
  params:
    page: { counter: 1 }
    per_page: { fixed: 30 }
```

The CLI derives flags such as `--limit`, `--all`, `--page`, `--offset`, or `--cursor` from the pagination mapping. Execute sessions expose opaque `page(pgN)` handles so models do not handle vendor cursors directly.

## Runtime Hydration

If an entity has both `query` and `get`, list results are summary rows by default and the runtime may issue concurrent GET calls to hydrate them into complete rows. Use `--summary` in `plasm-cgs` to skip hydration for list-shaped output.

This means one Plasm expression may produce multiple HTTP calls while still returning one normalized table or value.

## Validation and Testing

Use the **catalog directory** `apis/<api>/` for `schema validate` so **`domain.yaml` and `mappings.yaml` load together**. Validating only `domain.yaml` can produce false errors.

Schema/compiler validation:

```bash
cargo run -p plasm-cli --bin plasm -- schema validate apis/<api>
```

Mapping validation against a spec-backed mock:

```bash
cargo run -p plasm-cli --bin plasm -- validate --schema apis/<api> --spec path/to/openapi.json
```

Generated CLI sanity:

```bash
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/<api> --help
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/<api> <entity> --help
```

Hermit mock testing:

```bash
hermit --specs path/to/openapi.json --port 9090 --use-examples
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/<api> --backend http://localhost:9090 <entity> query
```

Eval coverage:

```bash
cargo run -p plasm-eval -- coverage --schema apis/<api> --cases apis/<api>/eval/cases.yaml
```

