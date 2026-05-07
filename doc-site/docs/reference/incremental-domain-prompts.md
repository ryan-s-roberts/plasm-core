# Incremental teaching-table prompts (DOMAIN waves) and reducing prompt churn

This document describes how Plasm serves the **Plasm teaching table** (many-shot, symbol-tuned **TSV** examples; internal pieces still say “DOMAIN” in code) for **HTTP execute** and **MCP execute** sessions, and why that design **reduces prompt churn** for agents and humans.

**Teaching medium:** agent-visible context is always the **TSV table** (`plasm_expr`, one tab, `Meaning`), optionally prefixed by `#` comment contract lines and wrapped in a markdown fence by HTTP/MCP hosts. The legacy compact markdown transcript (`;;`-style blocks) is not emitted on the wire.

## Goals

1. **Less redundant context** — Avoid sending the full DOMAIN wall on every tool turn when the session’s catalog entry and seeds have not changed.
2. **Incremental graph exposure** — Treat the CGS as a graph: ship DOMAIN **in waves** as more entity types are needed, instead of always expanding to a large 2-hop neighbourhood in the first message.
3. **Stable symbolic indices** — Keep `e#` / `m#` / `p#` assignments **monotonic**: once assigned in a session, a symbol does not change meaning when new entities or capabilities enter the slice.
4. **Aligned expand + DOMAIN** — Expression pre-parse expansion (`expand_*`) must use the **same** symbol map as the DOMAIN text the model saw, so `e1.m3(...)` expands consistently after each wave.

“**Prompt churn**” here means: **repeated or oversized DOMAIN text** in agent context (duplicate full prompts on session reopen, multi-megabyte tables when only a small neighbourhood is needed, or shifting `m#` indices between waves). Those waste tokens, confuse models, and break trust in symbolic examples.

## Problem (before this design)

- **Full dump** — Rendering DOMAIN for the union of 2-hop neighbourhoods around seeds produced large prompts even when the task only needed a few entity types.
- **Repeat sends** — MCP `plasm_context` (open path) could return the entire DOMAIN again when the server **reused** an existing session (`reused: true`), unless the client omitted the body (we now omit the DOMAIN block on reuse).
- **Index drift** — A naïve rebuild of `SymbolMap` from a growing entity set can **re-sort** method keys globally, which would reshuffle `m#` values between waves. Incremental sessions instead **append** new `(domain, kebab)` and identifier bindings.

## Design overview

### `FocusSpec::SeedsExact`

DOMAIN slicing can use an **exact** entity list (no automatic 2-hop union). That list is the first **wave** of exposure: only those entity blocks appear in the initial DOMAIN string.

Implementation: [`FocusSpec::SeedsExact`](plasm-oss/crates/plasm-core/src/symbol_tuning.rs) and [`entity_slices_for_render`](plasm-oss/crates/plasm-core/src/symbol_tuning.rs).

### `DomainExposureSession`

A session-scoped structure in **plasm-core** ([`DomainExposureSession`](plasm-oss/crates/plasm-core/src/symbol_tuning.rs)) allocates:

- **`e#`** — Order of **first exposure** of each entity name.
- **`m#`** — New `(domain, kebab)` capability pairs, sorted **only among newly added** pairs, then assigned the next free `m` indices.
- **`p#`** — New identifier names (fields, relations, capability params) visible in the cumulative slice, sorted among **new** names only, then assigned the next free `p` indices.

Existing assignments are never rewritten. Rendering uses [`render_domain_prompt_bundle_for_exposure`](plasm-oss/crates/plasm-core/src/prompt_render.rs); later waves pass `emit_entity_blocks` so only **new** entity blocks are appended (and the main “Valid expressions” preamble is omitted on those waves).

### DOMAIN exemplar anchors (CGS binding surface)

Whether DOMAIN text should include an **entity anchor** exemplar (for example `Entity($)` / symbolic `e#` usage) must not be decided in the prompt layer by naming a transport (for example “GraphQL”). **plasm-core** exposes transport-neutral predicates on the capability’s mapping template:

- [`template_domain_exemplar_requires_entity_anchor`](plasm-oss/crates/plasm-core/src/schema.rs) — true when the template needs an anchor for DOMAIN examples: HTTP **path** template variables **or** a GraphQL operation **`variables`** block that binds an `id` (or equivalent single-entity key).
- [`template_invoke_requires_explicit_anchor_id`](plasm-oss/crates/plasm-core/src/schema.rs) — used for expression pre-parse / shadow-invoke rules when an explicit anchor id is required (path vars **or** any GraphQL operation variable list), matching the compile path’s expectations.

