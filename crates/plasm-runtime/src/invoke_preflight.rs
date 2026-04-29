//! Invoke-time preflight: merge a hydrated GET row into the CML env under `{prefix}_{field}`.
//!
//! Orchestration lives on [`crate::ExecutionEngine::apply_invoke_preflight`](crate::ExecutionEngine::apply_invoke_preflight)
//! in [`execution`](crate::execution) (runs the configured GET, cache read-through, then merge).

use indexmap::IndexMap;
use plasm_compile::CmlEnv;
use plasm_core::TypedFieldValue;

pub(crate) fn merge_preflight_fields_into_env(
    env: &mut CmlEnv,
    prefix: &str,
    fields: &IndexMap<String, TypedFieldValue>,
) {
    for (k, v) in fields {
        env.insert(format!("{prefix}_{k}"), v.to_value());
    }
}
