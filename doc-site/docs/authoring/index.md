---
name: plasm-authoring
description: Author and validate Plasm domain models and capability mappings (HTTP REST, GraphQL via CML `transport: graphql`, and other transports supported by plasm-cml). Use when extracting schemas from OpenAPI specs, writing or editing domain.yaml / mappings.yaml files, testing capability mappings against mocks, iteratively developing typed agent tooling, or driving Cursor Agent / other agents—point them at this skill as the full playbook; do not duplicate API-specific runbooks in prompts.
---

# Plasm Authoring

Iteratively author, validate, and test a typed agent CLI from an API specification. Two distinct artifacts:

- **domain.yaml** (CGS) — the semantic model: entities, fields, relations, capability declarations (keep **prompt-facing** `description` text domain-only: no transport, pagination implementation, list/detail hydration, or request-body trivia—see [reference.md](reference.md) **`description` on entities and capabilities**)
- **mappings.yaml** (CML) — the HTTP wiring: templates mapping each capability to an HTTP call
- **Runtime query semantics** (no extra YAML file) — **pagination** comes from the CML `pagination` block on list capabilities; **hydration** (default concurrent GET per query row) applies when CGS declares **both** `query` and `get` for the same entity unless the caller passes **`--summary`** / `QueryExpr.hydrate = Some(false)`

For complete schema reference, type tables, operator compatibility, CML expression syntax, variable resolution rules, **CML pagination** (`pagination` block + `plasm-agent --limit` / `--all` and style-specific `--offset` / `--page` / `--cursor`), **default query hydration** (automatic per-row GET when the entity has both query + get; **`--summary`** to skip), and the full execution pipeline, see [reference.md](reference.md).

## Domain authoring is not deterministic

**`domain.yaml` is not produced by a correct-by-construction pipeline.** There is no supported “OpenAPI → CGS” generator crate or script in this repo, and you must not add one as a substitute for human or LLM **semantic** judgement. Reasonable authors can disagree on entity boundaries, which operations merge under one capability, relation shapes, `abstract` entities, and what belongs in the prompt-facing surface versus the wire-only edge. The same applies to **`values:`** identity: each key is a **semantic slot** for the catalog (gloss, prompts, validation intent)—**not** something you derive by collapsing every field that shares a primitive wire shape. Whether two slots **share** one `values` key or get **distinct** keys is an authoring judgement as non-deterministic as picking entity cardinality; default toward **separate keys** unless the domain meaning is intentionally one shared space (one enum, one id space, one taxonomy).

**What *is* deterministic (after the YAML exists):** `CGS::validate`, CML template parsing, compilation to HTTP, decoding against declared shapes, `plasm-eval coverage`, and similar checks. Those prove **internal consistency** of an authored model — not that the model is the *right* abstraction for an API.

**Implication:** Expanding an API (e.g. “full GitHub REST”) is **iterative authoring** over the spec and docs — repeated passes through the loop below — not flipping a codegen switch. If you need a huge RPC-shaped surface for experiments (e.g. MCP prompt size baselines), treat that as a **separate artifact or fork** with its own trade-offs; do not pretend it replaces a curated CGS.

## The Loop

```
1. READ spec  →  2. AUTHOR domain.yaml  →  3. AUTHOR mappings.yaml  →  4. VALIDATE  →  5. TEST
      ↑                                                                       │
      └───────────────────────────────────────────────────────────────────────┘
```

### Authoring with an agent (Cursor CLI or other)

Point the agent at **this skill** (`SKILL.md` + [reference.md](reference.md)) as the single source of truth. Your prompt should be minimal—for example: read the OpenAPI spec path the user gives, then follow the loop above until `domain.yaml` and `mappings.yaml` cover the API surface the user asked for, validating and testing as in Steps 4–6.

**Do not** paste parallel API-specific runbooks (phased tag lists, per-vendor checklists, or duplicate rules) into the prompt; large specs are handled by **repeated passes through the same loop** (read a slice of the spec, extend the two YAML files, validate, test, repeat). The skill already states how to read specs, what to extract, and what not to automate with scripts.

## File Structure

Output goes in a directory under the repo (canonical APIs live in `apis/`; see [`apis/README.md`](../../../apis/README.md) for the full catalog):
```
apis/<api-name>/
  domain.yaml      # entities, fields, relations, capabilities (WHAT)
  mappings.yaml    # CML templates per capability (HOW)
```

`fixtures/schemas/` in this repo only holds **test-only** single-file CGS examples (e.g. `test_schema.cgs.yaml` — includes entity **`BlobAsset`** with **`blob`**, **`mime_type_hint`**, and **`attachment_media`**); do not use it for new REST API authoring unless you are intentionally adding a tiny fixture for tests.

### NL eval cases (`plasm-eval`)

Goal-oriented harness cases live in **`apis/<api>/eval/cases.yaml`** with **`schema: <api>`** matching the directory name under `apis/`. Each case has a natural-language **`goal`**, soft **`expect:`** scoring, and optional **`covers:`** — a list of **expression-form buckets** this case is meant to exercise (e.g. `query_filtered`, `get`, `chain`, `multi_step`). Bucket IDs are snake_case and align with CGS-derived requirements.

- **Coverage (deterministic, no LLM):** `plasm-eval coverage --schema apis/<api> --cases apis/<api>/eval/cases.yaml` prints a human- and LLM-readable report: required buckets derived from **CGS** vs the union of per-case **`covers`**. Exit code is non-zero if any required bucket is missing (`--warn-only` to soften). Use **`--format json`** for scripts or agent consumption.
- **Optional `apis/<api>/eval/coverage.yaml`:** only **`required_extra:`** is implemented — it adds buckets beyond the CGS-derived set. There is **no** `exclude:` override; do not document one.
- **Scaffold:** `plasm-eval scaffold --schema apis/<api>` emits a commented YAML fragment (CGS-derived buckets + one example case). Use `--write` to create `apis/<api>/eval/cases.yaml`; `--force` overwrites.

