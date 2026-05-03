# Plasm Language Specification

!!! abstract "Specification status"

```
This page is the public contract for the Plasm surface language used by MCP `plasm` / `plasm_run` and by host integrations that submit Plasm programs.
```

Plasm is the compact language agents write after reading a live **DOMAIN** table. The DOMAIN table teaches the catalog-specific symbols and examples; this specification defines the stable language forms those examples use.

## 1. Scope and Conformance

Plasm covers:

- Single expressions over DOMAIN symbols.
- Multi-line programs with bindings and final roots.
- Query predicates, gets, method calls, actions, and relation traversal.
- Postfix transforms (`.limit`, `.sort`, projections, aggregation, grouping, `.singleton`, `.page_size`).
- Paging continuations (`page(...)`).
- Data literals, derives, row-wise effects, and bracket render.
- Tagged heredoc string literals for structured text.

The runtime may lower Plasm to an internal plan DAG, but that DAG is not a user-facing syntax. Diagnostics, prompts, copy, and documentation call the user surface **Plasm expressions** or **Plasm programs**.


| Term           | Meaning                                                                |
| -------------- | ---------------------------------------------------------------------- |
| **Expression** | One Plasm path/call/transform chain.                                   |
| **Program**    | One or more statements, usually bindings followed by final roots.      |
| **DOMAIN**     | The prompt table that maps session symbols to valid expression shapes. |


## 2. DOMAIN Gloss and Symbol Tuning

Agents normally see Plasm through a **TSV DOMAIN** block:

```tsv
plasm_expr	Meaning
e1  ;;  Repository  ;;  [owner,repo,description] - source-code repository
    e1(p1="ryan-s-roberts", p2="plasm-core")  ;;  get a repository
    e1{p3="open"}.limit(20)  ;;  list repositories filtered by status
p1  ;;  string · repository owner
p2  ;;  string · repository name
p3  ;;  string · lifecycle state
```

The `plasm_expr` column teaches expression shapes. Text after `;;` is **gloss** for the agent reading DOMAIN: entity meaning, field/parameter types, optional parameters, result hints, and short descriptions. Gloss is not part of the Plasm program an agent should submit; it is instruction text that explains how to use the symbols. DOMAIN renderers keep every expression shown before `;;` valid Plasm so examples remain executable teaching examples.

Symbol tuning replaces verbose catalog names with session-local symbols:


| Symbol | Meaning                                                                 | Allocation                                                     |
| ------ | ----------------------------------------------------------------------- | -------------------------------------------------------------- |
| `e#`   | Exposed entity block.                                                   | Order of first entity exposure in this logical session.        |
| `m#`   | Capability/method label on an entity.                                   | Assigned to newly exposed `(entity, capability)` pairs.        |
| `p#`   | Identifier: field, relation, capability parameter, or projection field. | Assigned to visible identifiers in the current exposure slice. |


Symbols are **session-local**. Do not cache them across a new logical session or after a host says the symbol space is new. Within one logical session, exposure is append-only: once `e1`, `m4`, or `p9` has a meaning, later DOMAIN waves do not reassign it. See [Incremental DOMAIN prompts](incremental-domain-prompts.md) for wave mechanics.

### Symbol Expansion

Host integrations parse model output by expanding symbols first:

- `e1` expands to the owning entity name in the exposed catalog.
- `e1.m3(...)` expands to the capability taught as `m3` for `e1`.
- `p7` expands according to position: field path, parameter name, relation name, or projection field.

If a DOMAIN row shows `e1{p3="open"}`, agents should use `p3` exactly. The implementation expands it before type checking.

## 3. Lexical Rules

- Statements are separated by real newline characters (`U+000A`) in multi-line programs.
- Empty lines are ignored.
- `;;` begins a gloss/comment suffix on a physical line, outside heredoc bodies.
- Labels are bare identifiers used for program bindings; `_`, `$`, and `return` are reserved.
- Final roots are written as a bare comma-separated list, with no `return` keyword.
- Delimiters `(...)`, `{...}`, `[...]`, and heredoc bodies are respected when splitting top-level commas or assignments.

Correct:

```text
repo = e2(p1="ryan-s-roberts", p2="plasm-core")
commits = repo.commits.limit(20)
commits[p3,p4]
```

Incorrect:

```text
repo = e2(...) commits = repo.commits commits
return commits
```

## 4. Values

Plasm values appear in predicates, method arguments, data literals, and templates.


| Form              | Example                                     | Notes                                                                               |
| ----------------- | ------------------------------------------- | ----------------------------------------------------------------------------------- |
| String            | `"open"`                                    | Quoted strings use backslash escapes for embedded quotes.                           |
| Number            | `42`, `3.14`                                | Interpreted by schema context.                                                      |
| Boolean           | `true`, `false`                             | Lowercase.                                                                          |
| Null              | `null`                                      | Useful where optional object fields are intentionally omitted by the mapping layer. |
| Array             | `["bug","docs"]`                            | Elements may be literals or typed references.                                       |
| Object            | `{title: "Fix docs", body: report.content}` | Used for data/derive shapes and object parameters.                                  |
| Binding reference | `report`, `report.content`                  | Valid in program contexts after `report` is bound.                                  |
| Row reference     | `_.id`, `_.title`                           | Valid only inside row-wise effect templates after `=>`.                             |
| Heredoc string    | `<<TAG ... TAG`                             | Structured multi-line string literal.                                               |


