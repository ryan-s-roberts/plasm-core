//! Phrase + lexical indexes over one CGS catalog.

use std::collections::HashMap;
use std::sync::Arc;

use plasm_core::schema::CGS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhraseSource {
    DiscoveryName,
    EntityName,
    ExpressionAlias,
    CapabilityTargetTerm,
}

#[derive(Debug, Clone)]
pub struct PhraseHit {
    pub entry_id: String,
    pub entity: String,
    pub phrase: String,
    pub source: PhraseSource,
}

#[derive(Debug, Clone)]
pub struct CatalogIndex {
    pub entry_id: String,
    #[allow(dead_code)]
    pub catalog_hash: String,
    phrase_to_hits: HashMap<String, Vec<PhraseHit>>,
    pub cgs: Arc<CGS>,
}

fn norm_phrase(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Tokens from entity `description` for lexical recall (e.g. Jira Issue text mentions "bug").
const DESCRIPTION_TOKEN_STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "for", "with", "from", "to", "of", "in", "on", "is", "are",
    "was", "were", "be", "been", "being", "it", "this", "that", "these", "those", "as", "at",
    "by", "etc",
];

fn description_tokens(description: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in description.split(|c: char| !c.is_ascii_alphanumeric()) {
        let t = raw.trim().to_lowercase();
        if t.len() < 3 || DESCRIPTION_TOKEN_STOPWORDS.contains(&t.as_str()) {
            continue;
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

fn insert_phrase(map: &mut HashMap<String, Vec<PhraseHit>>, key: String, hit: PhraseHit) {
    map.entry(key).or_default().push(hit);
}

/// Naive English plural for short discovery keys (e.g. `issue` → `issues`, `berry` → `berrys` is imperfect but cheap recall).
fn insert_phrase_with_plural_variant(
    map: &mut HashMap<String, Vec<PhraseHit>>,
    key: String,
    hit: PhraseHit,
) {
    insert_phrase(map, key.clone(), hit.clone());
    if key.len() >= 3 && !key.ends_with('s') {
        insert_phrase(map, format!("{key}s"), hit);
    }
}

impl CatalogIndex {
    pub fn build(entry_id: String, cgs: Arc<CGS>) -> Self {
        let catalog_hash = cgs.catalog_cgs_hash_hex();
        let mut phrase_to_hits: HashMap<String, Vec<PhraseHit>> = HashMap::new();
        let eid = entry_id.clone();

        for (ename, ent) in cgs.entities.iter() {
            let entity_s = ename.to_string();
            let key = norm_phrase(&entity_s);
            if !key.is_empty() {
                insert_phrase_with_plural_variant(
                    &mut phrase_to_hits,
                    key.clone(),
                    PhraseHit {
                        entry_id: eid.clone(),
                        entity: entity_s.clone(),
                        phrase: entity_s.clone(),
                        source: PhraseSource::EntityName,
                    },
                );
            }
            for a in &ent.expression_aliases {
                let k = norm_phrase(a);
                if k.is_empty() {
                    continue;
                }
                insert_phrase_with_plural_variant(
                    &mut phrase_to_hits,
                    k.clone(),
                    PhraseHit {
                        entry_id: eid.clone(),
                        entity: entity_s.clone(),
                        phrase: a.clone(),
                        source: PhraseSource::ExpressionAlias,
                    },
                );
            }
            if let Some(d) = &ent.discovery {
                for n in &d.names {
                    let k = norm_phrase(n);
                    if k.is_empty() {
                        continue;
                    }
                    insert_phrase_with_plural_variant(
                        &mut phrase_to_hits,
                        k.clone(),
                        PhraseHit {
                            entry_id: eid.clone(),
                            entity: entity_s.clone(),
                            phrase: n.clone(),
                            source: PhraseSource::DiscoveryName,
                        },
                    );
                }
            }

            let entity_key = norm_phrase(&entity_s);
            for tok in description_tokens(ent.description.as_str()) {
                if tok == entity_key {
                    continue;
                }
                insert_phrase_with_plural_variant(
                    &mut phrase_to_hits,
                    tok.clone(),
                    PhraseHit {
                        entry_id: eid.clone(),
                        entity: entity_s.clone(),
                        phrase: tok.clone(),
                        source: PhraseSource::DiscoveryName,
                    },
                );
            }
        }

        for cap in cgs.capabilities.values() {
            let domain = cap.domain.to_string();
            if let Some(d) = &cap.discovery {
                for t in &d.target_terms {
                    let k = norm_phrase(t);
                    if k.is_empty() {
                        continue;
                    }
                    insert_phrase(
                        &mut phrase_to_hits,
                        k.clone(),
                        PhraseHit {
                            entry_id: eid.clone(),
                            entity: domain.clone(),
                            phrase: t.clone(),
                            source: PhraseSource::CapabilityTargetTerm,
                        },
                    );
                }
            }
        }

        Self {
            entry_id,
            catalog_hash,
            phrase_to_hits,
            cgs,
        }
    }

    pub fn lookup_phrase(&self, phrase: &str) -> Vec<PhraseHit> {
        let k = norm_phrase(phrase);
        self.phrase_to_hits.get(&k).cloned().unwrap_or_default()
    }

    /// Any phrase key contained as substring in `utterance` (normalized), longest keys first.
    pub fn scan_utterance(&self, utterance_lower: &str) -> Vec<PhraseHit> {
        let u = norm_phrase(utterance_lower);
        let mut keys: Vec<&String> = self.phrase_to_hits.keys().collect();
        keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
        let mut seen = HashMap::<(String, String, String), ()>::new();
        let mut out = Vec::new();
        for k in keys {
            if u.contains(k.as_str()) {
                for h in self.phrase_to_hits.get(k).into_iter().flatten() {
                    let sig = (h.entry_id.clone(), h.entity.clone(), h.phrase.clone());
                    if seen.insert(sig, ()).is_none() {
                        out.push(h.clone());
                    }
                }
            }
        }
        out
    }

    pub fn entity_count(&self) -> usize {
        self.cgs.entities.len()
    }

    pub fn capability_count(&self) -> usize {
        self.cgs.capabilities.len()
    }
}

/// Whether `qualifier` is plausibly attached to `entity` via fields, entity discovery hints, or one hop relations.
pub fn qualifier_supported(cgs: &CGS, entity: &str, qualifier: &str) -> bool {
    let q = norm_phrase(qualifier);
    if q.is_empty() {
        return true;
    }
    let Some(ent) = cgs.entities.get(entity) else {
        return false;
    };

    for (fname, _) in ent.fields.iter() {
        let fk = norm_phrase(fname.to_string().as_str());
        if fk.contains(&q) || q.contains(&fk) {
            return true;
        }
    }

    if let Some(d) = &ent.discovery {
        for qn in &d.qualifier_names {
            if norm_phrase(qn) == q {
                return true;
            }
        }
    }

    for rel in ent.relations.values() {
        if let Some(rd) = &rel.discovery {
            for t in &rd.qualifier_terms {
                if norm_phrase(t) == q {
                    return true;
                }
            }
        }
        let tgt = rel.target_resource.to_string();
        let tk = norm_phrase(&tgt);
        if tk == q || tk.contains(&q) || q.contains(&tk) {
            return true;
        }
        if let Some(neigh) = cgs.entities.get(tgt.as_str()) {
            if let Some(d) = &neigh.discovery {
                for n in &d.names {
                    let nk = norm_phrase(n);
                    if nk == q || nk.contains(&q) || q.contains(&nk) {
                        return true;
                    }
                }
            }
        }
    }

    false
}
