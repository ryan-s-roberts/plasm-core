# Linear GraphQL — API coverage checklist

Track Plasm **capabilities** (CGS `domain.yaml` + CML `mappings.yaml`) vs Linear’s **GraphQL** API. Pair with **[README.md](README.md)** for REPL/CLI examples and **[eval/cases.yaml](eval/cases.yaml)** for NL eval goals.

Legend: **done** — mapped and validated in smoke tests | **partial** — subset of upstream surface | **planned** — not yet in this slice | **n/a** — intentionally out of scope

---

## Plasm capabilities (full inventory)

Each row is one **`capabilities.<id>`** in `domain.yaml` with a matching **`mappings.yaml`** entry.

| Capability | Kind | Entity | Linear GraphQL (representative) | Status |
|------------|------|--------|----------------------------------|--------|
| `issue_query` | query | Issue | `issues(first, after) { nodes { … parent { id, identifier, title } }, pageInfo }` | done |
| `issue_by_team_query` | query | Issue | `issues(..., filter: { team: { id: { eq } } })` (same parent selection) | done |
| `issue_get` | get | Issue | `issue(id) { … cycle { id, number, startsAt, endsAt, team { … } } … parent { … } children(…) … }` | done |
| `issue_create` | create | Issue | `issueCreate` → `issue { … state … cycle { … } parent { … } … }` (optional `stateId` / `parentId` / `cycleId` in input) | partial |
| `issue_update` | update | Issue | `issueUpdate` → `issue { … state … cycle { … } parent { … } … }` (optional `cycleId` in input) | partial |
| `issue_delete` | delete | Issue | `issueDelete(id)` (trash; no `permanentlyDelete` flag in Plasm CLI path) | partial |
| `issue_add_label` | update | Issue | `issueUpdate` with `addedLabelIds` | done |
| `issue_remove_label` | update | Issue | `issueUpdate` with `removedLabelIds` | done |
| `team_query` | query | Team | `teams(first, after)` | done |
| `team_get` | get | Team | `team(id)` | done |
| `project_query` | query | Project | `projects(first, after)` | done |
| `project_get` | get | Project | `project(id)` | done |
| `project_create` | create | Project | `projectCreate(input: ProjectCreateInput!)` | partial |
| `user_query` | query | User | `users(first, after)` | done |
| `user_get` | get | User | `user(id)` | done |
| `workflow_state_query` | query | WorkflowState | `workflowStates(..., filter: { team: { id: { eq } } })` | done |
| `workflow_state_create` | create | WorkflowState | `workflowStateCreate(input: WorkflowStateCreateInput!)` | partial |
| `cycle_query` | query | Cycle | `team(id) { cycles(first, after) { nodes { … team { … } }, pageInfo } }` | done |
| `cycle_get` | get | Cycle | `cycle(id) { … team { … } }` | done |
| `label_query` | query | Label | `issueLabels(first, after)` | done |
| `label_get` | get | Label | `issueLabel(id)` | done |
| `label_create` | create | Label | `issueLabelCreate(input: IssueLabelCreateInput!)` | partial |
| `label_update` | update | Label | `issueLabelUpdate(id, input: IssueLabelUpdateInput!)` | partial |
| `comment_query` | query | Comment | `comments(first, after)` | done |
| `comment_get` | get | Comment | `comment(id)` | done |
| `comment_create` | create | Comment | `commentCreate(input: CommentCreateInput!)` | done |

**Partial (mutations):** CML templates use `if exists` / `else null` for optional GraphQL inputs; **`eval_cml` omits `Value::Null` keys** in `type: object` bodies, and the **HTTP client still strips** any remaining JSON `null` entries before POST—omitted optional Plasm args do **not** appear as `"field": null` on the wire (partial `IssueUpdateInput`). See [reference.md](../../.cursor/skills/plasm-authoring/reference.md) (CML object + HTTP JSON body notes).

**Children on read:** `issue_get` uses `children(first: 250, includeArchived: true)` so sub-issues stay visible if archived; deeper pagination is not wired in this slice.

---

## Query root (by Linear field)

| Linear field / area | Plasm capabilities | Status |
|--------------------|--------------------|--------|
| **Workflow / board motion** | List columns via `workflow_state_query(team)`, move cards via `issue_update(state=WorkflowState(…))` (returns updated `state` in the mutation response) | done |
| `issues` | `issue_query`, `issue_by_team_query` | done |
| `issue` | `issue_get` | done |
| `teams` / `team` | `team_query`, `team_get` | done |
| `projects` / `project` | `project_query`, `project_get` | done |
| `users` / `user` | `user_query`, `user_get` | done |
| `workflowStates` | `workflow_state_query` | done |
| `team` → `cycles` | `cycle_query` (via `team(id).cycles`) | done |
| `cycle` | `cycle_get` | done |
| `issueLabels` / `issueLabel` | `label_query`, `label_get` | done |
| `comments` / `comment` | `comment_query`, `comment_get` | done |

**Not in slice (examples):** `initiatives`, `customViews`, search helpers, notification settings, OAuth-only fields, importer jobs, root-level `cycles(filter: …)` (this slice lists cycles per team via `team { cycles }`), etc.

---

## Mutation root

| Linear mutation area | Plasm capabilities | Status |
|---------------------|--------------------|--------|
| Issue lifecycle | `issue_create`, `issue_update`, `issue_delete`, `issue_add_label`, `issue_remove_label` (cycle assignment via `cycleId` on create/update when provided) | partial |
| Comments | `comment_create` | done |
| Labels | `label_create`, `label_update` | partial |
| Projects | `project_create` | partial |
| Workflow | `workflow_state_create` | partial |
| Label / project delete, `issueLabelDelete`, `projectDelete`, … | — | planned |
| Reactions, attachments, subscriptions | — | planned |
| `issueDelete(permanentlyDelete: true)` | — | planned (needs CLI/delete input plumbing) |

---

## Out of scope (document only)

- Inbound **webhooks** (not GraphQL execute).
- **File uploads** (may need transport / multipart work).
- **Real-time** subscriptions.

---

## Eval alignment

NL goals and coverage buckets live in **`eval/cases.yaml`**. Run:

```bash
cargo run -p plasm-eval -- coverage --schema apis/linear --cases apis/linear/eval/cases.yaml
```

Some cases include optional **`reference_expr`** (a valid one-line Plasm expression) for tooling that compares parse-derived **`covers`** to YAML; default coverage uses YAML **`covers`** only.