[`CapabilitySchema::domain_exemplar_requires_entity_anchor`](plasm-oss/crates/plasm-core/src/schema.rs) and [`invoke_requires_explicit_anchor_id`](plasm-oss/crates/plasm-core/src/schema.rs) delegate to those helpers. DOMAIN rendering consults the schema-level predicate (for example via `path_vars_empty` in [`prompt_render`](plasm-oss/crates/plasm-core/src/prompt_render.rs)) so **prompt synthesis stays free of GraphQL-specific conditionals**.

When the cumulative slice includes structured string semantics, the preamble adds **`<<TAG`** heredoc rules (see [`DOMAIN_RICH_STRING_GUIDANCE_SENTINEL`](plasm-oss/crates/plasm-core/src/prompt_render.rs) and fenced examples via [`DOMAIN_RICH_STRING_GUIDANCE_FENCED_SENTINEL`](plasm-oss/crates/plasm-core/src/prompt_render.rs)): copy-pastable fenced `text` blocks show tagged form only. The only multiline/raw string form in path expressions is bash-inspired `<<TAG` + newline + body + closing line (trimmed `TAG`), with the same close optionally glued before `)` / `,` / `}`.

**Grammar note:** The opener is **`<<`** (two characters) plus a tag, not `<<<`. Legacy `d<<<` is removed—use **`<<TAG`** only (never `<<` + newline alone).

### Execute session state (plasm-agent)

[`ExecuteSession`](plasm-oss/crates/plasm-agent-core/src/execute_session.rs) holds:

- `prompt_text` — Cumulative DOMAIN text (wave 1 + optional `## Expanded capabilities` sections).
- `domain_exposure` — The [`DomainExposureSession`] used for both DOMAIN rendering and [`expand_expr_for_domain_session`](plasm-oss/crates/plasm-core/src/symbol_tuning.rs) (via [`expand_expr_for_session_with_optional_exposure`](plasm-oss/crates/plasm-core/src/prompt_pipeline.rs)).
- `domain_revision` — Increments each time more entities are exposed.

Session identity (`prompt_hash`, `session` id) stays stable across waves; the hash is still derived from the **initial** prompt text for routing (see agent code paths).

## MCP tools

- `plasm_context`: **Call first** on each MCP connection. Pass **`intent`** (host-chosen, stable for the same agent context — see [`docs/mcp-session-reuse.md`](mcp-session-reuse.md)) and required `seeds` array of `{ api, entity }`. The server returns **`logical_session_ref`** (`s0`, `s1`, … — a per-connection slot, like artifact index `r/{n}`) for subsequent `plasm` calls; canonical UUID + trace identity are server-side (see [`docs/mcp-logical-sessions.md`](mcp-logical-sessions.md)).
- On a **fresh open** (no live execute binding for that logical id), the **primary** `api` is the **lexicographically first** distinct catalog id among seeds — this keeps [`SessionReuseKey`](plasm-oss/crates/plasm-agent-core/src/execute_session.rs) stable if the host reorders an equivalent seed set. **Secondary catalogs** in the same call are federated/expanded in lexicographic `api` order (after the primary), so multi-API open order does not depend on seed list order. Tool output returns **delta-only** DOMAIN waves (no full prompt replay on federate/expand), while session symbol maps stay append-only. **MCP** `_meta.plasm.continuity` always includes `stale_binding_recovered` and `new_symbol_space` (and `discard_cached_plasm_symbols` when `new_symbol_space` is true) — when that flag is set, **discard** any prior `e#`/`m#`/`p#` cached in the agent. Tenant MCP config scopes allowed APIs; a disallowed API fails the whole call. The TSV/DOMAIN contract teaches **named** `p#=…` / `name=…` slots for creates/updates; do not infer field meaning by permuting `p#` numerically after a new wave.
- `plasm`: Pass **`logical_session_ref`** and **`program`**. Runs Plasm lines using the session’s exposure map when present. Paginated lists: follow **`page(s0_pgN)`** (and `_meta.plasm.paging`) — the slot must match your `logical_session_ref`.

Cardinality: **many** logical sessions per MCP **transport** (`MCP-Session-Id`); **one** active Plasm execute binding per **logical session** (see [`mcp_server.rs`](plasm-oss/crates/plasm-agent-core/src/mcp_server.rs) module docs).

## Federated sessions (multi-catalog)

