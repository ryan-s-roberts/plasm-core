# Composed read views

**Views** in `domain.yaml` declare **composed read-only** capability DAGs: multi-step reads that bind scope, fan out over nodes, and project structured outputs — without exposing raw HTTP choreography to agents.

!!! note "Not tenant registry views"
    **CGS views** are catalog authoring constructs. They are unrelated to MCP tenant filtering or control-plane listing APIs that happen to use the word “views.”

---

## When to use views

| Situation | Prefer |
|-----------|--------|
| Single list or get | Plain **`query`** / **`get`** capabilities |
| Dashboard row combining counts, joins, scoped fetches | **`views:`** entry |
| Mail/work-tracker “inbox snapshot” patterns | **`views:`** with **`scope`** + **`nodes`** |
| User-defined DB columns at session open | **`schema_overlay:`** (see [Schema overlays](../reference/schema-overlay.md)) |

---

## Minimal example

```yaml
views:
  TeamSnapshot:
    scope:
      team_id: { kind: scope, required: true }
    nodes:
      open_issues:
        capability: list_issues
        bind:
          team: { kind: scope, key: team_id }
    output:
      open_count:
        kind: count
        from: open_issues
```

Matching **`mappings.yaml`** stub:

```yaml
TeamSnapshot:
  transport: view
```

Deep grammar (bind kinds, `relation_outputs`, computed fields): [Authoring reference — Composed read views](reference.md#composed-read-views).

---

## Computed templates

Node and output fields may use **`kind: computed`** with Minijinja templates evaluated at compile or wire time. Common filters:

| Filter | Role |
|--------|------|
| `urlencode` | Query/path encoding |
| `json_encode` | JSON text for wire bodies |
| `wire_time` | Pass-through temporal strings (`now`, `now-1h`) |
| `wire_query_suffix` | Append query fragments |

Full filter table and pitfalls: [View computed templates](reference.md#view-computed-templates-filters-and-time).

---

## Schema overlays (related)

When workspace **columns or properties** are defined at runtime (Notion, Fibery, Jira custom fields), pair a bootstrap entity with **`schema_overlay:`** instead of exploding static `customfield_*` keys. Authoring: [Runtime schema overlay](reference.md#runtime-schema-overlay-schema_overlay). Runtime: [Schema overlays](../reference/schema-overlay.md).

---

## Regression fixtures

Language conformance: `fixtures/schemas/plasm_language_matrix_views/` in the plasm-core repository. Extend matrix fixtures — not production `apis/*` — when locking view behavior in tests.
