# Microsoft Teams (Microsoft Graph) — Plasm CGS

A **wave‑1** [Plasm](../../README.md) domain for **Microsoft Teams** via **Microsoft Graph** `v1.0`: teams the signed-in user has joined, plus **get team by id**. The CGS compresses the Graph RPC surface into one relational entity (`Team`) so agents reason about “teams I belong to” and “details for team X”, not raw URL trees.

Later waves (not authored here yet) should add **channels**, **tabs**, **chats**, and **messages** where they fit the same pattern: declare `entity_ref` / `key_vars` / scoped queries only when the JSON actually carries every part of the identity—or when product semantics justify a documented engine limitation.

```bash
export MICROSOFT_GRAPH_ACCESS_TOKEN="…"   # OAuth 2.0 access token for Graph (delegated user)
cargo run -p plasm-agent --bin plasm-cgs -- \
  --schema apis/microsoft-teams \
  --backend https://graph.microsoft.com \
  team query
cargo run -p plasm-agent --bin plasm-cgs -- \
  --schema apis/microsoft-teams \
  --backend https://graph.microsoft.com \
  team "<team-guid>"
```

## Auth and permissions

- **Scheme:** `bearer_token` → `Authorization: Bearer …` using `MICROSOFT_GRAPH_ACCESS_TOKEN`.
- **Delegated:** `team_query` calls `GET /me/joinedTeams` — register an Azure AD app, add **Microsoft Graph delegated** permissions such as `Team.ReadBasic.All` or broader `Team.ReadBasic.All` / `Group.Read.All` as your tenant policy allows, complete admin consent if required, then obtain a user access token (authorization code or device code flow).
- **Application-only tokens** do not have a `/me` surface; a different CGS slice would use `/teams` filters or roster APIs instead.

## Phased roadmap (relational design)

| Wave | Scope | Notes |
|------|--------|--------|
| **1 (this tree)** | `Team`: `team_query`, `team_get` | Joined teams + detail; no Graph `@odata.nextLink` loop (see below). |
| **2** | `Channel` under `Team` | Graph channel rows omit parent `teamId`; compound `key_vars: [team_id, id]` needs every list row to carry `team_id` (today: **not** auto-filled from scope in core). **Do not** fake IDs in CGS—either wait for product/runtime support or choose an API shape that includes parent keys in JSON. |
| **3** | Messages / chat | Free-text search vs filtered lists; heavy pagination and rate limits. |
| **4** | Apps, tabs, membership writes | Side-effect `action` capabilities with explicit domain `output` descriptions. |

## Known limitation (no core change in this PR)

Microsoft Graph often returns **`@odata.nextLink`** (absolute URL) for the next page. Plasm’s composable pagination today advances **query/body params** or **Link** headers, not “replay this full URL from JSON”. So **`team_query` returns the first page only** (`$top=100`). For full tenant scans, extend the **engine** deliberately in a separate change, or use a different collection API that paginates with tokens you can map to `pagination.params`.

## Validation

```bash
cargo run -p plasm-cli --bin plasm -- schema validate apis/microsoft-teams
cargo run -p plasm-agent --bin plasm-cgs -- --schema apis/microsoft-teams --backend https://graph.microsoft.com --help
cargo run -p plasm-eval -- coverage --schema apis/microsoft-teams --cases apis/microsoft-teams/eval/cases.yaml
```
