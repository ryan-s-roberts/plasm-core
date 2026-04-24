You are the planning cell for a classified program (codename: **CHROMATIC-HELIX**) that combines
work-management execution (Linear) with specimen and tactical intel from the **PokéAPI** catalog in
this execute session (same registry as Linear—no out-of-band HTTP).

You are responsible for a credible **Linear-side** program plan—not only reading what already exists,
but establishing whatever issues, structure, and labels the scenario needs, subject to what the tools
can actually do and return. Do not invent facts; everything substantive must trace to tool output.

**Phase 1 — Catalog:** Orient in the combined catalog (`list_registry`, discovery, add capabilities as
needed) so you know which domains you are working with and how execute sessions are scoped per entry.

**Phase 2 — Linear program management:** Make the “mad scientist field lab / capture ops” program look
real in Linear—missions, milestones, blockers, ethics review, capture readiness, or whatever fits the
tone. Prefer a **hierarchical** story: use **parent/child issues** (exposed on `Issue` via Plasm), **labels** that encode
mission type (lab, field, containment, analysis), and **clear descriptions**. **Stamp kinetic board motion**, not commentary alone: use **`workflow_state_query`** (scoped by team) to list column ids, then **`issue_update(state=WorkflowState(…))`** to move at least several issues across workflow columns so progress is visible on the board; create/update responses include **`state`**, so you can evidence column moves from tool output. Add comments for handoffs as needed. If you cannot create or change work items, say so from what the tools
reported and proceed with what is verifiable.

Head office has complained about shallow tickets. They want **task-level detail**, **descriptions**,
and **visible progress** (states, comments, or updates you can evidence). A coherent multi-team plan
is required. The narrative should feel like a serious (if melodramatic) **Pokémon mission planning**
operation: target species, habitats, risk tiers, and “capture doctrine”—but **ground every claim** in
PokéAPI rows returned by Plasm for this session or explicit “not observed” notes.

You are responsible for credible stories only reading what
already exists, but establishing whatever state each scenario needs. Using structured content for comments and issues. 

Use **Markdown** in Linear **issue descriptions** and in **comments** (headings, lists, bold, code
fences for snippets, checklists where useful). Comments should read like real handoffs—status,
blockers, next steps—not one-line stubs. If writes are limited, say what the tools reported and still
surface a clear Linear slice from reads.

**Phase 3 — Specimen & intel (PokéAPI catalog):** Pull concrete species, types, abilities, or encounter context
that plausibly supports the Linear missions. Use only Plasm-executed PokéAPI results for facts; do not fabricate stats or
locations.

**Phase 4 — Cross-link narrative:** In one tight section, map a few Linear work items to PokéAPI-backed
targets (by name/id from tool output). Separate tool-grounded claims from color commentary.

**End with:** (a) what you could support with tool evidence, (b) what stayed unclear or blocked.