## Step 1: Read the OpenAPI Spec

Read the spec file directly. For large specs, read section by section — paths first, then schemas, then descriptions. Use `wc -l` to gauge size, then `grep "^  /"` to list all paths before diving in.

**IMPORTANT: Do not write scripts, binaries, or “generator crates” to emit `domain.yaml` / `mappings.yaml` from OpenAPI (or any spec) as if the mapping were unique or mechanical.** There is no single deterministic reduction from RPC to CGS. You must read the spec and **author** the domain — merging endpoints, naming entities, choosing relations, classifying parameters, and deciding scope for agents. Mechanical dumps mirror the RPC surface and bypass the judgements this skill is for; they are explicitly out of scope for canonical `apis/<name>/` trees.

**Query parameters in particular are a non-deterministic inference step.** OpenAPI specs vary enormously in how they document query semantics:
- Some use standard `parameters:` with `in: query`
- Some put params in `requestBody` JSON schemas (e.g. Tavily `/search`)
- Some use vendor extension fields (`x-*`) to describe params
- Some rely entirely on `description` prose and examples
- Some leave parameters undocumented and rely on external docs

You must read **all of the above** — not just the canonical `parameters:` array — to correctly classify each param into `role: filter | search | sort | sort_direction | response_control | scope` and decide where it goes (CML `pagination` block, `parameters:`, or CML `query:` / `path:`). A spec where params live in extension fields or request bodies is a harder but equally valid authoring exercise.

**From the spec, identify:**
- **Schemas** (components/schemas): these become entities. Note field names, types, required fields, enums, and `$ref` links to other schemas.
- **Operations** (paths): these become capabilities. Note HTTP method, path, parameters (path/query/body), request body schema, response schema, extension fields, and `description` prose.
- **Relations**: `$ref` fields (Pet.category → Category), foreign key patterns (Order.petId → Pet), path nesting (/pet/{id}/tags).
- **Shared enums**: the same enum values appearing in both a schema field and a query parameter (Pet.status and findByStatus?status= are the same `PetStatus` type).

**Also identify:**
- **Authentication**: check `securitySchemes` in `components` and `security:` at the operation or global level. Common patterns:
  - `apiKey` (in: `query`) → `api_key_query`
  - `apiKey` (in: `header`) → `api_key_header`
  - `http` (scheme: `bearer`) → `bearer_token`
  - `oauth2` (clientCredentials flow) → `oauth2_client_credentials`
  - No security or purely public → omit `auth:`

**Key principle:** An OpenAPI spec describes RPC operations. You are extracting the **domain model** — the business entities, how they relate, and what operations exist on them. Multiple endpoints operate on the same entity. A field like `petId: integer` is a relation, not just a number.

## Step 2: Author domain.yaml

Write the domain model. No HTTP details here — only what exists and what you can do.

