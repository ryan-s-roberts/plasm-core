# Grafana catalog scorecard

**catalog:** `apis/grafana`  
**domain version:** 5  
**scored:** 2026-05-23  
**evidence:** `plasm-cgs schema validate`, `plasm-eval coverage` (26 cases), LGTM REPL smoke (deeplink `url` with `now-1h`/`now`)

| Dim | Score | Justification |
|-----|-------|----------------|
| A Semantic compression | 3 | Entity-oriented model vs ~70 MCP tools; some plugin RPC paths remain path-shaped |
| B Typed values | 3 | Strong `entity_ref` / `select`; time range scope still plain string + `wire_time` in view |
| C Relation utility | 3 | `dashboard_summary.dashboard`; many areas list-only |
| D Action outputs | 4 | `discovery_data` / `query_results` split; side effects named concretely |
| E Mappings completeness | 3 | Validates; plugin/RBAC routes environment-dependent |
| F Views usage | 4 | `dashboard_summary` + `deeplink_generate` with computed assembled `url` |
| G Transport evidence | 3 | Tier 0 validate; Tier 2 LGTM smoke (core, explorers, deeplink); plugins 404 without apps |
| H Eval coverage | 4 | 26 cases including adversarial Explore deeplink (`gf-26`); coverage passes |
| I Description hygiene | 3 | Domain-first; a few long capability descriptions remain |
| J README quality | 4 | v5 scope, auth, limitations, eval commands |

**total:** 34/40  
**band:** B (ship-ready)

## Top improvement targets

1. **G → Tier 3:** Record sandbox write evidence for mutating caps (folder create, role assign) when a stable test tenant exists.
2. **B → 4:** Promote `from`/`to` deeplink scope to `temporal` value slots where teaching-table alignment allows.
3. **G/H:** Panel render PNG bytes and LLM `plasm-eval` run with a pinned model for regression tracking.
