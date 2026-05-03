# Cloudflare API (Phase 1)

Plasm CGS/CML for Cloudflare REST v4. **Phase 1** covers **zone-scoped** flows: **rulesets**, **managed phase entrypoints** (DDoS L7, managed WAF, custom rules, etc.), and **WAF packages**. Add more entities and capabilities in this directory as the surface grows.

Ground truth: Cloudflare REST API (OpenAPI). The upstream **full** spec is large and contains path patterns that **Hermit** (used by `plasm validate`) cannot load. This directory therefore keeps:


| File                                         | Purpose                                                                                                                       |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `[openapi.hermit.json](openapi.hermit.json)` | Minimal slice + `example` payloads for `plasm validate` / Hermit smoke tests.                                                 |
| Upstream                                     | `https://raw.githubusercontent.com/cloudflare/api-schemas/main/openapi.json` — refresh the slice when extending capabilities. |


Base URL: `https://api.cloudflare.com/client/v4`.

## Auth

Create an [API token](https://developers.cloudflare.com/fundamentals/api/get-started/create-token/) with at least **Zone → Zone → Read** and **Zone → WAF → …** / **Zone → Rulesets** permissions for the operations you use.

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

OAuth 2.1 flows (e.g. hosted MCP) are out of scope for this catalog; use a static bearer token from the environment.