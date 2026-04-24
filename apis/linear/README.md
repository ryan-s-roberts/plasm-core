# Linear — Plasm GraphQL slice

[Linear](https://linear.app/)’s API is **GraphQL** at `https://api.linear.app/graphql`. This directory defines a **CGS + CML** slice: **List and detail reads** (issues, teams, projects, users, labels, comments, workflow states, **cycles**), **scoped** issue, workflow-state, and **cycle** lists, and **writes** (`issueCreate` / `issueUpdate` / `issueDelete`, `commentCreate`). **[COVERAGE.md](COVERAGE.md)** is the detailed capability ↔ GraphQL matrix; this README focuses on **how to run** the agent and **REPL path expressions**.

## Auth

Personal API keys use a **raw** `Authorization` header value (no `Bearer` prefix). The schema uses `**api_key_header`** so the secret is sent as-is.

```bash
export LINEAR_API_TOKEN='lin_api_…'
```

Personal API keys are sent as the raw `Authorization` header value (no `Bearer` prefix). The CGS reads `**LINEAR_API_TOKEN**` only; whitespace-only values are rejected so you do not send a blank header.

OAuth access tokens use `Bearer <token>`; for those, switch the schema to `bearer_token` and a matching env var (or a forked `domain.yaml`).

## Start the REPL

```bash
cargo run --bin plasm-agent -- \
  --schema apis/linear \
  --backend https://api.linear.app \
  --repl
```

On startup you get the same **DOMAIN** prompt as `plasm-eval` (expression grammar, entity blocks, opaque `e#` / `p#` / `m#` tokens). Handy flags:

- `**--focus Issue`** (or `Team`, `Comment`, …) — smaller prompt centered on one entity.
- `**:help**` inside the REPL — grammar recap (`:schema`, `:clear`, `:output`, `:quit`, …).
- `**:llm …**` — natural-language mode (needs `OPENROUTER_API_KEY`); not required for the examples below.

To print the full DOMAIN text without starting the REPL:

```bash
cargo run -p plasm-eval -- --schema apis/linear --print-prompt
cargo run -p plasm-eval -- --schema apis/linear --print-prompt --focus Issue
```

## REPL examples (`plasm>`)

The REPL prompt is `**plasm>**` . Each line is **one Plasm path expression** (not shell `issue query …` — that is the **CLI** shape). Substitute real UUIDs from your workspace for the placeholders below.

**Reads — lists and projection**

```text
plasm> Issue
plasm> Issue[id,identifier,title]
plasm> Team
plasm> Project
plasm> User
plasm> Label
plasm> Comment
```

**Reads — get by id**

```text
plasm> Issue(00000000-0000-0000-0000-0000000000a1)
plasm> Team(00000000-0000-0000-0000-0000000000b2)
plasm> Project(00000000-0000-0000-0000-0000000000c3)
plasm> Comment(00000000-0000-0000-0000-0000000000d4)
```

**Reads — scoped lists (preferred over “reverse from parent” for these caps)**

The DOMAIN teaches: *to list X scoped by parent Y, use `**X{param=Y(id)}`** from X’s block.*

```text
plasm> Issue{team=Team(00000000-0000-0000-0000-0000000000b2)}
plasm> WorkflowState{team=Team(00000000-0000-0000-0000-0000000000b2)}
plasm> Cycle{team=Team(00000000-0000-0000-0000-0000000000b2)}
```

**Reads — relation navigation (chain)**

```text
plasm> Issue(00000000-0000-0000-0000-0000000000a1).assignee
plasm> Issue(00000000-0000-0000-0000-0000000000a1).team
plasm> Issue(00000000-0000-0000-0000-0000000000a1).labels
plasm> Issue(00000000-0000-0000-0000-0000000000a1).cycle
plasm> Comment(00000000-0000-0000-0000-0000000000d4).issue
```

**Reads — reverse traversal (when available)**

```text
plasm> Team(00000000-0000-0000-0000-0000000000b2).^Issue
```

**Writes**

```text
plasm> Issue.create(team=Team(00000000-0000-0000-0000-0000000000b2), title=New bug from Plasm)
plasm> Issue(00000000-0000-0000-0000-0000000000a1).update(title=Renamed title)
plasm> Issue(00000000-0000-0000-0000-0000000000a1).delete
plasm> Comment.create(issue=Issue(00000000-0000-0000-0000-0000000000a1), body=LGTM)
```

Phrases with spaces are allowed in shadow args (see grammar in `:help`). Prefer quoting or careful phrasing if you hit delimiter edge cases.

## CLI equivalents (non-REPL)

The same schema drives `**plasm-agent` subcommands** (useful for scripts). Examples:

```bash
# Lists
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  issue query --limit 20
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  issue by-team-query --team <team-uuid> --limit 50
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  workflowstate workflow-state-query --team <team-uuid>
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  cycle query --team <team-uuid>

# Get
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  issue <issue-uuid>

# Writes
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  issue create --team <team-uuid> --title "New bug" --description "Details…"
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  issue <issue-uuid> update --title "Renamed"
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  issue <issue-uuid> delete
cargo run --bin plasm-agent -- --schema apis/linear --backend https://api.linear.app \
  comment create --issue <issue-uuid> --body "LGTM"
```

**Writes** hit real Linear data — use a **sandbox team** or test workspace. `**issue delete`** sends `**issueDelete**` without `permanentlyDelete` (trash, not an admin hard-delete on this path).

## Coverage (summary)


| Area              | Capabilities (id)                                                                                                                          | Notes                                                        |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------ |
| **Issue**         | `issue_query`, `issue_by_team_query`, `issue_get`, `issue_create`, `issue_update`, `issue_delete`, `issue_add_label`, `issue_remove_label` | List + get + mutations for lifecycle and label attach/detach |
| **Team**          | `team_query`, `team_get`                                                                                                                   | List + get                                                   |
| **Project**       | `project_query`, `project_get`, `project_create`                                                                                           | Read + `**projectCreate`**                                   |
| **User**          | `user_query`, `user_get`                                                                                                                   | List + get                                                   |
| **WorkflowState** | `workflow_state_query`, `workflow_state_create`                                                                                            | Scoped list + `**workflowStateCreate`**                      |
| **Cycle**         | `cycle_query`, `cycle_get`                                                                                                                 | Per-team iteration windows (`team { cycles }` + `cycle`)     |
| **Label**         | `label_query`, `label_get`, `label_create`, `label_update`                                                                                 | Read + `**issueLabelCreate`** / `**issueLabelUpdate**`       |
| **Comment**       | `comment_query`, `comment_get`, `comment_create`                                                                                           | List + get + create                                          |


**Legend and gaps:** see **[COVERAGE.md](COVERAGE.md)** for status (**done** / **partial** / **planned**) and **out-of-scope** items (webhooks, uploads, label/project delete, …).

**Eval goals:** `[eval/cases.yaml](eval/cases.yaml)` lists NL goals for `plasm-eval` and `plasm-eval coverage`. Optional `**reference_expr`** on a case is a parseable ground-truth Plasm line (used with `**--compare-derived**` / `**--covers-source merge**` in advanced workflows; default `**coverage` uses YAML `covers` only**).

```bash
cargo run -p plasm-eval -- coverage --schema apis/linear --cases apis/linear/eval/cases.yaml
# Optional: require YAML `covers` ⊇ parse-derived buckets (extra YAML tags allowed)
cargo run -p plasm-eval -- coverage --schema apis/linear --cases apis/linear/eval/cases.yaml \
  --compare-derived --compare-derived-allow-extra-claims
```

## Pagination

List mappings use `**first` / `after**`, `**pageInfo.hasNextPage**`, `**endCursor**`, with `**response_prefix**` (e.g. `[data, issues, pageInfo]`) and `**body_merge_path: [variables]**`. See [GraphQL (`transport: graphql`)](../../.cursor/skills/plasm-authoring/reference.md).

## GraphQL errors

Linear may return HTTP **200** with an `**errors`** array. Inspect the raw response body if a call fails unexpectedly.

## Tests

Offline load + CML template validation:

```bash
cargo test -p plasm-e2e --test linear_smoke
```

Optional live read (network; `**LINEAR_API_TOKEN**`):

```bash
cargo test -p plasm-e2e --test linear_live -- --ignored
```

## See also

- `[schema.graphql](schema.graphql)` — authoring excerpt (not loaded by Plasm).
- `[mappings.yaml](mappings.yaml)` — GraphQL operation strings and CML.
- `[domain.yaml](domain.yaml)` — entities, capabilities, auth.