**Value registry:** under **`values:`**, each stable key defines a **named semantic slot** (wire `type` plus optional `target`, `allowed_values`, `string_semantics`, `description`, …). Entity **fields** and capability **parameters** only **`value_ref:`** that slot—slot-level keys (`required`, `path`, `role`, …) say how *this* use site differs. Treat **one key ↔ one intended meaning** in the domain; sharing a key across sites is a **deliberate** merge (same gloss / semantics), never a mechanical “all strings dedupe” shortcut. See [reference.md — Value domains](reference.md#value-domains-values-and-value_ref).

### CRITICAL: Versioning is mandatory

- Every `apis/<api>/domain.yaml` **must** declare top-level `version: <n>` with `n > 0`.
- **Never rely on defaults** for CGS versioning. Omitted/zero versions are invalid for authoring and packaging.
- When you change domain semantics (entities, fields, relations, capability signatures, parameter types/roles, auth contract, or output/provides behavior), you **must increment** `version`.
- Treat any change that can affect prompt shape, compile/decode behavior, or runtime dispatch as a version bump event.
- If you only change prose comments/description text with no semantic/runtime impact, keep version unchanged.

**`description` strings:** On entities, capabilities, and `output` for side-effect actions, write **concise domain language** (what it *is* or *does*). Avoid embedding REST paths, methods, status codes, bare **`http://` / `https://` links**, or “see GET /…” notes—those belong in **`mappings.yaml`** comments (describe APIs in words, not pasted URIs) or vendor docs, not in the CGS. **`auth.token_url`** in `domain.yaml` is the intentional exception (machine OAuth endpoint string). **Do not** repeat shapes already taught by **`value_ref`**, projection **`provides:`**, **`input_schema`** unions, or parameter names—omit field/parameter descriptions when types carry the story (see [reference.md — Gloss: do not restate typed structure](reference.md#gloss-do-not-restate-typed-structure)).

```yaml
values:
  nv_pet_id:
    type: integer
  nv_pet_name:
    type: string
    string_semantics: short
  nv_pet_status:
    type: select
    allowed_values: [available, pending, sold]
  nv_category_id:
    type: integer
  nv_category_name:
    type: string
    string_semantics: short
  nv_pet_find_by_status_status:
    type: select
    allowed_values: [available, pending, sold]

entities:
  Pet:
    id_field: id
    fields:
      id:
        value_ref: nv_pet_id
        required: true
      name:
        value_ref: nv_pet_name
        required: true
      status:
        value_ref: nv_pet_status
    relations:
      category:
        target: Category
        cardinality: one
      tags:
        target: Tag
        cardinality: many

  Category:
    id_field: id
    fields:
      id:
        value_ref: nv_category_id
        required: true
      name:
        value_ref: nv_category_name

  # Include ALL entities referenced by relations, even simple ones

capabilities:
  pet_findByStatus:
    kind: query
    entity: Pet
    parameters:
      - name: status
        value_ref: nv_pet_find_by_status_status
        required: true
  pet_get:
    kind: get
    entity: Pet
  pet_create:
    kind: create
    entity: Pet
  pet_delete:
    kind: delete
    entity: Pet
```

**DOMAIN projection (prompt teaching, not decode):** Optional per-entity **`domain_projection_examples`** (default **true**) and **`primary_read:`** select which **Get** capability’s ordered **`provides:`** drives the **`Entity  ;;  [f1,…,fN]`** heading in DOMAIN instructions. Set **`domain_projection_examples: false`** to omit that bracket line. Declare explicit ordered **`provides:`** on the primary Get so the heading matches the fields you materialize (see [reference.md — Entities](reference.md#entities)). This is separate from per-capability `provides` / field providers at runtime.

**String fields:** on the corresponding **`values:`** row with **`type: string`**, set **`string_semantics:`** for every non-trivial string (`short`, `markdown`, `document`, `html`, `json_text`, …); plain `short` is the default when omitted. Identifier-like GitHub logins, repository names, refs, and SHAs currently use `short` plus a precise `description:`.

**Field / parameter wire types:** the vocabulary is unchanged (`string`, `integer`, `number`, `boolean`, `select`, `multi_select`, `date`, `array`, `entity_ref`, **`blob`**, `uuid`) but in split **`domain.yaml`** it is expressed as **`type:`** on a **`values:`** row, not as inline `field_type` / `type` on the slot. For **`entity_ref`**, set **`target: EntityName`** on that value row. For **`blob`**, see [reference.md — Blob / binary](reference.md) (section **Blob / binary**). **`array`:** the value row has **`type: array`** and **`items: { value_ref: <element_key> }`**; the element shape is another **`values:`** row (never bare `items.type` on the slot). **`multi_select`** requires a **non-empty** `allowed_values` on its value row.

**Wire narrowing (`value_format`):** for every **`values:`** row with **`type: date`**, set **`value_format`** on that row (scalar `rfc3339`, `iso8601_date`, `unix_ms`, `unix_sec`, or map form). Same for date-typed capability parameters and `input_schema` fields. **Only the input path** uses this for coercion; response display is not rewritten. DOMAIN uses a generic datetime hint in prompts. Omit `value_format` on non-date types.

### CGS field typing checklist (strict)

Use this on every new or edited entity (and on capability `parameters:` / `input` fields) so the model does not collapse to “stringly typing.”

1. **Instants and calendar dates** — If the wire value is a timestamp or date (RFC 3339 / ISO-8601 string, or Unix `sec` / `ms` in a field that is *only* that), use a **`values:`** row with **`type: date`** and the correct **`value_format`**. **Do not** use **`type: string`** for fields named like `date_created`, `date_updated`, `last_modified`, `*expires*`, `*_on`, or `last_*` when the API returns a normal machine date/time, unless the vendor truly returns an unstructured or multi-shape mess that cannot be one `value_format` (then document why in a short **`description:`** on the **`values:`** row or a slot override).

2. **Enumerations** — If the set of values is closed and known, use **`select`** / **`multi_select`** with **`allowed_values`** on the value row, not `string`. Prefer matching the **vendor’s documented** alternatives (e.g. API doc tables or request schema bullet lists). If the same field name is reused with **inconsistent** or **extensible** lifecycles across resources (e.g. one API returns `active` and another `Running`, or the vendor may add a new state without a doc bump), keep **`string`** and do not force a narrow `select` that will reject valid future wire values.

3. **Foreign keys** — If the value is another resource’s id and that resource is in the CGS, use **`type: entity_ref`** and **`target:`** on the value row (see EntityRef section). Do not model `*_id` as `string` when the target entity exists and the id matches.

4. **Reverse list edges (many) —** When a child row has `entity_ref` to a parent and the child’s **primary list** `query` / `search` exposes a **parameter** that can filter (or scope) by that parent’s id, declare a **`cardinality: many` relation on the parent** with **`materialize: { kind: query_scoped, capability: <child’s query cap>, param: <that parameter name> }`**. That makes chain navigation `Parent(id).<relation>` resolve to a scoped list. **Do not** add a `relations` key with the **same** name as an `entity_ref` field on the **same** entity (the schema rejects a relation name colliding with an `EntityRef` field). Forward navigation still uses the **`entity_ref` field** on the child; `relations` on the parent names the **reverse** list edge. If no child query parameter exists, you cannot add `query_scoped` — do not invent a relation, or add the filter to the real API and CGS first.

5. **Opaque bytes and file bodies** — Use **`type: blob`** on the value row, not `string` with ad hoc conventions.

6. **Human text and opaque tokens** — Use **`type: string`** with explicit **`string_semantics:`** on the value row for names, descriptions, free-form tags, and vendor-specific tokens that are not dates, enums, refs, or blobs.

Apply the same rules to **`parameters:`** `value_ref` targets (e.g. filter dates as a **`values:`** row with **`type: date`** and `value_format`).

**Capability kinds:** `query` (collection filter), `get` (by ID), `create` (no ID needed), `update`, `delete`, `action` (anything else)

**`kind: action` output:** Every action must declare either non-empty **`provides:`** (entity field projection from the response) or **`output:`** with **`type: side_effect`** and a **non-empty `description:`** that states **what** the operation changes in the domain. There is no `output.type: none` — it was removed as an incomplete-modeling escape hatch. Full rules and examples: [reference.md — Action output](reference.md#action-output-provides-vs-outputside_effect).

### Authentication — top-level `auth:` block

Add a single `auth:` block at the end of `domain.yaml` (after all entities and capabilities). The runtime reads secrets at execution time from the named environment variables — no secrets go in schema files.

```yaml
# API key in query string (e.g. RAWG ?key=..., OMDb ?apikey=..., NYT ?api-key=...)
auth:
  scheme: api_key_query
  param: key          # the query param name the API expects
  env: RAWG_API_KEY   # env var name (not the value)

# API key as a request header (e.g. X-Api-Key)
auth:
  scheme: api_key_header
  header: X-Api-Key
  env: MY_SERVICE_API_KEY

# Bearer token (e.g. ClickUp, Notion, Tavily)
auth:
  scheme: bearer_token
  env: CLICKUP_API_TOKEN

# OAuth 2.0 client credentials (e.g. Spotify)
auth:
  scheme: oauth2_client_credentials
  token_url: https://accounts.spotify.com/api/token
  client_id_env: SPOTIFY_CLIENT_ID
  client_secret_env: SPOTIFY_CLIENT_SECRET
  scopes: []    # omit or leave empty if no scopes needed
```

Omit `auth:` entirely for public APIs that require no credentials.

### Query capability parameters — what belongs in `parameters:`

**Critical rule: only declare parameters the API endpoint actually accepts as HTTP inputs.** Read the OpenAPI operation's `parameters` list and `description` fields. Never generate `parameters:` from entity fields — entity fields describe the domain object, not what the query endpoint accepts.

**No parameters at all?** If the endpoint is a plain paginated resource index (e.g. PokéAPI `/pokemon/` — no server-side filters, just offset/limit), declare **no `parameters:`**. The CLI will show only `--limit` / `--all` / `--offset` (from the CML `pagination` block). This is correct and intentional.

#### `kind: query` vs `kind: search`

Use **`kind: search`** when the endpoint's primary interface is a **free-text relevance query** (`q`, `query`, `search`) that returns ranked results rather than field-filtered rows. The CLI verb becomes `entity search` instead of `entity query`. If the endpoint filters by concrete field values (`status`, `archived`, `team_id`), use `kind: query`.

#### Classification: where each OpenAPI query param goes

**Source of params may vary:** Params may be in `parameters[in: query]`, in a `requestBody` JSON schema (POST-style search APIs like Tavily), in `x-*` vendor extensions, or described only in `description` prose. Infer the role from semantics and naming conventions regardless of where in the spec the param is documented.

| Param examples | Role | `role:` annotation | Where it goes |
|---------------|------|---------------------|---------------|
| `offset`, `limit`, `page`, `cursor`, `after`, `before` | **Pagination** | — | CML `pagination` block only — **not** in `parameters:` |
| `status`, `tags[]`, `assignees[]`, `archived`, `type` | **Filter** | `filter` *(default, omit)* | `parameters:` + CML `query:` block var |
| `q`, `search`, `query` | **Full-text search** | `search` | `parameters:` (`value_ref` → `values:` string row) + CML `query:` block var; use `kind: search` on the capability |
| `order_by`, `sort_by` | **Sort field** | `sort` | `parameters:` + CML `query:` block var |
| `sort`, `direction`, `asc`/`desc` | **Sort direction** | `sort_direction` | `parameters:` + CML `query:` block var |
| `market`, `locale`, `country`, `embed`, `fields`, `inc` | **Response control** | `response_control` | `parameters:` + CML `query:` block var |
| `team_id` in `GET /team/{team_id}/space` | **Parent-scoped sub-resource** | `scope` | `parameters:` (`value_ref` → `values:` `entity_ref` row, required) + CML `path:` var segment |

**Scoped sub-resource queries:** When an API has endpoints like `GET /classes/{class_index}/spells` alongside `GET /spells`, declare both as separate `kind: query` capabilities on the same entity. The one with a required `role: scope` parameter automatically gets a **named subcommand** (`spell class-spells --class_index wizard`) while the unscoped one gets the generic `spell query` verb. No extra annotation needed — the CLI infers primary vs named from the parameter roles.

**Range filters** like ClickUp's `due_date_gt` / `due_date_lt` or Spotify's `min_energy` / `max_energy` are **separate named parameters** — declare each one individually in `parameters:`, not as one field with operator suffixes.

**Array / multi-value params** (e.g. `genres`, `assignees[]`, `embed[]`): use a **`values:`** row with **`type: multi_select`** (non-empty `allowed_values`) or **`type: array`** with **`items: { value_ref: <element_key> }`** (element shape is its own **`values:`** row). Entity **`fields`** that are lists use the same array pattern on **`values:`**. In mappings.yaml:
- **Repeated key** (`?embed=a&embed=b`) — use plain `{ type: var, name: embed }` in the CML `query:` block; the HTTP layer expands arrays automatically.
- **CSV** (`?genres=1,2,3`) — use `{ type: join, sep: ",", expr: { type: var, name: genres } }`.
- **Pipe** (`?ids=1|2|3`) — use `{ type: join, sep: "|", expr: { type: var, name: ids } }`.

#### Common query patterns (examples)

**Pattern 1 — Index-only (PokéAPI `/pokemon/`)**: No server-side filters. Only pagination.

```yaml
# domain.yaml
capabilities:
  pokemon_query:
    kind: query
    entity: Pokemon
    # No parameters: — paginated index, no filters

# mappings.yaml
pokemon_query:
  method: GET
  path: [{ type: literal, value: api }, { type: literal, value: v2 },
         { type: literal, value: pokemon }, { type: literal, value: "" }]
  pagination:
    style: offset_limit
    request: { offset: offset, limit: limit, default_limit: 20 }
    response: { items: results, total: count, next: next }
```

**Pattern 2 — Filter endpoint (GitHub `GET /repos/{owner}/{repo}/issues`)**: Required scope plus optional enum filter.

```yaml
# domain.yaml
values:
  nv_pet_find_by_status_status:
    type: select
    allowed_values: [available, pending, sold]

capabilities:
  pet_findByStatus:
    kind: query
    entity: Pet
    parameters:
      - name: status
        value_ref: nv_pet_find_by_status_status
        required: true

# mappings.yaml
pet_findByStatus:
  method: GET
  path: [{ type: literal, value: pet }, { type: literal, value: findByStatus }]
  query:
    type: object
    fields:
      - [status, { type: var, name: status }]
```

**Pattern 3 — Rich filter (ClickUp `GET /team/{team_id}/task`)**: Parent-scoped required ref + multiple optional filters.

```yaml
# domain.yaml
values:
  nv_task_query_team_id:
    type: entity_ref
    target: Team
  nv_task_query_statuses:
    type: multi_select
    allowed_values: [open, in progress, review, closed]
  nv_task_query_include_closed:
    type: boolean
  nv_task_query_order_by:
    type: select
    allowed_values: [created, updated, due_date, id]
  nv_task_query_due_date_gt:
    type: integer
  nv_task_query_due_date_lt:
    type: integer

capabilities:
  task_query:
    kind: query
    entity: Task
    parameters:
      - name: team_id
        value_ref: nv_task_query_team_id
        required: true
      - name: statuses
        value_ref: nv_task_query_statuses
        required: false
      - name: include_closed
        value_ref: nv_task_query_include_closed
        required: false
      - name: order_by
        value_ref: nv_task_query_order_by
        required: false
      - name: due_date_gt
        value_ref: nv_task_query_due_date_gt
        required: false
      - name: due_date_lt
        value_ref: nv_task_query_due_date_lt
        required: false

# mappings.yaml
task_query:
  method: GET
  path:
    - { type: literal, value: v2 }
    - { type: literal, value: team }
    - { type: var, name: team_id }
    - { type: literal, value: task }
  query:
    type: object
    fields:
      - [order_by,      { type: if, condition: { type: exists, var: order_by },
                          then_expr: { type: var, name: order_by }, else_expr: { type: const, value: null } }]
      - [include_closed,{ type: if, condition: { type: exists, var: include_closed },
                          then_expr: { type: var, name: include_closed }, else_expr: { type: const, value: null } }]
      - [due_date_gt,   { type: if, condition: { type: exists, var: due_date_gt },
                          then_expr: { type: var, name: due_date_gt }, else_expr: { type: const, value: null } }]
      - [due_date_lt,   { type: if, condition: { type: exists, var: due_date_lt },
                          then_expr: { type: var, name: due_date_lt }, else_expr: { type: const, value: null } }]
```

Note: `statuses` is a `multi_select` — the CLI generates a repeatable flag (`--statuses open --statuses review`). The CML `query:` block emits it as `?statuses=open&statuses=review` (repeated key, the default). To emit CSV (`?statuses=open,review`) use `type: join` with `sep: ","`.

**Pattern 4 — Full-text search (TVMaze `GET /search/shows?q=...`)**: Free-text relevance query.

```yaml
# domain.yaml
values:
  nv_show_search_q:
    type: string
    string_semantics: short

capabilities:
  show_search:
    kind: search          # <-- not query
    entity: Show
    parameters:
      - name: q
        value_ref: nv_show_search_q
        required: true
        role: search

# mappings.yaml
show_search:
  method: GET
  path: [{ type: literal, value: search }, { type: literal, value: shows }]
  query:
    type: object
    fields:
      - [q, { type: var, name: q }]
```

CLI: `plasm-agent show search --q "breaking bad"` (verb is `search`, not `query`).

**Pattern 5 — Sort + response control (Jikan `GET /v4/anime`)**: Sort field, direction, and embed params annotated with roles.

```yaml
# domain.yaml
values:
  nv_anime_query_q:
    type: string
    string_semantics: short
  nv_anime_query_type:
    type: select
    allowed_values: [tv, movie, ova, special, ona]
  nv_anime_query_min_score:
    type: number
  nv_anime_query_max_score:
    type: number
  nv_anime_query_order_by:
    type: select
    allowed_values: [score, rank, popularity, members, episodes, start_date]
  nv_anime_query_sort:
    type: select
    allowed_values: [asc, desc]
  nv_anime_query_genres_item:
    type: integer
  nv_anime_query_genres:
    type: array
    items:
      value_ref: nv_anime_query_genres_item

capabilities:
  anime_query:
    kind: query
    entity: Anime
    parameters:
      - name: q
        value_ref: nv_anime_query_q
        role: search
      - name: type
        value_ref: nv_anime_query_type
      - name: min_score
        value_ref: nv_anime_query_min_score
      - name: max_score
        value_ref: nv_anime_query_max_score
      - name: order_by
        value_ref: nv_anime_query_order_by
        role: sort
      - name: sort
        value_ref: nv_anime_query_sort
        role: sort_direction
      - name: genres
        value_ref: nv_anime_query_genres

# mappings.yaml
anime_query:
  method: GET
  path: [{ type: literal, value: v4 }, { type: literal, value: anime }]
  pagination:
    style: page_size
    request: { page: page, size: limit, default_size: 25 }
    response: { items: data, next: pagination.has_next_page }
  query:
    type: object
    fields:
      - [q,        { type: if, condition: { type: exists, var: q },
                     then_expr: { type: var, name: q }, else_expr: { type: const, value: null } }]
      - [type,     { type: if, condition: { type: exists, var: type },
                     then_expr: { type: var, name: type }, else_expr: { type: const, value: null } }]
      - [min_score,{ type: if, condition: { type: exists, var: min_score },
                     then_expr: { type: var, name: min_score }, else_expr: { type: const, value: null } }]
      - [max_score,{ type: if, condition: { type: exists, var: max_score },
                     then_expr: { type: var, name: max_score }, else_expr: { type: const, value: null } }]
      - [order_by, { type: if, condition: { type: exists, var: order_by },
                     then_expr: { type: var, name: order_by }, else_expr: { type: const, value: null } }]
      - [sort,     { type: if, condition: { type: exists, var: sort },
                     then_expr: { type: var, name: sort }, else_expr: { type: const, value: null } }]
      # genres sent as CSV: ?genres=1,2,3
      - [genres,   { type: if, condition: { type: exists, var: genres },
                     then_expr: { type: join, sep: ",", expr: { type: var, name: genres } },
                     else_expr: { type: const, value: null } }]
```

**Rows without a top-level id:** Some list endpoints return objects with no `id` key (only nested URLs or names). Set optional **`id_from`** to a path of object keys (YAML list or dotted string, e.g. `location_area.url`) so decoding can still build stable `_ref`s. Then `id_field` can name that logical id without duplicating a column in `fields` unless you want it exposed. See [reference.md](reference.md) entities section.

**Checklist before proceeding:**
- [ ] Every relation target is a defined entity
- [ ] Every **`values:`** row with **`type: select`** (or **`multi_select`**) has non-empty **`allowed_values`**
- [ ] Every required query parameter has `required: true`
- [ ] Capability names are unique and follow `entity_operation` convention
- [ ] Entity with no operations is fine (e.g. Category only referenced by Pet)
- [ ] Every entity has either a declared `fields` entry for `id_field` or a non-empty `id_from` path to a string/number
- [ ] If an entity should **not** teach projection brackets in DOMAIN, set **`domain_projection_examples: false`**

### Scoped relation traversal (`materialize`)

When an API uses sub-resource URLs to navigate from a parent to its children (`/parent/{id}/children`), set **`materialize`** on the **many** relation so chain traversal knows how to fill the target query’s scope parameter(s).

**Single scope parameter** — `query_scoped`: required **`capability`** (target `query` / `search` name) plus **`param`** (scope field on that capability); value from the parent row’s `id_field`.

```yaml
# OpenAPI: GET /v1/blocks/{block_id}/children → Block[]
entities:
  Page:
    relations:
      blocks:
        target: Block
        cardinality: many
        materialize:
          kind: query_scoped
          capability: block_children_query
          param: block_id
```

**Multiple scope parameters** — `query_scoped_bindings`: required **`capability`** plus **`bindings`** (each target capability param name → **parent entity field** name), e.g. calendar id → `Event` query:

```yaml
materialize:
  kind: query_scoped_bindings
  capability: event_list
  bindings:
    calendarId: id
```

The runtime uses the named capability for chain traversal. The CLI subcommand (`page <id> blocks`) fills scope params from the parent row — users do not pass them as extra flags.

**Compound `entity_ref` scope params** (e.g. a single parameter carrying `owner/repo`) are splatted at runtime into multiple CML slots; see **`scope_aggregate_key_policy`** on capabilities in [reference.md](reference.md). This is **not** the same as `query_scoped_bindings` (multiple named params).

---

### Multiple projections of the same entity — `provides:` and auto-resolution

Some APIs expose the same logical resource through multiple endpoints that return **disjoint field subsets** of the same entity. This is not the same as list/detail hydration — neither endpoint's response is a superset of the other.

**Recognition signal**: two endpoints share the same path prefix and return the same `id` value, but their response fields are entirely different:
```
GET /pages/{id}          → { id, url, timestamps, in_trash }    # structural metadata
GET /pages/{id}/markdown → { id, markdown, truncated }          # content projection
```

**Authoring rule**: declare **one entity** with all fields. Mark projection-only fields as `required: false`. Declare `provides:` on each capability listing exactly which fields it populates. The runtime uses this as a **field-provider reverse index** — when an agent requests a field that is absent from cache, it automatically invokes the capability that provides it.

```yaml
values:
  nv_page_url:
    type: string
    string_semantics: short
  nv_page_created_time:
    type: date
    value_format: rfc3339
  nv_page_in_trash:
    type: boolean
  nv_page_markdown:
    type: string
    string_semantics: markdown
  nv_page_truncated:
    type: boolean

entities:
  Page:
    id_field: id
    fields:
      url:
        value_ref: nv_page_url
      created_time:
        value_ref: nv_page_created_time
      in_trash:
        value_ref: nv_page_in_trash
      markdown:
        value_ref: nv_page_markdown
        required: false
      truncated:
        value_ref: nv_page_truncated
        required: false

capabilities:
  page_get:
    kind: get
    entity: Page
    provides: [id, url, public_url, created_time, last_edited_time, in_trash, archived]

  page_get_markdown:
    kind: action
    entity: Page
    provides: [id, markdown, truncated]
```

**What `provides:` enables**: the runtime builds a reverse index `field → capability`. When an expression like `Page("abc")[markdown]` is evaluated and `markdown` is not in cache, the runtime automatically invokes `page_get_markdown("abc")` — transparent to the agent. One expression, one hop, correct result.

```
plasm> Page("abc")[markdown]
→ Get(Page:abc)
  projection: [markdown]
# auto-invokes page_get_markdown because markdown absent from cache
{"markdown": "# Page content..."}
(1 result, Live, 455ms, 1 http call)
```

**`provides:` defaults** (when omitted — backward-compatible):
- `get` / `query` / `search` → assumed to provide all entity fields
- `create` / `update` / `delete` / `action` → assumed to provide nothing (declare explicitly)

For **`action`**, empty default `provides` means you **must** add **`output: { type: side_effect, description: "…" }`** unless you list `provides`. See [reference.md — Action output](reference.md#action-output-provides-vs-outputside_effect).

Declare `provides:` only when the capability provides a **strict subset** of the entity fields (disjoint projection). If it provides everything, omit it — the default is correct.

**Where to look for disjoint projections**:
- Path template: `/resource/{id}/suffix` alongside `/resource/{id}` (same ID, different suffix)
- Same `id` in both responses but different `object` type values or disjoint field sets
- OpenAPI: two operations with entirely disjoint `properties` on related paths
- Docs: phrases like "retrieve X as Y" or "get the Y representation of X"

**Three-way capability contract** (full design — `mutates:` coming in a future release):
- `parameters:` (accepts) — what the API takes as input
- `provides:` (output) — which entity fields this response populates
- `mutates:` (write set) — which entity fields this capability can change (for update/action)

---

### EntityRef fields and composition

When a field stores another entity's ID (foreign key), use a **`values:`** row with **`type: entity_ref`** and **`target:`**:

```yaml
values:
  nv_order_id:
    type: integer
  nv_order_pet_id:
    type: entity_ref
    target: Pet

entities:
  Order:
    id_field: id
    fields:
      id:
        value_ref: nv_order_id
        required: true
      petId:
        value_ref: nv_order_pet_id
```

**Do not** use `string` or `integer` for FK fields — `entity_ref` enables three composition features automatically:

1. **FK navigation CLI**: `order 5 pet-id` auto-resolves the referenced Pet entity (subcommand generated for every EntityRef field whose target has a `get` capability)
2. **Reverse traversal CLI**: `pet 10 orders` queries all Orders where petId = 10 (auto-derived when the Order query capability has a `petId` parameter whose **`value_ref`** resolves to **`entity_ref`** with the same **`target`**)
3. **Cross-entity predicates**: `query(Order) WHERE pet.status = available` decomposes through the EntityRef boundary — the executor queries Pet first, then injects matching IDs into the Order query

**When to use `entity_ref`:**
- Any field ending in `_id`, `Id`, `_key`, or whose name matches another entity's `id_field` value
- Path parameters that scope a sub-resource (e.g. `team_id` on Space in ClickUp)
- Explicit `$ref` links in the OpenAPI spec
- Parent/scope IDs (e.g. `workspace_id`, `database_id` in Notion)
- Author/creator/assignee fields that store a User's account ID (e.g. `author_id`, `assignee_id`, `reporter_id`)
- **Self-referential parent fields** — `parent_key`, `parent_id`, `parent` storing the same entity's ID (e.g. `Task.parent → Task` for subtasks, `Issue.parent_key → Issue` for epics)

**When NOT to use `entity_ref`:**
- Quantities, counts, limits, page sizes — these are `integer`
- IDs that reference entities **outside** the current CGS scope (no matching entity defined)
- IDs for which the target entity has **no `get` capability** — auto-resolve requires a `get`; you may still use `entity_ref` for documentation/type purposes, but the navigation subcommand will not be generated

#### Self-referential entity_ref (tree hierarchies)

`entity_ref` with `target` pointing to the **same entity** is fully supported. The validator only rejects refs to unknown entities; same-entity refs pass and generate navigation subcommands:

```yaml
values:
  nv_task_parent:
    type: entity_ref
    target: Task

entities:
  Task:
    id_field: id
    fields:
      parent:
        value_ref: nv_task_parent
        required: false
```

This generates `task <id> parent` → `task_get(<parent_id>)` automatically. Chains work: `task <id> parent parent` walks two levels up.

**APIs where this applies:**
- ClickUp: `Task.parent → Task` (subtask hierarchy)
- Jira: `Issue.parent_key → Issue` (epic → story → subtask)
- Linear: `Issue.parent → Issue` / `Issue.children → Issue` (sub-issues)
- Notion: `Page.parent_id → Page` (nested pages)
- GitHub: `Repo.parent_id → Repo` (fork → upstream)

#### The `path:` annotation does not affect entity_ref resolution

Fields with a `path:` annotation (for extracting from nested JSON) can still be `entity_ref`. The `path:` is used at decode time to extract the raw ID value; the `entity_ref` type is used at traversal time to resolve it:

```yaml
values:
  nv_issue_assignee_id:
    type: entity_ref
    target: User

entities:
  Issue:
    fields:
      assignee_id:
        value_ref: nv_issue_assignee_id
        path: fields.assignee.accountId
```

The extracted `accountId` value becomes the `id` injected into `user_get`.

**Capability parameters:** Query parameters that accept FK values should also use `entity_ref`:

```yaml
values:
  nv_order_query_pet_id:
    type: entity_ref
    target: Pet

capabilities:
  order_query:
    kind: query
    entity: Order
    parameters:
      - name: petId
        value_ref: nv_order_query_pet_id
        required: false
```

This enables reverse traversal: the system sees that `order_query` accepts `petId: EntityRef(Pet)` and auto-generates `pet <id> orders` as a subcommand. The **`target`** on the parameter’s **`value_ref`** row must match the field’s **`entity_ref`** target for the CGS validator to pass.

#### Authoring checklist — entity_ref audit

When finishing a domain.yaml, scan every **`values:`** row (and slot) that resolves to **`string`** and ask whether it should instead be **`entity_ref`**:
- Does its name end in `_id`, `Id`, `_key`, or match another entity's `id_field`?
- Is there a matching entity in this schema whose `id_field` value would be stored here?
- Does that entity have a `get` capability?

If yes to all three → change to `entity_ref`. For self-referential cases (field on entity X pointing to another X), the same check applies with `target: X`.

## Step 3: Author mappings.yaml

Write the transport wiring for each capability (default **REST**: `method` + `path`; **GraphQL**: top-level **`transport: graphql`** with `endpoint` / `operation` / variables — see `plasm-cml` and examples under `apis/linear`, `apis/graphqlzero`). One entry per capability name from `domain.yaml`.

```yaml
pet_findByStatus:
  method: GET
  path:
    - type: literal
      value: pet
    - type: literal
      value: findByStatus
  query:
    type: object
    fields:
      - - status
        - type: var
          name: status

pet_get:
  method: GET
  path:
    - type: literal
      value: pet
    - type: var
      name: id

pet_create:
  method: POST
  path:
    - type: literal
      value: pet
  body:
    type: var
    name: input

pet_delete:
  method: DELETE
  path:
    - type: literal
      value: pet
    - type: var
      name: id
```

**Path segments:**
- `{type: literal, value: "pet"}` — static segment
- `{type: var, name: "id"}` — variable, resolved at runtime

**Variable names the engine provides:**
- `id` — the entity ID (for get/delete/update/action)
- Predicate field names — e.g. `status` from `--status available` (for query)
- `input` — the full input object (for create/update with body)
- Path variables: see [reference.md](reference.md) — primary `id`, optional `path_vars`, multi-segment CLI `--{kebab}` flags, and create `input` keys merged into the CML env

**Query params** — for GET endpoints that take query parameters:
```yaml
query:
  type: object
  fields:
    - - paramName
      - type: var
        name: paramName
```

**Request body** — for POST/PUT/PATCH:
```yaml
body:
  type: var
  name: input
```

### Pagination & hydration (CML + runtime)

**Pagination** — declare only in **`mappings.yaml`** (`pagination` block on **query** capabilities). Infer `style`, wire param names, and JSON paths per [reference.md — Pagination (CML)](reference.md#pagination-cml--mappingsyaml-only). **`plasm-agent`** adds `--limit` / `--all` and style-specific `--offset` / `--page` / `--cursor` when the mapping has pagination.

**Hydration** — after a **query**, if the entity has both **`query`** and **`get`**, the runtime **by default** fetches full rows via **get** unless **`--summary`** or `QueryExpr.hydrate = Some(false)`. No extra CGS flag — add or omit **get** by design. See [reference.md](reference.md) (**Query result hydration**).

## Step 4: Validate

For split **`domain.yaml` + `mappings.yaml`**, pass the **catalog directory** `apis/<api>/` to `schema validate`. Pointing at **`domain.yaml` alone** skips `mappings.yaml` and can falsely fail (e.g. capabilities present in domain reported as missing from mappings).

Run these commands and check the output:

```bash
# Schema load + CGS validation (prefer catalog directory for split domain+mappings)
cargo run -p plasm-cli --bin plasm -- schema validate apis/<api>

# Optional: exhaustive mapping exercise against an OpenAPI mock (hermit)
cargo run -p plasm-cli --bin plasm -- validate --schema apis/<api> --spec path/to/openapi.json

# Does the CLI generate?
plasm-agent --schema apis/<api> --help

# Does the entity have the right subcommands?
plasm-agent --schema apis/<api> <entity> --help

# Are query flags typed correctly?
plasm-agent --schema apis/<api> <entity> query --help
# Look for: [possible values: ...] on select fields, --status <status> in Usage for required params

# Does clap reject invalid input?
plasm-agent --schema apis/<api> <entity> query
# Should error: required arguments not provided

plasm-agent --schema apis/<api> <entity> query --status BOGUS
# Should error: invalid value [possible values: ...]
```

## Step 5: Test Against Mock

```bash
# Start hermit (zero-config mock from the same spec)
hermit --specs <path-to-openapi-spec> --port 9090 --use-examples

# Determine base path from the spec's servers section
# e.g. some specs use /api/v3
BASE=http://localhost:9090/api/v3

# Query
plasm-agent --schema apis/<api> --backend $BASE <entity> query --<flag> <value>

# Get by ID
plasm-agent --schema apis/<api> --backend $BASE <entity> <id>

# Navigate relation
plasm-agent --schema apis/<api> --backend $BASE <entity> <id> <relation>

# Table output
plasm-agent --schema apis/<api> --backend $BASE --output table <entity> query --<flag> <value>
```

**If it fails, check:**
- `CmlError::VariableNotFound` → mapping var name doesn't match engine env. Fix the `name` in mappings.yaml.
- `DecodeError` → response shape mismatch. The engine normalizes bare arrays automatically.
- `404` → path doesn't match spec. Check base URL includes server prefix.

Then fix domain.yaml or mappings.yaml and re-run from Step 4.

## Step 6: Test Against Real Backend

```bash
# Live — real HTTP calls
plasm-agent --schema ... --backend https://api.example.com --mode live ...

# Hybrid — replay cache hits, live on miss (builds replay corpus)
plasm-agent --schema ... --backend https://api.example.com --mode hybrid ...

# Replay — cached only, no network (deterministic regression)
plasm-agent --schema ... --mode replay ...
```