Entity references in predicates expect identity for the target entity, not an arbitrary decoded row. If a binding yields a typed row for that target, the checker may narrow it to identity when key fields are top-level.

## 5. Single Expressions

### Entity Query

An entity symbol with no arguments reads a collection or list surface when DOMAIN teaches that shape:

```text
e1
e1.limit(10)
e1.sort(p5, desc)
```

### Entity Get / Constructor

Parentheses identify a specific entity instance or invoke a get-like capability:

```text
e2(p1="ryan-s-roberts", p2="plasm-core")
e7("spell/fireball")
```

The exact required arguments come from DOMAIN. Use named arguments when DOMAIN shows named `p#` slots.

### Predicates

Brace predicates filter query/search surfaces:

```text
e1{p3="open"}
e1{p3!="closed", p4 contains "parser"}
e1{p5 exists}
```

Supported predicate structure is catalog-typed and includes comparison, containment, existence, conjunction/disjunction/negation where the parser and type checker accept them. The DOMAIN examples are authoritative for which fields and operators are valid for an entity.

### Search

When DOMAIN teaches a search form, `~` applies a text query:

```text
e1~"panic in parser"
e1~"oauth refresh".limit(5)
```

Search exists only for entities with a CGS search capability exposed in the current DOMAIN slice.

### Method and Action Calls

Capabilities may appear as methods on an entity:

```text
e2(p1="ryan-s-roberts", p2="plasm-core").m4(p8="main")
e3.m9(p10="hello")
```

`m#` names are symbol-tuned method labels. Some methods are reads; others are writes or side effects. `plasm` dry-run should be used to inspect side-effect plans before `plasm_run`.

### Relation Traversal

Declared relations are traversed with dot syntax:

```text
repo = e2(p1="ryan-s-roberts", p2="plasm-core")
issues = repo.issues.limit(20)
issues
```

Relation traversal is valid only for relations taught by DOMAIN and present in the CGS. In programs, `label.relation` substitutes the expression bound to `label`; compute/data/derive/render bindings cannot be extended as relation anchors.

### Projection

Bracket projection narrows returned fields:

```text
e1[p2,p3]
e1{p4="open"}[p2,p3,p5]
repo.commits[p6,p7]
```

Projection lists are non-empty. DOMAIN entity headings may show a recommended projection list after `;;`; any non-empty subset of valid fields may be used when type checking accepts it.

### Paging Continuations

When a result indicates more pages, the host returns an opaque paging handle. Continue with exactly that handle:

```text
page(s0_pg1)
page(s0_pg1, limit=50)
```

Paging handles are session-scoped and opaque. Do not synthesize them from API pagination parameters.

## 6. Postfix Transforms

Postfix transforms apply left-to-right on the materialized result of the previous expression or binding.


| Transform  | Example                                      | Result                                    |
| ---------- | -------------------------------------------- | ----------------------------------------- |
| Projection | `items[p1,p2]`                               | Rows with selected fields.                |
| Limit      | `items.limit(10)`                            | First `n` rows.                           |
| Sort       | `items.sort(p3)` / `items.sort(p3, desc)`    | Rows ordered by a field.                  |
| Aggregate  | `items.aggregate(n=count, total=sum(p4))`    | Singleton aggregate row.                  |
| Group      | `items.group_by(p5, n=count, total=sum(p4))` | One row per group key.                    |
| Singleton  | `item.singleton()`                           | Runtime assertion/coercion for one row.   |
| Page size  | `items.page_size(100)`                       | Request/plan hint for upstream page size. |
| Render     | `items[p1,p2] <<TAG ... TAG`                 | Singleton row with `content`.             |


Aggregate specs name outputs explicitly: `n=count`, `total=sum(amount)`, `avg_score=avg(score)`, `lo=min(score)`, `hi=max(score)`. Bare count shorthands may be accepted for repair, but docs and prompts teach explicit output names.

Example:

```text
open = e1{p3="open"}.limit(100)
by_author = open.group_by(p6, n=count)
by_author.sort(n, desc).limit(10)
```

## 7. Programs

A program is a sequence of statements:

```text
binding = expression
binding2 = expression_or_transform
final_root, another_root
```

Rules:

- Each binding introduces one label.
- Labels are unique.
- The final statement is a comma-separated list of roots: labels or expressions.
- Bindings may refer to earlier bindings, not later ones.
- A bare final expression is allowed; the compiler creates a synthetic return node.

Example:

```text
repo = e2(p1="ryan-s-roberts", p2="plasm-core")
commits = repo.commits.sort(p5, desc).limit(20)
summary = commits[p6,p7,p8] <<RPT
{% for r in rows %}
- {{ r.p6 }}: {{ r.p7 }}
{% endfor %}
RPT
summary
```

