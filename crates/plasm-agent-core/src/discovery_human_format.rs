//! Single discovery surface for agents: MCP `discover_capabilities` (non-typed) and HTTP terminal discover
//! both render via [`format_discovery_markdown`].

use std::collections::{BTreeMap, BTreeSet};

use plasm_core::discovery::{Ambiguity, DiscoveryResult, EntitySummary, RankedCandidate};

/// MCP entity `description` column: max chars (Unicode scalars).
pub const MCP_DISCOVERY_ENTITY_SUMMARY_MAX: usize = 200;

/// Single-line TSV field: collapse whitespace, strip tabs, truncate (Unicode scalars).
fn mcp_discovery_tsv_field(s: &str, max_chars: usize) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let no_tabs = collapsed.replace('\t', " ");
    let n = no_tabs.chars().count();
    if n <= max_chars {
        no_tabs
    } else {
        let head: String = no_tabs.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

fn entity_summary_description<'a>(
    entity_summaries: &'a [EntitySummary],
    entity: &str,
) -> Option<&'a str> {
    entity_summaries
        .iter()
        .find(|e| e.name == entity)
        .map(|e| e.description.as_str())
}

/// TSV: `api`, `entity`, `description` — dedupe `(entry_id, entity)` from `candidates`, stable sort.
pub fn discovery_capability_tsv_for_candidates(
    candidates: &[RankedCandidate],
    entity_summaries: &[EntitySummary],
) -> String {
    let mut by_entry: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in candidates {
        by_entry
            .entry(c.entry_id.clone())
            .or_default()
            .insert(c.entity.clone());
    }

    let mut lines = vec!["api\tentity\tdescription".to_string()];
    for (eid, entities) in &by_entry {
        for entity in entities {
            let description = entity_summary_description(entity_summaries, entity)
                .map(|raw| mcp_discovery_tsv_field(raw, MCP_DISCOVERY_ENTITY_SUMMARY_MAX))
                .unwrap_or_default();
            lines.push(format!(
                "{}\t{}\t{}",
                mcp_discovery_tsv_field(eid, 200),
                mcp_discovery_tsv_field(entity, 200),
                description,
            ));
        }
    }
    lines.join("\n")
}

/// Full structured discovery result (all ranked candidates).
pub fn discovery_capability_tsv(result: &DiscoveryResult) -> String {
    discovery_capability_tsv_for_candidates(&result.candidates, &result.entity_summaries)
}

fn ambiguity_markdown_lines(ambiguities: &[Ambiguity]) -> String {
    let mut s = String::new();
    for Ambiguity {
        dimension: _,
        entry_ids,
        capability_name,
        score,
    } in ambiguities
    {
        s.push_str(&format!(
            "- `{capability_name}` (score {score}) — pick one `api`: {}\n",
            entry_ids.join(", ")
        ));
    }
    s
}

/// Markdown block for ambiguities (same text as MCP `discover_capabilities`).
pub fn discovery_ambiguity_markdown(result: &DiscoveryResult) -> String {
    if result.ambiguities.is_empty() {
        return String::new();
    }
    let mut s = String::from("**Same name in more than one `api`**\n\n");
    s.push_str(&ambiguity_markdown_lines(&result.ambiguities));
    s.push('\n');
    s
}

/// MCP `discover_capabilities` (non-typed) and `POST /v1/terminal/discover`: fenced TSV + ambiguity notes.
pub fn format_discovery_markdown(result: &DiscoveryResult) -> String {
    let mut s = String::new();
    if result.candidates.is_empty() {
        s.push_str("_No matching entities._\n\n");
    } else {
        s.push_str("```tsv\n");
        s.push_str(&discovery_capability_tsv(result));
        s.push_str("\n```\n\n");
    }
    s.push_str(&discovery_ambiguity_markdown(result));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use plasm_core::discovery::CapabilityQuery;

    #[test]
    fn discovery_capability_tsv_header_and_row() {
        let r = DiscoveryResult {
            contexts: vec![],
            candidates: vec![RankedCandidate {
                entry_id: "demo".into(),
                entity: "Widget".into(),
                capability_name: "list".into(),
                score: 2,
                reason_codes: vec![],
                capability_description: "List widgets".into(),
            }],
            ambiguities: vec![],
            applied_query_echo: CapabilityQuery::default(),
            closure_stats: None,
            schema_neighborhoods: vec![],
            entity_summaries: vec![EntitySummary {
                name: "Widget".into(),
                description: "Widget summary.".into(),
            }],
        };
        let tsv = discovery_capability_tsv(&r);
        assert!(tsv.starts_with("api\tentity\tdescription\n"));
        assert!(tsv.contains("demo\tWidget\tWidget summary."));
    }

    #[test]
    fn format_discovery_markdown_matches_mcp_non_typed_shape() {
        let r = DiscoveryResult {
            contexts: vec![],
            candidates: vec![RankedCandidate {
                entry_id: "demo".into(),
                entity: "Widget".into(),
                capability_name: "list".into(),
                score: 2,
                reason_codes: vec![],
                capability_description: "List widgets".into(),
            }],
            ambiguities: vec![],
            applied_query_echo: CapabilityQuery::default(),
            closure_stats: None,
            schema_neighborhoods: vec![],
            entity_summaries: vec![EntitySummary {
                name: "Widget".into(),
                description: "Widget summary.".into(),
            }],
        };
        let md = format_discovery_markdown(&r);
        assert!(md.contains("```tsv"));
        assert!(md.contains("demo\tWidget\tWidget summary."));
        assert!(!md.contains("typed:"));
    }
}
