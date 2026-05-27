# Linear — task-oriented Plasm catalog

[Linear](https://linear.app/) GraphQL at `https://api.linear.app/graphql`. This catalog is **name-centric** (issue identifiers like `ENG-42`, team keys like `ENG`, state/label names in search) — aligned with [Linear #1035](https://github.com/linear/linear/issues/1035) task-shaped agent surfaces, not a mirror of every GraphQL operation.

**Vendor schema:** run introspection — do not hand-author SDL.

```bash
export LINEAR_API_TOKEN='lin_api_…'
apis/linear/scripts/refresh_schema.sh
```

## Auth

Personal API keys use a **raw** `Authorization` header (no `Bearer` prefix).

```bash
export LINEAR_API_TOKEN='lin_api_…'
```

## REPL

```bash
cargo run --bin plasm -- \
  --schema apis/linear \
  --backend https://api.linear.app \
  --repl
```

## Task expressions (examples)

**Search / filter**

```text
plasm> Issue.search(team_key=ENG, label_name=bug, q=auth)
plasm> Issue.search(assignee_name=Jane Doe, state_name=In Progress)
plasm> Project.search(q=Mobile)
plasm> Document.search(q=spec)
```

**Get by human identifier**

```text
plasm> Issue(ENG-42)
plasm> Team(ENG)
```

**Composed reads (#1035-style)**

```text
plasm> IssueContext(ENG-42)
plasm> MyWorkSnapshot
plasm> ProjectContext(<project-uuid>)
plasm> IssueNavigationLink(ENG-42)
plasm> CycleBoardSnapshot(<cycle-uuid>)
```

**Writes**

```text
plasm> Issue.create(team=Team(ENG), title=New bug from Plasm)
plasm> Issue(ENG-42).update(title=Renamed, state_name=In Progress)
plasm> Issue(ENG-42).delete
plasm> Comment.create(issue=Issue(ENG-42), body=LGTM)
plasm> Issue(ENG-42).comments
```

**Pagination:** list/search capabilities paginate via `page(pg#)` in execute sessions after the first wave.

## Coverage

See [COVERAGE.md](COVERAGE.md). Eval goals: [eval/cases.yaml](eval/cases.yaml).

```bash
cargo run -p plasm-eval -- coverage --schema apis/linear --cases apis/linear/eval/cases.yaml
```

## Tests

```bash
cargo test -p plasm-e2e --test linear_smoke
cargo test -p plasm-e2e --test linear_live -- --ignored   # needs LINEAR_API_TOKEN
```