A single execute session (`prompt_hash` + `session`) can expose entities from **more than one** registry row (`entry_id`) **without** merging their [`CGS`](plasm-oss/crates/plasm-core/src/schema.rs) graphs into one artifact.

- **Prompt / symbols** — [`DomainExposureSession`](plasm-oss/crates/plasm-core/src/symbol_tuning.rs) tracks which catalog each exposed entity name belongs to; DOMAIN rendering and the symbol map stay **append-only** (`e#` / `m#` / `p#` monotonic **within that session**). Headings and tables can reflect **(registry entry, entity)** so the model knows which API each block refers to.
- **Execution** — The agent keeps one [`CgsContext`](plasm-oss/crates/plasm-core/src/cgs_context.rs) per `entry_id` (backend URL, auth, and its own `CGS`). [`FederationDispatch`](plasm-oss/crates/plasm-core/src/cgs_federation.rs) maps exposed entity names to the owning context; the runtime selects HTTP origin (and typecheck graph) **per operation**, not a single merged schema.
- **MCP** — If an execute binding already exists and `seeds` include an `entry_id` not yet in the session, the server federates that catalog into the same session (additional DOMAIN wave, same binding). Seeds for already-loaded entries produce expand waves.
- **HTTP** — Primary flow is still `POST /execute` with one `entry_id`; extending with a second catalog may use the same federate path as MCP where implemented (see [`http_execute.rs`](plasm-oss/crates/plasm-agent-core/src/http_execute.rs)).

**Not in scope:** global merge semantics for **colliding** entity names across catalogs — prompts are symbolic and **(catalog, entity)** disambiguates; sessions do not rely on a structural union of CGS.

## HTTP parity

`POST /execute` creates sessions the same way (incremental first wave + stored `domain_exposure`). There is no separate HTTP route for expansion in the minimal design; MCP `plasm_context` invokes the same expand/federate paths server-side.

## MCP: who orders discover vs execute?

The **host agent** (e.g. Cursor) decides **which tool to call and when**. The server surfaces **`plasm_context` first** in tool order and **`initialize` instructions** requiring it before other Plasm tools; it cannot fully enforce ordering. If the model skips search, you may see **only** `plasm_agent::http_execute` “execute expression” lines in logs — that means the client went straight to execute after (or without) a `plasm_context` open that might have happened in an earlier turn or session.

**Observability:** at `INFO`, `plasm_agent::mcp` logs **`discover_capabilities`**, **`plasm_context`**, **`plasm`**, and **`list_registry`** when those tools run, so a healthy flow shows **search → plasm_context → plasm** explicitly. Filter with `RUST_LOG=plasm_agent::mcp=info` (or `info` for the whole crate) to confirm.

## Related code

- CGS template binding helpers (DOMAIN anchor / invoke id): [`plasm-oss/crates/plasm-core/src/schema.rs`](plasm-oss/crates/plasm-core/src/schema.rs) (`template_domain_exemplar_requires_entity_anchor`, `template_invoke_requires_explicit_anchor_id`)
- Federation dispatch (multi-context, no CGS merge): [`plasm-oss/crates/plasm-core/src/cgs_federation.rs`](plasm-oss/crates/plasm-core/src/cgs_federation.rs)
- Symbol tuning and exposure: [`plasm-oss/crates/plasm-core/src/symbol_tuning.rs`](plasm-oss/crates/plasm-core/src/symbol_tuning.rs)
- DOMAIN rendering: [`plasm-oss/crates/plasm-core/src/prompt_render.rs`](plasm-oss/crates/plasm-core/src/prompt_render.rs)
- Prompt pipeline: [`plasm-oss/crates/plasm-core/src/prompt_pipeline.rs`](plasm-oss/crates/plasm-core/src/prompt_pipeline.rs)
- HTTP + expand: [`plasm-oss/crates/plasm-agent-core/src/http_execute.rs`](plasm-oss/crates/plasm-agent-core/src/http_execute.rs)
- MCP: [`plasm-oss/crates/plasm-agent-core/src/mcp_server.rs`](plasm-oss/crates/plasm-agent-core/src/mcp_server.rs)

## Summary

**Prompt churn** is reduced by (1) **exact** first-wave DOMAIN size, (2) **append-only** waves via `plasm_context` seed deltas, (3) **no duplicate DOMAIN** on reused opens, and (4) **monotonic** `e#`/`m#`/`p#` so earlier examples remain valid as the session grows. **Federation** adds (5) **multi-catalog** sessions without merging CGS — same monotonic symbol stream, dispatch per [`CgsContext`](plasm-oss/crates/plasm-core/src/cgs_context.rs).
