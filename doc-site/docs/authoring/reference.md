# CGS & CML Reference

## Authoring vs determinism

**Writing `domain.yaml` is not a deterministic process.** OpenAPI (and friends) do not uniquely define a CGS: entity boundaries, capability grouping, relations, parameter roles, and `abstract` embed-only types are **semantic choices**. Tools may assist (e.g. an LLM reading the spec), but there is no canonical auto-generator in-repo and no guarantee that two valid domain models for the same API are equivalent.

**After** the YAML is written, **validation and compilation** are deterministic: schema checks, CML parse/compile, and runtime request shaping are mechanical consequences of what you authored.

## Obsolete or unsupported (do not teach)

| Topic | Status |
|-------|--------|
| **`domain_projection_fields`** | Removed — use **`domain_projection_examples`**, optional **`primary_read`**, and explicit ordered **`provides:`** on the primary Get (see [Entities](#entities)). |
| **`output.type: none`** | Removed — actions need **`provides:`** and/or **`output: { type: side_effect, description: … }`**. |
| **CGS as `.json`** | **Not loaded** — `load_schema` rejects JSON paths; use a directory with **`domain.yaml` + `mappings.yaml`**, a combined authoring **`.yaml`**, or **`.cgs.yaml`** interchange. |
| **`apis/<api>/eval/coverage.yaml` `exclude:`** | **Not implemented** — only **`required_extra`** exists in `plasm-eval` coverage overrides. |
| **`string` + `string_semantics: blob`** | **Legacy** — the loader normalizes this to **`field_type: blob`** and clears blob string semantics. Prefer authoring **`field_type: blob`** directly. |

## How CGS, CML, and runtime fit together

| Layer | Artifact / crate | Role in list queries |
|-------|------------------|----------------------|
| **CGS** | `domain.yaml` | Declares entities and capability **kinds** (`query`, `get`, …). Whether an entity has **both** query and get determines whether **hydration** is eligible — not how HTTP works. |
| **CML** | `mappings.yaml` | Compiles each capability to HTTP/GraphQL. Optional composable **`pagination:`** (`PaginationConfig`: `params`, `location`, …) on **query** mappings drives multi-request pagination; list decode shape lives in **`response:`** / decoder config. |
| **Runtime** | `plasm-runtime`, `plasm-agent` | Evaluates CML, loops pages when CLI `--limit`/`--all` (or internal caps) ask for more, decodes rows, merges into `GraphCache`. **LLM execute** uses opaque **`page(pg#)`** continuations (session-scoped) instead of exposing raw API pagination fields. Then (by default) runs **concurrent GET** per row to upgrade **summary → complete** when CGS has a **get** for that entity. |

**Pagination wiring** is a **CML** concern; **opaque LLM paging handles** are minted by **`plasm-agent`** execute sessions. **Hydration** is a **runtime** policy gated by **CGS** capability pairs (`query` + `get`).

## CGS (Capability Graph Schema) — domain.yaml

The CGS is the semantic domain model. It declares what entities exist, how they relate, and what operations are available. It contains no HTTP details.

### CRITICAL: Versioning is mandatory

- Every `apis/<api>/domain.yaml` must declare top-level `version: <n>` where `n > 0`.
- Version defaulting is forbidden; omitted/zero versions are invalid for authoring and plugin packaging.
- Increment `version` whenever domain semantics change (entities, fields, relations, capability signatures, parameter typing/roles, auth contract, output/provides behavior).
- Keep version unchanged only for non-semantic text edits (comments/prose) that do not affect runtime behavior, prompts, compile/decode, or dispatch.

### Entities

An entity is a typed domain object with a primary key, fields, and relations.

```yaml
entities:
  <EntityName>:               # PascalCase
    id_field: <field_name>    # logical primary key for refs / CLI; must exist in fields unless id_from is set
    id_from: <path>           # optional — when list/detail JSON rows have no top-level id, take identity from nested keys
    fields:
      <field_name>:
        field_type: <type>    # see Field Types below
        required: <bool>      # default false
        value_format: <scalar or { temporal: ... }>  # required when field_type is date (see ValueWireFormat)
        target: <EntityName>  # required when field_type is entity_ref
        allowed_values:       # required for select/multi_select (multi_select must be non-empty)
          - value1
          - value2
        items:                 # required when field_type is array — element typing (see Array element typing below)
          type: string
    relations:
      <relation_name>:
        target: <EntityName>  # must be a defined entity
        cardinality: one|many
    domain_projection_examples: false   # optional — default true; false = omit `[field,…]` projection list on the DOMAIN entity heading
    primary_read: <get_capability_id>    # optional — which Get’s ordered `provides` drives projection teaching; default = primary Get (see plasm-core)
```

**DOMAIN projection teaching (default on):** For each entity with a primary **Get** and non-empty ordered **`F`** from `CGS::domain_projection_heading_fields` in `plasm-oss/crates/plasm-core/src/schema.rs` (same as `provides` / default field order), the prompt renderer puts **`F`** in a single bracket on the **entity heading** line after `;;`, before the description: `Entity  ;;  [f1,f2,…,fN] -  …`. This applies even when DOMAIN teaches fetch as a zero-arity method (`Entity.m#()`) instead of `Entity($)`. Expressions still use `Entity(…)[subset]` for actual reads. The **Valid expressions** preamble states that **any non-empty subset** of those fields is valid for trimming payloads; DOMAIN does not enumerate every prefix. **`F`** comes from that Get’s explicit **`provides:`** list (order preserved); if `provides` is empty, **`F`** defaults to **`id_field` first**, then remaining fields **lexicographically**. Set **`domain_projection_examples: false`** to suppress heading brackets (replaces the old empty `domain_projection_fields: []`). Optional **`primary_read:`** names the **Get capability id** when you must override which Get defines **`F`** (otherwise the same **primary Get** as the CLI manifest). This is **prompt teaching only**; runtime decode still uses per-capability **`provides`** / `effective_provides`.

**`from_parent_get` pitfall:** The JSON path must match the **parent GET response** for that relation. Array-of-ref shapes differ by API (e.g. PokéAPI Pokémon `moves[].move` vs Type `moves[]` as bare `{name,url}`). Copying one entity’s `materialize.path` to another without checking the wire JSON yields empty relations at decode time.

**Cardinality `one` + nested child:** When the child ref is **not** top-level `{relation_name}.name` (e.g. under `meta.ailment` on a move), declare **`materialize: { kind: from_parent_get, path: [...] }`** on that **one** relation. Only **`from_parent_get`** is allowed on cardinality `one`; query-scoped materialization remains for **many** relations.

**`id_from` (optional):** sequence of JSON object keys from the row object to a scalar `string` or `number` used as the stable id (e.g. a canonical URL). YAML may be a list `[location_area, url]` or a dotted string `location_area.url`. When `id_from` is present and non-empty, you **do not** need a `fields` entry named `id_field` solely for decoding; the runtime injects `id_field` into decoded rows from this path when missing.

**Constraints:**
- `id_field` must name a field in `fields`, **or** `id_from` must be a non-empty path as above
- Every relation `target` must be a defined entity (no dangling refs)
- Entity names are case-sensitive and must be unique

### `path` and `derive` (wire response shaping)

By default, each field is read from a **top-level JSON key** matching the field name on the decoded row. Override the location with **`path`** in `domain.yaml` (loads as [`FieldSchema.wire_path`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs)): either a dotted string (`owner.login`) or a YAML list of object keys (`[payload, headers]`).

**`derive`** runs on the extracted JSON value **before** optional scalar [`Transform`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-compile/src/decoder.rs) steps. Rules ([`FieldDeriveRule`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs), `type` tag, `snake_case`):

| `type` | Input shape | Behavior |
|--------|-------------|----------|
| `segments_after_prefix` | JSON string | Strip a URL prefix, split on `/`, take `part_index` (GitHub Issue `repository_url` → `owner` / `repo`). |
| `name_value_array_lookup` | JSON **array** of objects | Find the first element where `match_key_field` equals `equals` (defaults: `match_key_field` = `name`, `value_field` = `value`). Optional **`case_insensitive`** ASCII fold on that comparison (RFC 5322 header names). Return `value_field` from that object; if no match, the field decodes as null. Fits Gmail `payload.headers`, AWS-style `[{ "Key": "…", "Value": "…" }]` tags when `match_key_field` / `value_field` are set to `Key` / `Value`, and similar EAV-lite arrays. |
| `object_key_lookup` | JSON **object** | Return `obj[key]`; optional **`case_insensitive`** resolution of the key string against object keys. |

**`provides` vs full row decode:** HTTP GET responses are decoded using **all** entity fields that have `path`/`derive` wiring. Capability **`provides`** controls summary-vs-complete detection for list/search ([`CGS::effective_provides`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs)) and DOMAIN projection teaching; it does **not** strip extra decoded fields from the cached entity row by itself. If you need two different agent-facing projections over the same HTTP operation, duplicate capabilities with different `provides` is only a *documentation* narrowing unless the runtime adds explicit field filtering.

**Other `apis/*` adopters:** prefer `name_value_array_lookup` wherever a vendor exposes metadata as an array of small records (tags, headers, key/value rows). This repository’s **Gmail** CGS uses it for message headers; AWS/GCP-style tag arrays are a natural next candidate when those surfaces are modeled.

**CML HTTP `body` (beyond `object` / `var`):** the CML expression enum includes vendor-specific builders. Example: **`gmail_rfc5322_send_body`** evaluates to the JSON body for Gmail `users.messages.send` (`raw` plus optional `threadId`) from CML env keys `from`, `to`, `subject`, `plainBody`, and optional `threadId`, `inReplyTo`, `references` — see [`apis/gmail/mappings.yaml`](https://github.com/ryan-s-roberts/plasm-core/blob/main/apis/gmail/mappings.yaml) `message_send_simple` and [`gmail_send_body.rs`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-cml/src/gmail_send_body.rs). **`gmail_rfc5322_reply_send_body`** is the same wire shape but derives defaults from preflight **`parent_*`** keys (see **`invoke_preflight`** on [`CapabilitySchema`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/schema.rs) and Gmail **`message_reply`**).

**`description` on entities and capabilities:** Optional but recommended when it helps agents and humans. Write **short domain prose**—what the thing *is* or what the operation *means* to a user or integrator. Do **not** paste HTTP methods, URL paths, OpenAPI operation ids, bare **`http://` / `https://` links**, or “maps to …” wiring notes; those belong in **`mappings.yaml`** (comments may name products and hosts in words, not pasted URIs) or external API docs. The same rule applies to **`output.description`** for `side_effect` actions: state the **domain effect** (e.g. “message moves to Trash”), not the transport shape (“PATCH, empty body”, “returns 204”). **Exception:** `auth.token_url` and similar **machine** OAuth/OpenID fields may contain a provider token URL string—those are runtime wiring, not human DOMAIN copy.

### Field Types

| Type | YAML value | CLI parser | Operators | Description |
|------|-----------|------------|-----------|-------------|
| String | `string` | string | `=`, `!=`, `contains`, `exists` | Free text |
| UUID | `uuid` | string | `=`, `!=`, `contains`, `exists` | Canonical UUID primary keys — wire values are strings; use for stable opaque ids (e.g. Linear `id`). No `string_semantics` (that is for `string` only). |
| Integer | `integer` | `i64` | `=`, `!=`, `>`, `<`, `>=`, `<=`, `exists` | 64-bit integer |
| Number | `number` | `f64` | `=`, `!=`, `>`, `<`, `>=`, `<=`, `exists` | Floating point |
| Boolean | `boolean` | `--flag`/`--no-flag` | `=`, `!=`, `exists` | True/false |
| Select | `select` | `PossibleValuesParser` | `=`, `!=`, `in`, `exists` | Single enum. Requires `allowed_values`. |
| MultiSelect | `multi_select` | repeatable string | `contains`, `in`, `exists` | Multiple enum. Requires `allowed_values`. |
| Date | `date` | string or integer (see `value_format`) | `=`, `!=`, `contains`, `exists` | **Requires `value_format`:** `rfc3339`, `iso8601_date`, `unix_ms`, or `unix_sec`. **Predicate / expression inputs** are normalized to that wire shape (forgiving parse; UTC for full datetimes). **Display** of API responses is not rewritten via `value_format`. Prompt DOMAIN lines use a generic `…=datetime` hint, not the wire-format token. |
| Array | `array` | repeatable string | `contains`, `in`, `exists` | Homogeneous list. **Requires nested `items:`** describing each element (see below). |
| EntityRef | `entity_ref` | string | `=`, `!=`, `exists` | Foreign key to another entity. Requires `target: EntityName`. ID values may be string or number at runtime. |
| **Blob** | **`blob`** | string (opaque / base64 / attachment-shaped JSON) | `=`, `!=`, `exists` | **Opaque binary or base64-heavy payloads** (file bytes, RFC822 `raw`, attachment `contentBytes`, GitHub Contents `content`, …). **Do not** use `string_semantics` on blob fields (omit the key). Legacy `string` + `string_semantics: blob` loads as blob. |

### Blob / binary (`field_type: blob`)

Use **`blob`** when the wire value is **not** human prose (base64/base64url, opaque octets, or the reserved attachment object), including:

- **Entity fields** populated from APIs that return base64 attachment bodies, binary-safe strings, or a JSON object with reserved **`__plasm_attachment`** metadata (`uri`, `mime_type` / `media_type`, optional `bytes_base64`).
- **Capability parameters** with the same shape (e.g. Gmail `raw`, GitHub Contents **`content`** as base64 in JSON).

**Do not** use **`blob`** for HTML/markdown message bodies meant to be read as text (keep **`string`** with **`string_semantics: markdown`** or **`document`** as appropriate).

**Authoring knobs (entity `fields:` only):**

| Key | Applies to | Notes |
|-----|------------|--------|
| **`mime_type_hint`** | `string` or **`blob`** | Hint for host **tabular** summaries when the cell is reference-only or split (see below). Example: `application/octet-stream`, `message/rfc822` (describe in `description` if you cannot set a single hint). |
| **`attachment_media`** | **`blob` only** | Optional coarse class: `generic`, `image`, `audio`, `video`, `document` — for prompts/tooling; wire shape unchanged. |
| **`agent_presentation`** | `string` or **`blob`** | Optional override; **`blob`** defaults to **reference-only** summaries (same as non-short strings) when unset. |

**Execute summaries (table / TSV):** for CGS **`blob`** entity fields, the agent formatter emits two columns, **`{field}_ref`** and **`{field}_mime`**, so URI (or `(in artifact)`) and MIME stay split. Non-blob columns that hold a full **`__plasm_attachment`** object still use a **single** cell `uri (mime)` or `(in artifact) (mime)` when the payload is bytes-only.

**HTTP runtime:** on **2xx** responses whose body is **not** JSON, the default transport may **coerce** the body into a JSON object  
`{ "__plasm_attachment": { "bytes_base64": "…", "mime_type": "<Content-Type or application/octet-stream>" } }`  
unless the body looks like HTML/XML (error path preserved). Design decoders / `provides` so this shape can land on a **`blob`** field when APIs return raw octets.

**Fixtures (interchange CGS):** see **`fixtures/schemas/test_schema.cgs.yaml`** — entity **`BlobAsset`** declares two blob fields: **`payload`** (`attachment_media: generic`, octet-stream hint) and **`icon_png`** (`attachment_media: image`, PNG hint), plus a minimal **`blob_asset_get`** capability so the catalog validates. **`fixtures/schemas/capability_with_input.cgs.yaml`** includes an optional **`artifact`** `blob` field on **`update_account`** input for interchange coverage.

### Array element typing (`items:`)

Every **`field_type: array`** on an entity field and every capability parameter with **`type: array`** must include a nested **`items:`** map. It uses the same type vocabulary as top-level fields, keyed as **`type:`** (not `field_type`).

```yaml
# Entity field
photoUrls:
  field_type: array
  required: true
  items:
    type: string

# Capability parameter
- name: labelIds
  type: array
  required: false
  items:
    type: string

# Element is entity_ref
- name: assignee_ids
  type: array
  required: false
  items:
    type: entity_ref
    target: User

# Element is select (allowed_values on items, non-empty)
- name: flags
  type: array
  required: false
  items:
    type: select
    allowed_values: [a, b]

# Element is date — put value_format on items, not on the outer array field
- name: dates
  type: array
  required: false
  items:
    type: date
    value_format: rfc3339
```

**Loader constraints:** `items.type` must not be `array` or `multi_select`. For `items.type: select`, **`allowed_values`** on `items` is required and must be non-empty. For `items.type: date`, **`value_format`** must be set on **`items`**. Do not attach `allowed_values` to `items` unless `items.type` is `select`.

**`multi_select`:** `allowed_values` is **required** and must be **non-empty** on the field or parameter itself (not inside `items:`).

### CLI Flag Generation

**Query subcommands**: flags are generated from the capability's `parameters:` — one flag per declared parameter, typed by the parameter’s **`type:`** (same vocabulary as entity **`field_type:`**). No parameters declared = no filter flags (just pagination controls from the CML `pagination` block). Entity fields do **not** generate query flags.

| Parameter `type` | Flag generated | Parser |
|------------------------|----------------|--------|
| `string` / `uuid` / `date` / `entity_ref` / **`blob`** | `--param` | string |
| `integer` | `--param` | `i64` |
| `number` | `--param` | `f64` |
| `boolean` | `--param` (flag, no value) | SetTrue |
| `select` | `--param` with `[possible values: ...]` | PossibleValuesParser |
| `multi_select` / `array` | `--param` repeatable | string (Append) |

**Relation subcommands**: flags are generated from the **target entity's query capability `parameters:`** — same rules as query subcommands. If the target has no query capability or no parameters, the relation subcommand shows only pagination flags (if available).

**Create / update / action subcommands**: flags come from the capability's `input_schema` (same vocabulary, different purpose — these are write inputs, not query filters).

### EntityRef Composition (CLI auto-derived subcommands)

Beyond query filter flags, `entity_ref` fields drive three additional CLI features:

| Feature | CLI example | How it works |
|---|---|---|
| **FK navigation** | `order 5 pet-id` | Subcommand per EntityRef field (when target has Get cap). Resolves to full target entity. |
| **Reverse traversal** | `pet 10 orders` | Auto-derived when a query capability on another entity has an `entity_ref` parameter targeting this entity. Injects `petId=10` as predicate. |
| **Cross-entity filter** | `order query` with predicate `pet.status=available` | Dot-path predicates decomposed: push-left (query foreign first, inject IDs) or pull-right (client-side N+1 filter). |

**Naming conventions:**
- FK navigation: `petId` → subcommand `pet-id` (camelCase → kebab-case)
- Reverse traversal: target entity `Order` → subcommand `orders` (pluralized lowercase)

**Authoring for reverse traversal:** Ensure the query capability parameter uses `type: entity_ref` with `target` matching the field's target. The CGS validator checks that `entity_ref` targets align between entity fields and capability parameters.

### Capabilities

A capability declares an operation available on an entity.

```yaml
capabilities:
  <entity>_<operation>:       # unique name, conventionally entity_verb
    kind: <kind>              # see Capability Kinds below
    entity: <EntityName>      # must be a defined entity
    parameters:               # optional, for capabilities with typed params
      - name: <param>
        type: <field_type>
        description: <string> # optional; short human meaning for agents and DOMAIN gloss (see below)
        target: <EntityName>  # required when type is entity_ref
        required: <bool>
        allowed_values: [...]  # for select / multi_select (multi_select: non-empty)
        items:                  # required when type is array — element typing (see Array element typing)
          type: <element_type>
        role: <role>          # optional semantic role — see Parameter Roles below
```

Capability parameters use the same `field_type` vocabulary as entity fields (including `entity_ref` + `target`).

**`description` on capability parameters:** Optional. When the prompt uses a **symbolic** `PromptRenderMode` (**compact** or **tsv**, via `--symbol-tuning compact|tsv` on `plasm-mcp` / `plasm-repl` / `plasm-eval` — not a legacy `symbol_tuning: true` flag), each parameter gets a `p#` gloss line in DOMAIN (compact: line before first use; tsv: folded into the teaching table). The gloss shows the parameter type and, after a middle dot (`·`), either this **`description`** (trimmed, possibly truncated) or, if omitted, the **wire `name`** from YAML. Use the same style as entity field descriptions: short domain prose, not HTTP or mapping trivia.

### Parameter Roles

The optional `role:` annotation declares the **semantic purpose** of a parameter. This helps agents and LLM tooling understand how the parameter affects results, beyond just its data type.

| `role:` | Semantics | Examples |
|---------|-----------|---------|
| `filter` | Equality/range predicate on entity field values **(default)** | `status`, `archived`, `due_date_gt` |
| `search` | Free-text relevance query — server ranks results | `q`, `query`, `search` |
| `sort` | Sort field selector | `order_by`, `sort_by` |
| `sort_direction` | Ascending/descending companion to `sort` | `sort`, `direction` |
| `response_control` | Payload shape/detail control — does not filter results | `embed`, `fields`, `inc`, `exc` |
| `scope` | Parent-entity pivot wired into the URL path (always `entity_ref`, required) | `team_id`, `space_id` |

`role:` is informational metadata — it does not change how the parameter is transmitted over HTTP. Transmission is controlled entirely by the CML `query:` or `path:` block in mappings.yaml.

### Foreign key fields (`entity_ref`)

Use `entity_ref` when a field stores another entity’s primary key. Declare the referenced entity in `target`. The CGS validates that `target` names a defined entity.

For **`query`** capabilities, if a parameter has the **same name** as an entity field and both are `entity_ref`, their `target` values must match. That ties the HTTP/query parameter to the domain FK and enables static reverse-traversal lookup: `CGS::find_reverse_traversal_caps("Pet")` returns every query capability whose parameters include `EntityRef(Pet)`.

CML does not change: variables (e.g. `team_id`) are still bound from the compiled environment. Typing is enforced in the CGS only.

Example (two-sided pattern):

```yaml
entities:
  Order:
    id_field: id
    fields:
      id: { field_type: integer, required: true }
      petId:
        field_type: entity_ref
        target: Pet
        required: false

capabilities:
  order_findByPetId:
    kind: query
    entity: Order
    parameters:
      - name: petId
        type: entity_ref
        target: Pet
        required: true
```

### Capability Kinds

| Kind | Semantics | CLI position | Requires ID |
|------|-----------|-------------|-------------|
| `query` | Filter/list a collection by field predicates | `entity query --flags` | No |
| `search` | Full-text relevance search; primary input is a `q`/`query`/`search` param | `entity search --flags` | No |
| `get` | Fetch single by ID/key | `entity <id>` (implicit) | Yes |
| `create` | Create new entity | `entity create --flags` | No |
| `update` | Modify existing entity | `entity <id> update --flags` | Yes |
| `delete` | Remove entity | `entity <id> delete` | Yes |
| `action` | Any other operation | `entity <id> actionName --flags` | Yes |

### Action output: `provides:` vs `output.side_effect`

`kind: action` must declare **how the response is modeled**:

1. **Entity projection** — non-empty `provides:` lists which entity fields the HTTP response populates (same rules as other kinds that return entity-shaped JSON).
2. **No projection** — the call is **effectful** (something changes) but the response is empty, opaque, or not mapped onto entity fields. Declare **`output`** with **`type: side_effect`** and a **non-empty `description:`** string that states **what** changes in the domain (not generic “updates resource”, and not HTTP status or path trivia).

There is **no** `output.type: none` in the schema: it invited silent, incomplete modeling. Side-effect actions must always say what they *do*.

```yaml
capabilities:
  message_trash:
    description: Move a message to TRASH
    kind: action
    entity: Message
    output:
      type: side_effect
      description: "Moves the message to Trash; the response carries no fields mapped onto this entity."

  page_get_markdown:
    kind: action
    entity: Page
    provides: [id, markdown, truncated]   # projection path — no side_effect block needed
```

**Validation:** CGS `validate` rejects (a) `action` with neither `provides` nor `output`, and (b) `side_effect` with missing or whitespace-only `description`.

**`query` vs `search`**: Use `query` when the API filters by field equality/range predicates (`status=available`, `archived=true`). Use `search` when the primary input is a free-text relevance query (`q=pikachu`) and results are ranked, not field-filtered. Search capabilities are excluded from reverse-traversal FK lookups (`find_reverse_traversal_caps`). CLI verb is `search` not `query`.

### Multiple query capabilities per entity (primary vs named)

An entity can have multiple `kind: query` (or `kind: search`) capabilities. The CLI automatically determines which one gets the `query`/`search` verb and which get named subcommands:

| Capability shape | CLI position | Example |
|------------------|-------------|---------|
| No required params (or only optional filters) | `entity query --flags` (primary) | `spell query --level 1` |
| Required params but **no** `role: scope` | First one: `entity query --flags` (primary); others: `entity cap-name --flags` | `pet query --status available` (primary), `pet findbytags --tags fluffy` (named) |
| Required `role: scope` param | Always named: `entity cap-name --scope_param value` | `spell class-spells --class_index wizard --level 1` |

**Detection is automatic from parameter roles — no extra annotation needed.** If a query capability has a required `role: scope` parameter, it always gets a named subcommand. Among non-scoped caps, the parameterless one (or the first with required params) becomes primary.

**Validation rule:** At most one parameterless (no required params) query/search per entity.

### Required Parameters

When a capability has a parameter with `required: true`, the CLI enforces it:

```yaml
capabilities:
  pet_findByStatus:
    kind: query
    entity: Pet
    parameters:
      - name: status
        type: select
        required: true      # CLI will reject if --status not provided
        allowed_values: [available, pending, sold]
```

CLI behavior:
```
$ pet query
error: the following required arguments were not provided: --status <status>

$ pet query --status INVALID
error: invalid value 'INVALID' [possible values: available, pending, sold]

$ pet query --status available
(executes)
```

### Relations and Navigation

Relations create navigation subcommands. The **target entity's query capability `parameters:`** become filter flags.

```yaml
# In domain.yaml:
entities:
  Pet:
    relations:
      tags:
        target: Tag
        cardinality: many
  Tag:
    fields:
      name:
        field_type: string
```

CLI behavior:
```
$ pet 10 tags              # navigate Pet→Tag relation
$ pet 10 tags --name Fluffy  # navigate + filter by Tag's fields
```

#### Scoped many-relations — `materialize: query_scoped` / `query_scoped_bindings`

When a REST API uses a sub-resource URL pattern (`/parent/{parent_id}/children`) or a scoped list query, declare **`materialize`** on the **many** relation so chain traversal knows which target capability parameters to fill from the parent row.

**Single scope parameter** (`query_scoped`) — **`capability`** names the exact target `query` / `search` capability; **`param`** is its scope field; the value comes from the parent entity’s **`id_field`** (same behavior the runtime historically called “via_param”):

```yaml
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

**Multiple scope parameters** (`query_scoped_bindings`) — same required **`capability`**, plus map each **target capability parameter name** to a **parent entity field name**:

```yaml
entities:
  Calendar:
    relations:
      events:
        target: Event
        cardinality: many
        materialize:
          kind: query_scoped_bindings
          capability: event_list
          bindings:
            calendarId: id
```

**CLI behavior**: scope arguments are **not** separate flags — they are filled from the parent entity id / fields:
```
$ page "abc-123" blocks
$ calendar "cal-1" events
```

**REPL / expression syntax:**
```
Page("abc-123").blocks
Page~"Agentium".blocks
```

**Multiline / structured string values** in predicates and method arguments use a bash-inspired **tagged** **`<<TAG`** heredoc (not Rust `r#` strings): `<<TAG\n` … `\nTAG\n` with `TAG` alone on a closing line (trimmed), or `TAG)` / `TAG,` / `TAG}` glued on that line. DOMAIN prompts echo this when `string_semantics` is not plain `short`.

**Compound `entity_ref` scope parameters** (one param that unpacks to several path/query slots, e.g. repository identity) use runtime **scope splat** and optional **`scope_aggregate_key_policy`** on the capability — distinct from **`query_scoped_bindings`** (several named params bound to parent fields).

#### Multiple projections of the same entity — `provides:` and auto-resolution

When multiple API endpoints return disjoint field subsets of the same logical resource (same `id`, different fields), model them as **one entity** with `required: false` on projection-only fields. Declare `provides:` on each capability to enable auto-resolution.

```yaml
entities:
  Page:
    fields:
      url: { field_type: string }              # from page_get
      markdown:
        field_type: string
        required: false                         # from page_get_markdown; auto-resolved on demand

capabilities:
  page_get:
    kind: get
    entity: Page
    provides: [id, url, public_url, created_time, last_edited_time, in_trash, archived]

  page_get_markdown:
    kind: action                               # enriches same Page entity with content
    entity: Page
    provides: [id, markdown, truncated]        # declares the content projection
```

**`provides:`** declares which entity fields a capability populates in its response. The runtime builds a reverse index (`field → capability`) and uses it to auto-invoke the correct capability when a projected field is absent from cache.

**Auto-resolution** in action:
```
plasm> Page("abc")[markdown]
# "markdown" absent from cache → auto-invokes page_get_markdown("abc")
# additive merge → Page:abc now has markdown + all metadata fields
{"markdown": "# Full page content..."}
```

**`provides:` defaults** when omitted (backward-compatible):
- `get` / `query` / `search` → provides all entity fields (optimistic)
- `create` / `update` / `delete` / `action` → provides nothing (declare explicitly)

**Recommendation for `kind: get`:** Declare an explicit ordered **`provides:`** listing every scalar field the detail response materializes (same names as `entities.<Entity>.fields`), with **`id_field` first** and the rest in the same order as in the entity block. That keeps **decode / `field_providers`** accurate and fixes the **DOMAIN heading projection list** (`Entity  ;;  [f1,…,fN] - …`) to that order instead of `id_field` + lexicographic fallback.

For **`action`**, if you rely on the default empty `provides`, you **must** add **`output: { type: side_effect, description: "…" }`** (see **Action output** above). Other kinds do not require `output` when `provides` is empty unless you add structured `output` for documentation.

**Three-way capability contract** — full field-level provenance:

| Annotation | Direction | Meaning |
|---|---|---|
| `parameters:` | input | What the API endpoint accepts |
| `provides:` | output | Which entity fields the response populates |
| `mutates:` | write set | Which entity fields this capability changes *(roadmap)* |

**Recognition**: path `/resource/{id}` + `/resource/{id}/suffix`; both return same `id`; disjoint fields.

---

## CML (Capability Mapping Language) — mappings.yaml

CML defines how each capability translates to an HTTP request (or GraphQL over HTTP when **`transport: graphql`**). It is a declarative template language — no loops; conditionals are **`if`** with **`exists`**, **`equals`**, or **`bool`** conditions (see below), total evaluation.

### Structure

Each capability name from domain.yaml gets one entry:

```yaml
<capability_name>:
  method: GET|POST|PUT|PATCH|DELETE
  path: <path_segments>
  query: <cml_expr>       # optional
  body: <cml_expr>        # optional
  headers: <cml_expr>     # optional
  pagination: <pagination_block>   # optional; query capabilities only
```

### Pagination (CML) — mappings.yaml only

Pagination is **transparent in the domain model**: `domain.yaml` still uses `kind: query` for list capabilities. HTTP pagination is declared only in **CML** so the execution engine can merge page parameters, decode the configured items path, and loop until completion.

When a mapping includes `pagination`, `plasm-agent` adds **built-in** CLI flags (not from entity fields). **`--limit`** and **`--all`** are always present; starting-position flags are derived from the **`pagination.params`** map (counter / fixed / `from_response` keys and `location`) so **`--help`** only lists what applies to that capability.

**LLM / MCP execute:** paginated queries return **one upstream page** by default. When more pages exist, the host mints an opaque session handle (`pg1`, `pg2`, …) and surfaces **`has_more`** plus a compact **`page(pgN)`** follow-up (and `_meta.plasm.paging` when MCP meta is enabled). Models continue with **`page(pgN)`** or **`page(pgN, limit=50)`**; transport-specific param names stay out of the prompt.

| Flag | Effect (CLI) |
|------|--------------|
| `--limit N` | Return at most **N** entities total (may issue multiple upstream requests). |
| `--all` | Fetch until the API reports no next page (runtime safety cap: 10_000 pages). |

Default when neither is set: **first page only** (LLM execute matches this unless the model issues `page(pg#)` continuations).

#### Pagination block schema (composable `PaginationConfig`)

Rust ground truth: [`PaginationConfig`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-cml/src/cml.rs) in **`mappings.yaml`** under `pagination:`.

```yaml
pagination:
  location: query            # query | body | link_header | block_range
  body_merge_path: [variables, o, paginate]   # optional; when location: body
  response_prefix: [data, issues, pageInfo]   # optional; scope for stop_when / from_response
  stop_when:
    field: hasNextPage
    eq: false
  params:
    offset:
      counter: 0
      step: 20
    limit:
      fixed: 20
    after:
      from_response: endCursor
```

Decode shape for list bodies remains on the mapping’s **`response:`** / decoder (`items`, `items_path`, …) — not inside `pagination:`.

#### `location` (summary)

| `location` | Role |
|------------|------|
| `query` (default) | Merge `params` into the query string. |
| `body` | Merge `params` under `body_merge_path` (or top-level JSON body). |
| `link_header` | Next page from `Link: …; rel="next"` (**Live** mode; replay caveats without stored headers). |
| `block_range` | EVM log ranges (`from_block` / `to_block`). |

#### Inference heuristics (LLM / authoring)

| OpenAPI / response signal | Likely `pagination.params` / `location` shape |
|-----------------------------|-----------------------------------------------|
| Query params `offset` + `limit` | Counters + fixed limit, `location: query` |
| Query param `page` (no `offset`) | `page` counter + optional `per_page` / `size` fixed |
| Params `cursor`, `start_cursor`, `after` | `from_response` continuation fields |
| Params `starting_after` / `ending_before` | Keyset-style `after` / `before` params |
| Schema `Paginated*` with `count`, `next`, `previous`, `results` | Offset/page + `response_prefix` if nested |
| `has_more` + `data` | Often `stop_when` + `from_response` on nested `pageInfo` |
| `next_cursor` + `results` | Cursor param + `from_response` |
| No list pagination parameters | omit `pagination` |

#### GraphQL (`transport: graphql`)

GraphQL list capabilities use the **same** composable `pagination:` shape as HTTP (see `apis/graphqlzero`, `apis/linear`, etc.):

- **`location`**: typically `body` with variables merged under **`body_merge_path`** (e.g. `[variables]` or `[variables, o, paginate]`).
- **`params`**: maps keys merged at that path — e.g. Relay **`first`** / **`after`** with `{ from_response: endCursor }`.
- **`response_prefix`**: optional path from the **root JSON response** for **`stop_when`** and **`from_response`** (e.g. `[data, issues, pageInfo]`).
- **`--limit` / `--all`**: same CLI behavior as HTTP when `pagination` is present.

**CML `object` fields: `Value::Null` keys are omitted at eval time.** In [`eval_cml`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-cml/src/cml.rs), when building a `type: object`, any key whose sub-expression evaluates to **`Value::Null`** is **not inserted** into the parent object. So the common optional pattern `type: if` / `condition: exists` / `else_expr: { type: const, value: null }` produces **no key** for missing inputs—well-typed **omit** semantics in the compiled `Value`, not only on the wire. Nested objects are evaluated recursively with the same rule.

**HTTP JSON body: null keys are still stripped before POST** (`strip_null_fields` in `plasm-oss/crates/plasm-runtime/src/http_transport.rs`) as a safety net for any remaining `null` in nested JSON (e.g. from non-`object` paths). Together, CML omit + transport strip match typical **partial GraphQL mutation inputs** (omit field = leave unchanged). Mappings YAML may still show `else: null`; that evaluates to null, then the key is dropped at object construction.

**Explicit JSON `null` to clear a field:** A key whose value must be a literal `null` in JSON (e.g. clear an optional assignee) is **not** representable if the only way to express it is `Value::Null` inside a CML object (it will be omitted). A future extension could add a dedicated CML/`Value` form for explicit null; replay **`RequestFingerprint`** hashes the compiled `CompiledRequest` body, which after object-omit no longer carries null entries for omitted optionals—aligned with the wire.

### Query result hydration (runtime + `plasm-agent`)

This is **not** part of CML or `domain.yaml`. After a **query** succeeds, if the CGS defines a **`get`** capability on the **same entity** as the query’s `entity`, the runtime **defaults** to:

1. Merging decoded list rows into `GraphCache` as **`completeness: summary`**.
2. For each returned **`Ref`**, issuing the **get** mapping (concurrent, up to **`ExecutionConfig::hydrate_concurrency`**, default **5**) unless the cache already holds **`complete`** for that ref.
3. Merging GET responses as **`complete`** and returning entities in **query result order**.

**Opt out (list-shaped output only):**

- **`plasm-agent`:** `--summary` on `entity query …` and on **relation** subcommands that dispatch a target `QueryExpr`. The flag exists only when the **queried** entity has a **get** capability.
- **IR / programmatic:** `QueryExpr.hydrate = Some(false)` for one query; or **`ExecutionConfig.hydrate = false`** for the whole engine.

**When hydration does not run:** the entity has **query** but **no** **get** mapping (nothing to upgrade with).

**Interaction with pagination:** pagination collects the ordered list of refs first; hydration runs **after** pages are merged (same concurrency and skip rules per row).

**Cache semantics:** `CachedEntity.completeness` is **`summary`** or **`complete`**. Merge **never** overwrites **`complete`** with **`summary`**. **`execute_get`** returns a cache hit only for **`complete`** rows; **`summary`** forces a GET so `pet 10` after `pet query` still deepens the payload.

### Path Segments

An ordered list of literal strings and variable references:

```yaml
path:
  - type: literal
    value: pet                # → /pet
  - type: var
    name: id                  # → /pet/{id}
  - type: literal
    value: uploadImage        # → /pet/{id}/uploadImage
```

### CML Expressions

#### Variable reference
```yaml
type: var
name: <variable_name>
```

#### Constant
```yaml
type: const
value: <any_value>
```

#### Object (key-value pairs)
```yaml
type: object
fields:
  - - key_name
    - type: var
      name: value_var
  - - another_key
    - type: const
      value: fixed_value
```

#### Conditional (`if`)

```yaml
type: if
condition:
  type: exists
  var: <variable_name>
then_expr: <cml_expr>
else_expr: <cml_expr>
```

**Conditions** (`CmlCond` in `plasm-cml`): **`exists`** (variable bound), **`equals`** (compare two expressions), **`bool`** (truthy eval of a sub-expression). Prefer **`exists`** for optional query params; use **`equals`** / **`bool`** when the API needs explicit sentinels or flags.

#### Array join (CSV / pipe serialisation)

Join an array variable into a single delimited string. Use when the API expects a comma-separated or pipe-separated list rather than repeated query keys.

```yaml
type: join
sep: ","          # separator (use "|" for pipe-delimited)
expr:
  type: var
  name: genres    # must resolve to Value::Array
```

In the `query:` block:
```yaml
# Emits ?genres=1,2,3 (CSV)
query:
  type: object
  fields:
    - - genres
      - type: join
        sep: ","
        expr: { type: var, name: genres }

# Emits ?ids=1|2|3 (pipe)
    - - ids
      - type: join
        sep: "|"
        expr: { type: var, name: ids }
```

**Repeated-key arrays** (`?embed=a&embed=b`): Use a plain `Var` without `join`. The HTTP execution layer automatically expands `Value::Array` query param values into repeated `key=value` pairs:
```yaml
# Emits ?embed=cast&embed=episodes
    - - embed
      - type: var
        name: embed
```

### Variable Resolution

The execution engine populates the CML environment before template evaluation:

| Operation | Variables set |
|-----------|--------------|
| **Query** | `filter` (compiled BackendFilter), each predicate field=value pair (e.g. `status`=`"available"`), `projection` |
| **Get** | `id`, plus all path var names from the CML template set to the ID value |
| **Create** | `input` (Value::Object from CLI flags) |
| **Delete** | `id`, plus all path var names |
| **Update/Action** | `id`, path var names, `input` |

This means: if the spec uses `{petId}` in the path, the CML template should use `name: id` (normalized) OR `name: petId` (the engine sets both).

### Compilation: CML → HTTP Request

```
CML template + environment variables
    ↓ eval_path_segment() per segment
    → URL path string
    ↓ eval_cml() on query expr
    → URL query parameters
    ↓ eval_cml() on body expr (when body_format is json or form_urlencoded)
    → JSON or scalar-map request body
    ↓ eval_cml() on each multipart.parts[].content (when body_format is multipart)
    → compiled multipart parts (null parts omitted)
    ↓ assemble
    → CompiledRequest { method, path, query, body, body_format, multipart, headers }
```

The compiled request is deterministic: same template + same env = same HTTP request. This enables blake3 fingerprinting for record/replay.

### Example: Full Mapping

OpenAPI endpoint:
```
GET /pet/findByStatus?status=available
```

CML mapping:
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
```

When the user runs `pet query --status available`:
1. Predicate: `status = "available"`
2. CML env: `{status: "available"}`
3. Path: `/pet/findByStatus`
4. Query: `?status=available`
5. HTTP: `GET /pet/findByStatus?status=available`

### Example: Path Variable

OpenAPI endpoint:
```
DELETE /pet/{petId}
```

CML mapping:
```yaml
pet_delete:
  method: DELETE
  path:
    - type: literal
      value: pet
    - type: var
      name: id
```

When the user runs `pet 10 delete`:
1. Entity ID: `"10"`
2. CML env: `{id: "10", petId: "10"}` (engine sets both)
3. Path: `/pet/10`
4. HTTP: `DELETE /pet/10`

### Example: Request Body

OpenAPI endpoint:
```
POST /pet  (body: Pet schema)
```

CML mapping:
```yaml
pet_create:
  method: POST
  path:
    - type: literal
      value: pet
  body:
    type: var
    name: input
```

When the user runs `pet create --name Fido --status available`:
1. Input: `{name: "Fido", status: "available"}`
2. CML env: `{input: {name: "Fido", status: "available"}}`
3. Body: `{"name": "Fido", "status": "available"}`
4. HTTP: `POST /pet` with JSON body

### Request body formats (`body_format`)

Default is **`json`**: `body:` is evaluated to a Plasm [`Value`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-core/src/value.rs) and POSTed as `application/json` (nulls stripped on the wire).

**`form_urlencoded`:** `body:` must evaluate to a **flat object** of string/number/bool fields; the runtime sends `application/x-www-form-urlencoded`.

**`multipart`:** do **not** set `body:`. Instead set **`multipart:`** with a **`parts:`** list. Each part has:

- **`name`:** form field name (required).
- **`file_name`:** optional `Content-Disposition` filename (typical for file parts).
- **`content_type`:** optional MIME for the part (JSON object/array parts default to `application/json` when omitted).
- **`content`:** a CML expression evaluated like `body:` fields. If it evaluates to **null**, the part is **omitted** (optional metadata).

**File bytes:** evaluate `content` to an attachment-shaped JSON object with reserved **`__plasm_attachment`** and non-empty **`bytes_base64`** (same shape as decoded HTTP binary and CGS **`blob`** fields). URI-only attachments are rejected for outbound multipart. In **`domain.yaml`**, model the slot as **`type: blob`** when you want strict typing; **`type: json`** is also accepted for attachment-shaped values (e.g. to keep DOMAIN prompts minimal in small demo catalogs).

Example (OpenAPI-style upload + optional string field):

```yaml
body_format: multipart
multipart:
  parts:
    - name: additionalMetadata
      content:
        type: if
        condition: { type: exists, var: additionalMetadata }
        then_expr: { type: var, name: additionalMetadata }
        else_expr: { type: const, value: null }
    - name: file
      file_name: upload.png
      content:
        type: var
        name: file
```

Rust ground truth: [`HttpBodyFormat`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-cml/src/cml.rs), [`MultipartBodySpec`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-cml/src/cml.rs), wire build in [`http_transport.rs`](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-runtime/src/http_transport.rs).

---

## Authentication

Declare authentication once at the top level of `domain.yaml` under the `auth:` key. For **public** HTTP APIs (no outbound credentials), use `scheme: none` so tooling can tell intentional “no auth” from a missing block. Omitting `auth` entirely is still accepted for backward compatibility but is ambiguous for UX (tool-model cannot distinguish “public” from “not yet modeled”). Credential-bearing schemes read secrets at request time from **environment variables** or hosted KV via `SecretProvider`. No secrets are stored in schema files.

**Constraint:** `auth: { scheme: none }` cannot be combined with a top-level `oauth:` block (OAuth implies delegated auth).

### Supported schemes

| Scheme | YAML `scheme:` value | Injected as | Env var fields |
|--------|----------------------|-------------|----------------|
| No outbound credentials (public API) | `none` | *(nothing)* | — |
| Static API key in a header | `api_key_header` | `<header>: <value>` | `header`, `env` |
| Static API key in query param | `api_key_query` | `?<param>=<value>` | `param`, `env` |
| Bearer token | `bearer_token` | `Authorization: Bearer <token>` | `env` |
| OAuth 2.0 client credentials | `oauth2_client_credentials` | `Authorization: Bearer <token>` (token cached + auto-refreshed) | `token_url`, `client_id_env`, `client_secret_env`, `scopes` (optional) |

### Examples

```yaml
# Public / open HTTP API (e.g. PokéAPI, D&D 5e)
auth:
  scheme: none

# API key sent as a query parameter (e.g. RAWG, OMDB)
auth:
  scheme: api_key_query
  param: key        # query param name
  env: RAWG_API_KEY # name of the env var holding the secret

# API key sent as a query parameter with a different param name (e.g. OMDb)
auth:
  scheme: api_key_query
  param: apikey
  env: OMDB_API_KEY

# API key sent as a query parameter for NYTimes
auth:
  scheme: api_key_query
  param: api-key
  env: NYTIMES_API_KEY

# Bearer token (e.g. ClickUp personal API token, Notion, Tavily)
auth:
  scheme: bearer_token
  env: CLICKUP_API_TOKEN

# Static API key in a named header
auth:
  scheme: api_key_header
  header: X-Api-Key
  env: MY_SERVICE_API_KEY

# OAuth 2.0 client credentials (e.g. Spotify)
auth:
  scheme: oauth2_client_credentials
  token_url: https://accounts.spotify.com/api/token
  client_id_env: SPOTIFY_CLIENT_ID
  client_secret_env: SPOTIFY_CLIENT_SECRET
  scopes:
    - user-read-private     # optional; omit if not needed
```

### How auth injection works

Auth is injected **before** CML-declared `headers:` so that per-capability mappings can override credentials if ever needed. Pagination continuation requests (Link header follow-ups) receive the same credentials automatically.

For `oauth2_client_credentials`, the runtime:
1. Checks a per-`AuthResolver` in-memory cache (`tokio::sync::RwLock<Option<CachedToken>>`).
2. If the cached token is still valid (with a 30-second safety margin), uses it directly.
3. Otherwise exchanges `client_id` + `client_secret` for a fresh token via `POST token_url`, caches it, then proceeds.

### Runtime extension

The `SecretProvider` trait in `plasm-runtime::auth` is `dyn`-compatible. To use a secret store other than env vars, implement `SecretProvider` and pass it to `AuthResolver::new(scheme, Arc::new(my_provider))`.

---

## Execution Pipeline

```
CLI args
  → clap parses typed flags (rejects invalid values/types/missing required)
  → dispatch builds Expr (Query/Get/Create/Delete/Invoke)
  → type_check_expr validates against CGS
  → normalize predicate (flatten, DeMorgan, dedup)
  → compile predicate to BackendFilter
  → populate CML environment
  → eval CML template → CompiledRequest
  → execute HTTP (live/replay/hybrid)
  → normalize response (bare array → {results: [...]})
  → decode response via schema-driven decoder (fields from CGS entity)
  → merge decoded entities into graph cache (stable Ref identity)
  → after **query**, optional concurrent **GET** per row when entity has **get** (unless `--summary` / `QueryExpr.hydrate == Some(false)`)
  → format output (json/table/compact)
```

Per **compiled** capability, the same CGS + CML + input yields the same primary HTTP request (fingerprint-based replay). **Pagination** and **hydration** add further requests whose count depends on result size, cache state, and flags — each follow-up request is still compiled and replayed like any other GET.
