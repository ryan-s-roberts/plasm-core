# Connect an API

**Goal:** Ship a **typed catalog** agents can target: one `**domain.yaml`** (what exists / what can be done) and one `**mappings.yaml**` (how each capability hits HTTP or GraphQL).

This page is the **short tutorial**. Doctrine, edge cases, pagination tables, and naming rules live in **[Authoring reference](reference.md)**—open that when you are editing YAML daily.

---

## What you author


| Artifact                  | Role                                                                                                 |
| ------------------------- | ---------------------------------------------------------------------------------------------------- |
| `**domain.yaml` (CGS)**   | Entities, fields, relations, capability declarations—the **semantic contract** agents see as DOMAIN. |
| `**mappings.yaml` (CML)** | Per-capability templates—paths, methods, bodies, pagination hooks.                                   |


There is **no third YAML** for “runtime query semantics”: list behavior comes from CML `**pagination`** blocks; hydration defaults apply when CGS declares both `**query**` and `**get**` for an entity unless callers opt out (see reference).

---

## Mental model: API → graph → capabilities

1. **Entities** are nouns stable enough to teach (`Issue`, `Repository`, …).
2. **Fields / relations** mirror payloads and joins you want agents to chain.
3. **Capabilities** are the verbs (`list_issues`, `get_issue`, …) with typed inputs/outputs.

You are **not** transcribing every OpenAPI path 1:1. You **merge** endpoints where they express the same capability, **split** where semantics diverge, and **name** for prompt clarity—not for RPC nostalgia.

---

## Five-step loop

```
READ spec  →  AUTHOR domain.yaml  →  AUTHOR mappings.yaml  →  VALIDATE  →  TEST
     ↑                                                           │
     └───────────────────────────────────────────────────────────┘
```

1. **Read** the vendor spec (paths, schemas, auth, pagination, error envelopes).
2. **Author** `domain.yaml`: entities, relations, capabilities that match how agents should think.
3. **Author** `mappings.yaml`: wire each capability to concrete HTTP (or GraphQL) templates.
4. **Validate:** `cargo run -p plasm-cli -- schema validate apis/<api>/domain.yaml` (and mapping checks per repo norms).
5. **Test:** REPL (`plasm-repl`) or `plasm-cgs` against a real backend; add `**apis/<api>/eval/cases.yaml`** when you want regression goals.

Repeat until coverage matches the surface you promised operators.

---

## Where files live

Canonical catalogs live under `**apis/<api-name>/**`:

```
apis/<api-name>/
  domain.yaml      # WHAT (CGS)
  mappings.yaml    # HOW (CML)
```

See [Catalogs index](../reference/apis-readme.md) for the full tree. Test-only fixtures under `fixtures/schemas/` are **not** the place for new REST catalogs unless you intend a tiny compiler fixture.

---

## First validation command

```bash
cargo run -p plasm-cli -- schema validate apis/<api>/domain.yaml
```

**Expect:** exit code `0`. Fix CGS errors before investing in deep mapping work.

---

## NL eval (optional but valuable)

Goal-oriented harness cases live in `**apis/<api>/eval/cases.yaml`**.

- **Deterministic coverage report:**  
`cargo run -p plasm-eval -- coverage --schema apis/<api> --cases apis/<api>/eval/cases.yaml`
- **Scaffold:**  
`cargo run -p plasm-eval -- scaffold --schema apis/<api>` (see `--write` in reference).

---

## Pack plugins (multi-entry hosts)

For `plasm-mcp` with `**--plugin-dir`**, pack catalogs to ABI v4 plugins (see `**AGENTS.md**` and [Genco plugin pipeline](../reference/genco-plugin-pipeline.md)).

---

## Next steps


| Need                                                       | Page                                                                              |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Full YAML doctrine, pagination, hydration, operator tables | **[Authoring reference](reference.md)**                                           |
| Language agents write (`e#`, heredocs, `.limit`)           | [Language specification](../reference/plasm-language-unification.md)              |
| Transport quirks when mappings fail at the wire            | [CML — mappings.yaml](reference.md#cml-capability-mapping-language--mappingsyaml) |
| Catalog roster                                             | [Catalogs](../reference/apis-readme.md)                                           |


---

## Honest scope note

`**domain.yaml` is not minted by a correct-by-construction OpenAPI→CGS generator** in this repo. Deterministic checks apply **after** YAML exists (`validate`, compile, eval). Expect **iterative** authoring for large APIs—same loop, many passes—not a single codegen run. Details: [Authoring reference — Authoring vs determinism](reference.md#authoring-vs-determinism).