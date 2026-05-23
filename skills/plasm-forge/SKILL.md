---
name: plasm-forge
description: OSS entry skill for Plasm CGS / CML catalog authoring — points at the canonical skill suite under `skills/` in plasm-core. Use when agents need the authoring playbook; pair with `.cursor/agents/plasm-forge.md` (Cursor) for autonomous catalog work.
---

# Plasm Forge (OSS Entry)

**This file carries no authoring doctrine.** The canonical Plasm authoring skill suite lives under `skills/` in this repository:

- [plasm-authoring/SKILL.md](../plasm-authoring/SKILL.md) — primary CGS / CML authoring workflow.
- [plasm-authoring/reference.md](../plasm-authoring/reference.md) — deep CGS / CML reference.

Companion skills:

- [plasm-catalog-e2e-test/SKILL.md](../plasm-catalog-e2e-test/SKILL.md) — Hermit, live, and sandbox transport testing.
- [plasm-catalog-polish/SKILL.md](../plasm-catalog-polish/SKILL.md) — autonomous diagnostic / fix loop.
- [plasm-catalog-score/SKILL.md](../plasm-catalog-score/SKILL.md) — rubric scorecard.
- [plasm-catalog-reprint/SKILL.md](../plasm-catalog-reprint/SKILL.md) — full-cutover regeneration.
- [plasm-catalog-retro/SKILL.md](../plasm-catalog-retro/SKILL.md) — post-authoring retrospective.

## Why this entry exists

CGS / CML / catalog authoring is a `plasm-core` concern. These skills are **redistributable** under `skills/` (not tied to a specific IDE). API catalogs live under `apis/`; the compiler, runtime, and packaging live in this repository.

Any product-specific guidance (hosted deployment, SaaS control plane) belongs in consumer repos' `docs/` or agent files. **Do not** duplicate CGS / CML authoring rules outside `skills/`.

## Cursor agent

For autonomous catalog runs in Cursor, use [`.cursor/agents/plasm-forge.md`](../../.cursor/agents/plasm-forge.md).

If a doc references **plasm-forge**, treat the `plasm-authoring` skill tree as the actual destination for doctrine.
