# plasm-e2e

End-to-end integration tests (Hermit and related harnesses) for the Plasm stack.

## Design boundary: no domain leakage

Plasm is a **general-purpose language and runtime for API mapping** (schema, expressions, CML, execution). **Domain-specific knowledge is forbidden in this crate:** no branches on particular CGS entity or capability names from `apis/…`, no field-alias or env-key hacks for one vendor’s HTTP templates, and no special transport cases tied to a single product.

Catalog behavior belongs in `**apis/<name>/`**, fixtures, and optional **plugins**—expressed as data and schema-driven rules. Code here stays **agnostic**, driven only by loaded CGS and generic IR/types.

Plasm **surface language + DAG semantics** for real multi-line programs live in `[tests/plasm_language_matrix.rs](tests/plasm_language_matrix.rs)` (Hermit-backed live runs against `[fixtures/schemas/plasm_language_matrix](../../fixtures/schemas/plasm_language_matrix)` / `[fixtures/real_openapi_specs/plasm_language_matrix.yaml](../../fixtures/real_openapi_specs/plasm_language_matrix.yaml)`). Prefer extending that matrix over adding scattered story-style compiler tests elsewhere.

### Where other tests belong


| Tier                           | Crate / target                                                              | Role                                                                                                                                                                        |
| ------------------------------ | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Language catalog**           | `cargo test -p plasm-e2e --test plasm_language_matrix`                      | User-visible programs: parse → plan → dry IR → **live** Hermit; feature tags + matrix rows.                                                                                 |
| **Compiler / plan invariants** | `plasm-agent-core` `plasm_dag` `#[cfg(test)]` (+ `evaluate_plasm_plan_dry`) | Staging, diagnostics, plan JSON shape **without HTTP**. Keep **small**; if it asserts “this program means X” for authors, add or reference a **matrix row**.                |
| **Parser micro-cases**         | `plasm-core` `expr_parser` (+ `postfix`)                                    | Lexer edge cases, error spans, postfix peel order, tiny AST — **minimal CGS**. Semantic “this Plasm line means X” should **mirror** a matrix row in a comment or test name. |
| **Integration smoke**          | `cargo test -p plasm-e2e --test hermit_e2e`                                 | Hermit + engine + cache + CLI over **fixture/vendor** CGS — **not** the unified language conformance suite.                                                                 |


See [AGENTS.md](../../AGENTS.md) for workspace layout and commands.