## 8. Data Literals and Derives

Object and array literals can be bound as data:

```text
payload = {title: "Docs", labels: ["documentation"]}
payload
```

`source => value` derives a new artifact from rows in `source`. Inside the right-hand side, `_` refers to the current row:

```text
items = e1.limit(10)
cards = items => {id: _.id, title: _.title}
cards
```

Derives are artifact computations, not outbound API calls.

## 9. Row-wise Effects (`for_each`)

`source => effect_expression` runs a write/action template once per source row when the right side is a side-effecting Plasm expression:

```text
drafts = e1{p3="draft"}.limit(5)
updates = drafts => e3.m4(p7=_.id, p8="archived")
updates
```

The row cursor is `_`. Use `_.field` to pass row values into arguments. The right side type-checks as create, update, delete, or action; read-only expressions are not row-wise effects.

## 10. Typed Dynamic References

Inside programs, method and predicate RHS positions accept typed references to prior bindings, not only concrete literals:

```text
report = e1[p2,p3] <<RPT
{% for r in rows %}{{ r.p2 }}: {{ r.p3 }}{% endfor %}
RPT
sent = e3.m19(p91=report.content)
sent
```

Important distinctions:

- `p91=report` passes the materialized output of `report`.
- `p91=report.content` passes the `content` field from that output.
- Bracket render returns a singleton row shaped like `{"content": "<rendered text>"}`.
- String parameters receive `report.content`, not the whole `report` row.

For scalar materialized nodes, passing the whole binding may be correct; match the schema’s expected value type.

## 11. Tagged Heredocs

Structured strings use tagged heredocs:

```text
body = <<PLASM_MAIL_9c2e
Subject: Status

The build passed.
PLASM_MAIL_9c2e
body
```

Rules:

- The opener is `<<TAG` followed by a real newline.
- The closing line is the first body line whose trimmed content equals `TAG`.
- The parser also accepts `TAG)`, `TAG,`, or `TAG}` on the closing line when the heredoc is nested in a call/object.
- The close rule is first-match-wins; there is no “last closing tag wins” scan.
- Choose a tag that cannot appear as a trimmed line inside the payload.

Unsafe for arbitrary mail/markdown:

```text
<<EOF
EOF
```

Prefer high-entropy labels such as `PLASM_MAIL_9c2e` or `GMAIL_RAW_EOF`.

## 12. Bracket Render and Minijinja

Bracket render projects source rows and renders a Minijinja template:

```text
commits = e1.limit(5)
report = commits[p2,p3] <<REPORT
{% for r in rows %}
- {{ r.p2 }}: {{ r.p3 }}
{% endfor %}
REPORT
report
```

The only guaranteed template binding is `rows`: a JSON array of projected row objects. Output may be plain text, Markdown, HTML fragments, CSV-like text, or JSON text. If the body must contain literal `{{`, `{%`, or `{#`, wrap that passage in `{% raw %}...{% endraw %}`.

Render source columns may be explicit (`source[p1,p2] <<TAG`) or inferred from a row-producing source where inference is supported. Render outputs cannot themselves be used as render sources.

## 13. Examples by Task

### Fetch One Object

```text
repo = e2(p1="ryan-s-roberts", p2="plasm-core")
repo[p3,p4,p5]
```

### Search, Sort, and Trim

```text
hits = e1~"symbol tuning".sort(p4, desc).limit(10)
hits[p2,p4,p5]
```

### Aggregate a List

```text
items = e1{p3="open"}.limit(100)
totals = items.group_by(p6, n=count)
totals.sort(n, desc).limit(20)
```

### Render Text Then Send It

```text
rows = e1{p3="open"}.limit(20)
digest = rows[p2,p5] <<DIGEST
{% for r in rows %}
- {{ r.p2 }} — {{ r.p5 }}
{% endfor %}
DIGEST
sent = e3.m7(p9=digest.content)
sent
```

## 14. Relationship to DOMAIN Rendering

DOMAIN is not a loose hint; it is the many-shot teaching surface for the active symbol map. For each exposed entity, the renderer emits only expression shapes that parse, normalize, and type-check against the CGS before being shown.

DOMAIN renderers:

- Start with the stable language contract.
- Emit a `plasm_expr` / `Meaning` table.
- Use `e#`, `m#`, and `p#` in examples when symbol tuning is enabled.
- Put parameter/field glosses near first use.
- Include heredoc guidance only when the exposed slice contains structured strings.
- Omit duplicate DOMAIN replay on no-op session reuse.

Agents and host integrations:

- Treat DOMAIN examples as authoritative for available entities, methods, relations, fields, filters, and parameter names.
- Preserve and reuse symbols only while the logical session remains valid.
- Discard cached symbols when the host reports a new symbol space.
- Use `plasm_context` to expose additional entities instead of inventing symbols.

## 15. Implementation Notes

Parsing and lowering are owned by `plasm-core` and `plasm-agent-core`. The serialized `Plan` IR is archival and traceable, but it is not a second syntax tier. Optimizers and transport adapters preserve the semantics of the surface language described here.