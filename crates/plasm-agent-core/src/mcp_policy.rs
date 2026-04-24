//! Filter discovery output using [`crate::mcp_runtime_config::McpRuntimeConfig`].

use plasm_core::discovery::DiscoveryResult;

use crate::mcp_runtime_config::McpRuntimeConfig;

pub fn filter_discovery_result(mut r: DiscoveryResult, cfg: &McpRuntimeConfig) -> DiscoveryResult {
    r.candidates.retain(|c| {
        cfg.entry_allowed(&c.entry_id) && cfg.capability_allowed(&c.entry_id, &c.capability_name)
    });
    r.schema_neighborhoods
        .retain(|n| cfg.entry_allowed(&n.entry_id));
    r.ambiguities
        .retain(|a| a.entry_ids.iter().any(|eid| cfg.entry_allowed(eid)));
    r
}

pub fn filter_registry_entries(
    entries: Vec<plasm_core::discovery::CatalogEntryMeta>,
    cfg: &McpRuntimeConfig,
) -> Vec<plasm_core::discovery::CatalogEntryMeta> {
    entries
        .into_iter()
        .filter(|m| cfg.entry_allowed(&m.entry_id))
        .collect()
}
