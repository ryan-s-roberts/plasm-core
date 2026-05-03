# Correction vs error (catalogue)

This document summarizes how Plasm surfaces failures to operators vs to the LLM.

## Roles

| Concept | Audience | Content |
|--------|----------|---------|
| **ERROR** | Logs, REPL, `StepError::error` | `thiserror` / parser line; optional `span_offset` for tooling. Omitted from eval `correction_context` JSON. |
| **CORRECTION** | `correction_context`, retries | Single `correction` string: everything the model should act on (may contain multiple paragraphs). |

## JSON (eval / BAML)

- `correction` — the only LLM-facing instruction block (merged from any former “hint” lines).
- `span_offset` — optional byte offset.
- Raw ERROR strings are **not** included in this JSON.

Legacy eval JSON may still have `message` and/or `hints` / `correction_hints`; those are merged into `correction` when deserializing.

## Merge policy (parse + recovery)

When deterministic recovery returns multiple candidate lines, they are appended into `correction` after the expression (one block, no separate hint array).

## Type check: chain without Get

`TypeError::ChainTargetMissingGet` is raised when a chain step would auto-fetch an entity that has no Get capability; correction text tells the model to fetch another way or extend the schema.
