# How Plasm fits together

If you only remember one sentence: **Plasm models an API as a graph, compiles agent-sized programs against that graph, and runs them through concrete HTTP (or GraphQL) mappings.**

Use this page before diving into CGS/CML jargon elsewhere.

---

## Running example: “GitHub Issue”

Imagine an agent should **show issue #42** in a repo.

1. **Conceptually** there is an entity type like `Issue` with fields (`title`, `state`, …) and relations (`repository`, `assignees`).
2. **Operationally** “show” becomes an HTTP `GET` with path `/repos/{owner}/{repo}/issues/{number}`.
3. **For the agent**, you don’t hand-write URLs each time—you expose the graph as **DOMAIN** rows (`e1`, `e2`, …) and let the agent write a tiny **Plasm** expression such as `e1.title` or a short pipeline.

Once that clicks, the formal names below are just labels for those layers.

---

## CGS (Capability Graph Schema): what exists and what can be done

The **CGS** describes:

- **Entities** — Nouns in the domain (`Issue`, `Repository`, …).
- **Fields & relations** — Data on those entities and links between them.
- **Value domains** — In split `domain.yaml`, top-level **`values:`** holds **semantic slots** (wire `type` plus gloss); entity fields and capability parameters use **`value_ref`** into those rows. Splitting vs sharing keys is an authoring judgement (like entity boundaries), not something you infer from wire type alone.
- **Capabilities** — Observable actions or queries the runtime can perform (`get_issue`, `list_issues`, …), with typed inputs and outputs.

Think of CGS as *the contract the agent reasons about*. It is authored as YAML (`domain.yaml`) and loaded into the runtime.

---

## CML (Capability Mapping Language): how calls hit the wire

**CML** attaches **transport-specific templates** to each capability—typically REST paths, methods, headers, and JSON bodies (`mappings.yaml`).

- Same CGS idea (“get this issue”) can map to **REST today** and **GraphQL tomorrow** with different mapping files.
- The compiler/runtime fills templates from evaluated inputs and executes the HTTP stack.

---

## Plasm language: what agents actually write

Agents don’t POST arbitrary JSON against raw URLs (unless you bypass Plasm). They write **Plasm** programs against symbols exposed in **DOMAIN** instructions:

- **`e#`** — Entity rows (nouns in context).
- **`m#`** — Scalar metrics / counts / summaries that showed up in the session.
- **`p#`** — Plans or projections from earlier steps.

Expressions compose with pipes and postfix transforms (`.limit`, `.sort`, …). Multi-line payloads use tagged **heredocs** or bracket render—see the [Language definition](reference/plasm-language-definition.md).

---

## Runtime + host: sessions, cache, MCP

The **runtime** evaluates programs, deduplicates work with **cooperative caching**, enforces capability semantics, and records traces.

The **`plasm-mcp` host** exposes:

- **Streamable HTTP MCP** — Tools such as `discover_capabilities`, `plasm_context`, `plasm`, `plasm_run`.

Session shaping (`intent`, reuse keys) matters when agents reconnect—see [MCP session reuse](reference/mcp-session-reuse.md).

---

## Federation (multiple catalogs in one session)

One logical session can load **multiple registry entries** (different APIs). Symbols stay session-local; dispatch resolves per owning graph—there is **no merged mega-schema**. See [Incremental DOMAIN](reference/incremental-domain-prompts.md#federated-sessions-multi-catalog).

---

## Where transport requirements belong

Some transports impose extra constraints (pagination envelopes, error shapes, auth headers). That detail is essential **when designing new mappings or transports**, but it is **not** the first lesson.

If you are implementing a **new** backend binding or debugging HTTP glue, start from [CML — mappings.yaml](authoring/reference.md#cml-capability-mapping-language--mappingsyaml) and vendor examples under `apis/<name>/` in the repository; repo **`AGENTS.md`** carries additional wire notes. Everyone else can defer deep transport detail until a concrete integration fails at the wire layer.

---

## Common misconceptions

| Myth | Reality |
|------|---------|
| “DOMAIN merges APIs.” | Federation keeps distinct graphs; prompts label rows per catalog entry. |
| “YAML is runtime magic.” | YAML is authoring input; packed plugins or loaded schemas drive production binaries. |

---

## Next steps

- Hands-on: [Start here](getting-started.md)
- Operator focus: [Run the MCP appliance](appliance/onboarding.md)
- Catalog authoring: [Connect an API](authoring/index.md)
