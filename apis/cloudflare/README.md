# Cloudflare API (Phase 1)

Plasm CGS/CML for Cloudflare REST v4. **Phase 1** covers **zone-scoped** flows: **rulesets**, **managed phase entrypoints** (DDoS L7, managed WAF, custom rules, etc.), and **WAF packages**. Add more entities and capabilities in this directory as the surface grows.

Ground truth: Cloudflare REST API (OpenAPI). The upstream **full** spec is large and contains path patterns that **Hermit** (used by `plasm validate`) cannot load. This directory therefore keeps:


| File                                         | Purpose                                                                                                                       |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `[openapi.hermit.json](openapi.hermit.json)` | Minimal slice + `example` payloads for `plasm validate` / Hermit smoke tests.                                                 |
| Upstream                                     | `https://raw.githubusercontent.com/cloudflare/api-schemas/main/openapi.json` — refresh the slice when extending capabilities. |


Base URL: `https://api.cloudflare.com/client/v4`.

## Auth

Phase 1 uses **`Authorization: Bearer <API_TOKEN>`** (see `domain.yaml` `auth`). Global API keys are deprecated for new automation—use [API tokens](https://developers.cloudflare.com/fundamentals/api/get-started/create-token/).

### API token permissions

Grant the token access to the **zones** you need (specific zone IDs or all zones). Names below match Cloudflare’s [API token permission reference](https://developers.cloudflare.com/fundamentals/api/reference/permissions/).

| Need | Permission |
|------|------------|
| Zones list/get | **Zone → Zone → Read** |
| Zone rulesets list/get (`ruleset_query` / `ruleset_get`) | **Zone → Zone WAF → Read** (and **Edit** for writes where applicable) |
| **Managed WAF** phase entrypoints (e.g. `http_request_firewall_managed`) — `ruleset_entrypoint_get` / `ruleset_entrypoint_update` | **Zone → Zone WAF → Read** / **Zone WAF Edit** |
| **`ddos_l7`** phase entrypoint (`…/rulesets/phases/ddos_l7/entrypoint`) | **Zone → HTTP DDoS Managed Ruleset → Read** (and **Edit** for updates). **Not** covered by Zone WAF alone—a token with only WAF scopes often gets **`403` “request is not authorized”** on this path. |
| **`ddos_l4`** phase entrypoint | **Zone → L4 DDoS Managed Ruleset → Read** (or **Write** for changes)—network-layer DDoS managed ruleset, separate from HTTP DDoS and from Zone WAF. |
| `waf_package_query` | **Zone → Zone WAF → Read** |

**OAuth:** `domain.yaml` also lists `oauth.provider: cloudflare` for hosted flows that map Plasm scope ids to a Cloudflare OAuth app. That path is separate from bearer **API tokens**; for REPL/CI/agents, configure **`CLOUDFLARE_API_TOKEN`** with the table above.

```bash
export CLOUDFLARE_API_TOKEN='...'
cargo run -p plasm-cli --bin plasm -- schema validate apis/cloudflare
cargo run -p plasm-cli --bin plasm -- validate --spec apis/cloudflare/openapi.hermit.json apis/cloudflare
```

## Scope (Phase 1)

- **Zone** — list (`GET /zones`) and get (`GET /zones/{zone_id}`).
- **Ruleset** — list for a zone, get one ruleset (includes rules when the API returns them).
- **RulesetEntrypoint** — get/update the managed entrypoint for a **phase** (`…/rulesets/phases/{phase}/entrypoint`).
- **WafPackage** — list WAF packages for a zone.

Hosted MCP / browser OAuth can use the same capability graph once an outbound OAuth app is registered; API tokens remain the simplest path for CI and local REPL.