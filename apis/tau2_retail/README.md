# τ²-bench retail (simulated DB) — CGS slice

This directory holds a **Capability Graph Schema** aligned with the **retail** domain in [sierra-research/tau2-bench](https://github.com/sierra-research/tau2-bench): tool names and parameters match [`src/tau2/domains/retail/tools.py`](https://github.com/sierra-research/tau2-bench/blob/main/src/tau2/domains/retail/tools.py), and entity shapes follow [`data_model.py`](https://github.com/sierra-research/tau2-bench/blob/main/src/tau2/domains/retail/data_model.py).

There is **no public HTTP API** for this domain in τ² — state lives in Python `RetailDB`. **`mappings.yaml`** uses synthetic **`POST /v1/tau2/retail/<tool_name>`** routes with a JSON **body** so a future **HTTP shim** can forward to `RetailTools.use_tool(...)`.

**User lookups:** `find_user_id_by_email` / `find_user_id_by_name_zip` are **`kind: query` on `User`** with **`provides: [user_id]`** (subset response). With **`get_user_details`** on the same entity, the runtime can **hydrate** query rows to the full profile — same pattern as disjoint API projections in the Plasm authoring reference (one logical entity, explicit `provides`, optional per-row GET).

**Product ↔ Variant:** `Variant.product_id` is the forward **EntityRef**; **`Product.relations.variants`** (cardinality **many**, **`via_param: product_id`**) is the synthesized inverse used by **`list_variants_by_product`** so `plasm-agent` can run **`product <product_id> variants`** (scope injected from the parent id; list rows hydrate via **`get_item_details`** when not using `--summary`).

## Use today

- **NL → expression:** `cargo run -p plasm-eval -- --schema apis/tau2_retail --cases apis/tau2_retail/eval/cases.yaml`
- **Coverage (no LLM):** `cargo run -p plasm-eval -- coverage --schema apis/tau2_retail --cases apis/tau2_retail/eval/cases.yaml`

## Eval cases (Plasm format)

Upstream τ² retail tasks are vendored and converted to Plasm `EvalCase` YAML:

| File | Role |
|------|------|
| [`eval/tau2_retail_tasks.json`](eval/tau2_retail_tasks.json) | Copy of [tau2-bench `data/tau2/domains/retail/tasks.json`](https://github.com/sierra-research/tau2-bench/blob/main/data/tau2/domains/retail/tasks.json) (see [`eval/SOURCE.md`](eval/SOURCE.md) for provenance). |
| [`eval/cases.yaml`](eval/cases.yaml) | Generated: one case per τ² task id `0`–`113` (`tau2-000` … `tau2-113`) plus three **synthetic** coverage stubs (`tau2-cov-*`). |

**Regenerate** after replacing the JSON (e.g. upstream bump):

```bash
python3 scripts/port_tau2_retail_tasks.py
```

The generator maps each official tool name to CGS entities for `expect.entities_any`, derives `covers` from the tool multiset, and builds `goal` from `reason_for_call` plus `known_info` / `unknown_info` (and non-placeholder `task_instructions`). Official traces never call `list_all_product_types` or `list_variants_by_product`, so synthetic cases ensure `plasm-eval coverage` includes `query_all`, `reverse`, and `chain` buckets.

### Metric differences vs τ²-bench

- **τ²** scores exact tool call sequences against `RetailDB`. **Plasm eval** scores parsed **Plasm expressions** against soft YAML expectations (`entities_any`, `covers`, etc.); it does **not** replay τ² actions.
- **`pred_fields_any` is omitted** in the bulk port: the harness matches predicate fields on **queries**, not invoke/update arguments, so filling fields from τ² would be misleading.
- Tasks whose official `evaluation_criteria.actions` is **empty** (simulator-only checks via `communicate_info` / `nl_assertions`) are tagged **`tau2_communicate_only`**; expression-level scoring is intentionally weak for those rows.

## Use later (transport)

Point `plasm-agent --backend` at a shim that implements the paths in `mappings.yaml` (see plan: Phase 2 τ-DB transport).

## τ² eval (parallel track)

Run the official harness on the same natural-language task text as in `eval/cases.yaml` goals — comparable **task wording**, not identical **metric**.
