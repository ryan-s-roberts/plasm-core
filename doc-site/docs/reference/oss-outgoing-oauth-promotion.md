# Outgoing OAuth: promotion path from SaaS to core

**Goal:** **Outgoing** OAuth (third-party API credentials, OAuth link flow, refresh) is a **core single-user** capability. **Incoming** browser OAuth (GitHub login, tenant identity) stays **SaaS enterprise UI + control plane** only.

**Do not conflate** auth planes: outbound provider auth ≠ MCP transport keys ≠ execute JWT identity (see [Incoming auth](plasm-mcp-incoming-auth.md) and [Appliance persistence](oss-appliance-mcp-persistence.md)).

## Current state


| Concern                                          | Location                                                                                                                                                              |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OAuth token/link **logic**                       | `plasm-runtime` (`hosted_oauth_kv`, `oauth_client`), `plasm-agent-core` (`oauth_link_catalog`, `oauth_link_session`, `outbound_secret_provider`)                      |
| Link + outbound-secret **HTTP** (hosted product) | Implemented in the composed hosted binary (outside this OSS tree).                                                                                                    |
| Control-plane contract                           | Automation uses `/internal/oauth-link/v1/`* and `/internal/outbound-secrets/v1/*` with the deployment control-plane secret (not documented on this site).             |
| Hosted UI                                        | **Plasm Cloud** ([platform.plasm.tools](https://platform.plasm.tools)) provides OAuth provider registration and outbound connection flows that target those surfaces. |
| Pure OSS binary                                  | `PlasmHostState.saas == None` — no `/internal/`*; outbound today is limited unless extended                                                                           |


## Target seam

1. **Mount outgoing OAuth HTTP on OSS data plane** — Move or duplicate the **public** link/callback and secret write surfaces from `plasm-saas` into `plasm-agent-core` HTTP (`http.rs` / dedicated module), gated for **single-user / local** use (e.g. localhost, shared local secret, or file-based allowlist — exact policy is a follow-on implementation task).
2. **Keep SaaS control-plane variants** — Hosted deployments may keep `/internal/`* for Phoenix automation; contract doc remains the integrator surface.
3. **Fence incoming OAuth** — GitHub login, `IncomingAuthController`, `http_incoming_tenant`, and workspace binding remain in `web/` + `plasm-saas` only.
4. **Phoenix as composer** — Ops/project UIs continue to **call** core or `/internal/`* to upsert providers and secrets; they do not own the OAuth protocol implementation.

## Migration notes

- Shared header auth for `/internal/*` lives in `plasm-agent-core` `[control_plane_http.rs](https://github.com/ryan-s-roberts/plasm-core/blob/main/crates/plasm-agent-core/src/control_plane_http.rs)`; OSS-mounted outgoing routes need a **different** gate.
- `PlasmSaaSHostExtension` currently wires `oauth_link_catalog` and `outbound_secret_provider`; pure OSS may need the same types attached to `PlasmOssHostState` or a minimal extension without tenant MCP — see `server_state.rs` in `plasm-agent-core`.