# Linear — task-oriented coverage

Agent tasks map to Plasm symbols (not GraphQL operation names). Wire details live in `mappings.yaml`; refresh vendor SDL via `scripts/refresh_schema.sh`.

Legend: **done** | **partial** | **planned** | **n/a**

## Task → capability map

| Agent task | Plasm surface | Status |
|------------|---------------|--------|
| Find / filter issues | `issue_search` (`kind: search`, name filters) | done |
| Get issue by identifier | `issue_get` (`id_field: identifier`) | done |
| Issue + comments context | `IssueContext` view | done |
| My assigned work | `MyWorkSnapshot` view | done |
| Team lookup by key | `team_get` (`id_field: key`) | done |
| Cycles for team | `cycle_query` (`team_key` scope) | done |
| Workflow columns | `workflow_state_query` (`team_key`) | done |
| Create / update / trash issue | `issue_create`, `issue_update`, `issue_delete` | done |
| Comment on issue | `comment_create` (`issue` → Issue) | done |
| List issue comments | `comment_by_issue_query`, `Issue.comments` | done |
| Project + status updates | `ProjectContext` view, `project_update_*` | done |
| Documents | `document_search`, `document_get`, `document_update` | partial |
| Initiatives | `initiative_query`, `initiative_get`, `initiative_update` | partial |
| Milestones | `project_milestone_*` | partial |
| Share issue URL | `IssueNavigationLink` view | done |
| Sprint board | `CycleBoardSnapshot` view | done |

## Name-centric parameters (`issue_search`)

| Parameter | Role |
|-----------|------|
| `q` | Title contains |
| `team_key` | Team short key (e.g. ENG) |
| `state_name` | Workflow column name |
| `label_name` | Label name (filter wiring may need vendor filter shape tuning) |
| `assignee_name` | User display name |
| `priority` | 0–4 |
| `cycle` | Cycle entity ref (iteration filter) |

## Writes — `preflight` (press)

`issue_create` and `issue_update` declare runtime **`preflight`** steps:

- **`team`** (`entity_ref`) → `team_get` → `teamId`
- **`state_name`** → `workflow_state_query` + `query_pick` → `stateId` (first page only; ambiguous names fail)
- **`assignee_name`** → `user_search` + `query_pick` → `assigneeId`
- **`project_name`** → `project_search` + `query_pick` → `projectId`
- **`add_label_name` / `remove_label_name`** (update only) → `label_ids_delta` → `labelIds`

**Comments:** `comment_create` and `comment_by_issue_query` both take `issue` (`entity_ref` → Issue, human identifier). `Issue.comments` materializes the scoped query.

## Removed (merged from GraphQL-shaped slice)

- `issue_query`, `issue_by_team_query`, `issue_by_cycle_query` → `issue_search`
- `issue_add_label`, `issue_remove_label` → `issue_update` (`add_label_name` / `remove_label_name` via `label_ids_delta`)
- Admin-only: `label_create`, `workflow_state_create` (out of agent task surface)

## Eval

```bash
cargo run -p plasm-eval -- coverage --schema apis/linear --cases apis/linear/eval/cases.yaml
```
