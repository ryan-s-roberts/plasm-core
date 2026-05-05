//! Typed decomposition, scoring, gating, and [`crate::AgentDiscovery`] implementation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use plasm_core::schema::{CapabilityKind, CGS};
use tracing::{debug_span, info_span, Instrument};

use crate::decompose::decompose;
use crate::embedder::{cosine_sim, BlockingEmbedder};
use crate::index::{qualifier_supported, CatalogIndex, PhraseHit, PhraseSource};
use crate::metrics;
use crate::types::{
    evidence_codes, ClarificationAnswer, ClarificationDimension, ClarificationOption,
    ClarificationPrompt, ClarificationState, DiscoveryDecision, DiscoveryError, DiscoveryEvidence,
    DiscoveryQuery, ReadyTarget, TargetHypothesis,
};

/// Async typed discovery over one or more loaded CGS graphs.
pub struct TypedDiscovery {
    indexes: Vec<CatalogIndex>,
    embedder: Option<Arc<BlockingEmbedder>>,
    /// Max clarification / ready options per response.
    pub max_options: usize,
}

impl TypedDiscovery {
    pub fn from_cgs_entries(entries: Vec<(String, Arc<CGS>)>, enable_embeddings: bool) -> Self {
        let t0 = Instant::now();
        let mut indexes = Vec::new();
        let mut total_ent = 0i64;
        let mut total_cap = 0i64;
        for (eid, cgs) in entries {
            let idx = CatalogIndex::build(eid, cgs);
            total_ent += idx.entity_count() as i64;
            total_cap += idx.capability_count() as i64;
            indexes.push(idx);
        }
        metrics::record_index_build("success", t0.elapsed());
        metrics::record_index_sizes(total_ent, total_cap);

        let embedder = if enable_embeddings {
            Some(Arc::new(BlockingEmbedder::new(
                fastembed::EmbeddingModel::AllMiniLML6V2,
                2,
            )))
        } else {
            None
        };

        Self {
            indexes,
            embedder,
            max_options: 8,
        }
    }

    pub fn with_max_options(mut self, n: usize) -> Self {
        self.max_options = n.max(1).min(32);
        self
    }

    fn catalog_entry_ids(&self) -> Vec<String> {
        self.indexes.iter().map(|i| i.entry_id.clone()).collect()
    }

    fn cgs_for_entry<'a>(&'a self, entry_id: &str) -> Option<&'a CGS> {
        self.indexes
            .iter()
            .find(|i| i.entry_id == entry_id)
            .map(|i| i.cgs.as_ref())
    }

    fn score_hit(hit: &PhraseHit) -> f64 {
        let base = match hit.source {
            PhraseSource::DiscoveryName => 100.0,
            PhraseSource::EntityName => 90.0,
            PhraseSource::ExpressionAlias => 85.0,
            PhraseSource::CapabilityTargetTerm => 70.0,
        };
        base + hit.phrase.len() as f64 * 0.05
    }

    fn pick_capability_for_entity(
        cgs: &CGS,
        entity: &str,
        verbs: &[String],
    ) -> Option<(String, CapabilityKind)> {
        let mut caps: Vec<_> = cgs
            .capabilities
            .iter()
            .filter(|(_, c)| c.domain.as_str() == entity)
            .map(|(n, c)| (n.to_string(), c.kind, c))
            .collect();
        caps.sort_by(|a, b| a.0.cmp(&b.0));

        let wants_search = verbs.iter().any(|v| v == "search");
        let wants_get = verbs
            .iter()
            .any(|v| ["get", "fetch", "open"].contains(&v.as_str()));
        let wants_query = verbs.iter().any(|v| {
            ["list", "query", "show", "find", "pull"]
                .iter()
                .any(|x| *x == v.as_str())
        });

        if wants_search {
            if let Some((n, k, _)) = caps
                .iter()
                .find(|(_, k, _)| matches!(k, CapabilityKind::Search))
            {
                return Some((n.clone(), *k));
            }
        }
        if wants_get {
            if let Some((n, k, _)) = caps
                .iter()
                .find(|(_, k, _)| matches!(k, CapabilityKind::Get))
            {
                return Some((n.clone(), *k));
            }
        }
        if wants_query {
            if let Some((n, k, _)) = caps
                .iter()
                .find(|(_, k, _)| matches!(k, CapabilityKind::Query))
            {
                return Some((n.clone(), *k));
            }
        }

        caps.first().map(|(n, k, _)| (n.clone(), *k))
    }

    fn evidence_for_hit(hit: &PhraseHit) -> DiscoveryEvidence {
        let (code, detail) = match hit.source {
            PhraseSource::DiscoveryName => (evidence_codes::DISCOVERY_NAME, hit.phrase.clone()),
            PhraseSource::EntityName => (evidence_codes::ENTITY_NAME, hit.entity.clone()),
            PhraseSource::ExpressionAlias => (evidence_codes::EXPRESSION_ALIAS, hit.phrase.clone()),
            PhraseSource::CapabilityTargetTerm => {
                (evidence_codes::CAP_TARGET_TERM, hit.phrase.clone())
            }
        };
        DiscoveryEvidence::new(code, detail)
    }

    async fn discover_inner(
        &self,
        query: &DiscoveryQuery,
    ) -> Result<DiscoveryDecision, DiscoveryError> {
        let utterance = query.utterance.trim();
        if utterance.is_empty() {
            metrics::record_request_outcome("error");
            return Err(DiscoveryError::EmptyUtterance);
        }

        let span = info_span!(
            "plasm.discovery.discover",
            model_id = self
                .embedder
                .as_ref()
                .map(|e| e.model_id())
                .unwrap_or("none"),
            entry_count = self.indexes.len() as i64,
        );
        async { self.discover_inner_body(query, utterance).await }
            .instrument(span)
            .await
    }

    async fn discover_inner_body(
        &self,
        query: &DiscoveryQuery,
        utterance: &str,
    ) -> Result<DiscoveryDecision, DiscoveryError> {
        let t_dec0 = Instant::now();
        let catalog_ids = self.catalog_entry_ids();
        for eid in &query.allowed_entry_ids {
            if !catalog_ids.iter().any(|c| c == eid) {
                metrics::record_request_outcome("error");
                return Err(DiscoveryError::UnknownEntry(eid.clone()));
            }
        }

        let decomposed = {
            let _g = debug_span!("plasm.discovery.decompose_intent").entered();
            decompose(utterance, &catalog_ids)
        };
        metrics::record_intent_decompose_duration(t_dec0.elapsed());

        let lower = utterance.to_lowercase();

        let mut hits: Vec<PhraseHit> = Vec::new();
        for idx in &self.indexes {
            if !query.allowed_entry_ids.is_empty()
                && !query.allowed_entry_ids.iter().any(|e| e == &idx.entry_id)
            {
                continue;
            }
            if let Some(ref fe) = query.force_entry_id {
                if fe != &idx.entry_id {
                    continue;
                }
            }
            hits.extend(idx.scan_utterance(&lower));
            for p in &decomposed.noun_phrases {
                hits.extend(idx.lookup_phrase(p));
            }
        }

        let mut seen = HashSet::new();
        hits.retain(|h| {
            let k = (
                h.entry_id.clone(),
                h.entity.clone(),
                h.phrase.clone(),
                format!("{:?}", h.source),
            );
            seen.insert(k)
        });

        let mut hypotheses: Vec<TargetHypothesis> = Vec::new();
        for h in hits {
            let Some(idx) = self.indexes.iter().find(|i| i.entry_id == h.entry_id) else {
                continue;
            };
            if let Some(ref fe) = query.force_entity {
                if fe.to_lowercase() != h.entity.to_lowercase() {
                    continue;
                }
            }
            let Some((cap_name, cap_kind)) =
                Self::pick_capability_for_entity(&idx.cgs, &h.entity, &decomposed.operation_verbs)
            else {
                continue;
            };
            let score = Self::score_hit(&h);
            let ev = vec![Self::evidence_for_hit(&h)];
            hypotheses.push(TargetHypothesis {
                entry_id: h.entry_id.clone(),
                entity: h.entity.clone(),
                capability_name: cap_name,
                capability_kind: cap_kind,
                score,
                matched_phrase: h.phrase.clone(),
                qualifiers: Vec::new(),
                evidence: ev,
            });
        }

        // One hypothesis per (catalog, entity): multiple capabilities for the same entity
        // share one lexical hit — keeping all of them floods clarification lists and hides
        // distinct entities (e.g. GitHub `Label` vs `Issue`).
        let mut best: HashMap<(String, String), TargetHypothesis> = HashMap::new();
        for hyp in hypotheses {
            let k = (hyp.entry_id.clone(), hyp.entity.clone());
            best.entry(k)
                .and_modify(|e| {
                    if hyp.score > e.score {
                        *e = hyp.clone();
                    }
                })
                .or_insert(hyp);
        }
        let mut hypotheses: Vec<TargetHypothesis> = best.into_values().collect();
        hypotheses.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if query.enable_embeddings && self.embedder.is_some() && !hypotheses.is_empty() {
            let t_embed = Instant::now();
            metrics::record_embed_cache("miss");
            let embedder = self.embedder.as_ref().unwrap().clone();
            let lines: Vec<String> = hypotheses
                .iter()
                .map(|h| format!("{} {} {}", h.entry_id, h.entity, h.matched_phrase))
                .collect();
            let mut all_texts = vec![utterance.to_string()];
            all_texts.extend(lines.iter().cloned());
            let t_batch = Instant::now();
            match embedder.embed_batch(all_texts).await {
                Ok(vecs) if vecs.len() == hypotheses.len() + 1 => {
                    let qv = &vecs[0];
                    for (i, h) in hypotheses.iter_mut().enumerate() {
                        let sim = cosine_sim(qv, &vecs[i + 1]);
                        h.score += 30.0 * sim;
                        if sim > 0.01 {
                            h.evidence.push(DiscoveryEvidence::new(
                                evidence_codes::EMBEDDING_SIM,
                                format!("{sim:.4}"),
                            ));
                        }
                    }
                    metrics::record_embed_batch_duration(t_batch.elapsed());
                }
                Ok(_) => metrics::record_embed_cache("error"),
                Err(e) => {
                    metrics::record_embed_cache("error");
                    tracing::warn!(error = %e, "typed discovery embedding failed; continuing lexical-only");
                }
            }
            let _ = t_embed;
        }

        hypotheses.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let max_h = self.max_options.min(query.max_options).max(1);
        hypotheses.truncate(max_h);
        metrics::record_hypothesis_count(hypotheses.len() as u64);

        if decomposed.api_hints.len() == 1 {
            let only = decomposed.api_hints[0].clone();
            hypotheses.retain(|h| h.entry_id == only);
        }

        if hypotheses.is_empty() {
            metrics::record_option_count(0);
            metrics::record_request_outcome("no_match");
            return Ok(DiscoveryDecision::NoMatch {
                evidence: vec![DiscoveryEvidence::new(
                    "no_lexical_hit",
                    "no phrase matched catalog vocabulary after filters",
                )],
            });
        }

        let t_graph = Instant::now();
        {
            let _g = debug_span!("plasm.discovery.validate_qualifiers").entered();
            let qualifiers_candidates: Vec<String> = if let Some(top) = hypotheses.first() {
                decomposed
                    .noun_phrases
                    .iter()
                    .filter(|p| {
                        !top.matched_phrase.eq_ignore_ascii_case(p)
                            && !top.entity.eq_ignore_ascii_case(p)
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            for h in &mut hypotheses {
                let Some(cgs) = self.cgs_for_entry(&h.entry_id) else {
                    continue;
                };
                let mut qv = Vec::new();
                for q in &qualifiers_candidates {
                    if q.len() < 2 {
                        continue;
                    }
                    if qualifier_supported(cgs, &h.entity, q) {
                        qv.push(q.clone());
                        h.evidence.push(DiscoveryEvidence::new(
                            evidence_codes::GRAPH_QUALIFIER_OK,
                            format!("{} supports '{}'", h.entity, q),
                        ));
                    }
                }
                h.qualifiers = qv;
            }
        }
        metrics::record_graph_validate_duration(t_graph.elapsed());

        let t_gate = Instant::now();
        let _g = debug_span!("plasm.discovery.gate_decision").entered();
        let cap = self.max_options.min(query.max_options).max(1);

        // Clarify API: same matched phrase, multiple catalogs, no API hint.
        if decomposed.api_hints.is_empty() && hypotheses.len() > 1 {
            let phrase0 = hypotheses[0].matched_phrase.to_lowercase();
            let same_phrase: Vec<_> = hypotheses
                .iter()
                .filter(|h| h.matched_phrase.to_lowercase() == phrase0)
                .collect();
            let mut eids: HashSet<String> = HashSet::new();
            for h in &same_phrase {
                eids.insert(h.entry_id.clone());
            }
            if eids.len() > 1 {
                let opts: Vec<ClarificationOption> = eids
                    .into_iter()
                    .take(cap)
                    .map(|eid| ClarificationOption {
                        label: format!("catalog `{eid}`"),
                        entry_id: Some(eid),
                        entity: None,
                        qualifier: None,
                    })
                    .collect();
                metrics::record_option_count(opts.len() as u64);
                metrics::record_clarification("api");
                metrics::record_request_outcome("clarify_api");
                metrics::record_decision_duration(t_gate.elapsed());
                return Ok(DiscoveryDecision::ClarifyApi {
                    prompt: ClarificationPrompt {
                        dimension: ClarificationDimension::Api,
                        prompt:
                            "Multiple catalogs match this phrase. Which integration should we use?"
                                .into(),
                        options: opts,
                        evidence: vec![],
                    },
                });
            }
        }

        // Clarify entity: top two close scores, different entities.
        if hypotheses.len() > 1 {
            let a = &hypotheses[0];
            let b = &hypotheses[1];
            if (a.score - b.score).abs() < 8.0 && a.entity != b.entity {
                let opts: Vec<ClarificationOption> = hypotheses
                    .iter()
                    .take(cap)
                    .map(|h| ClarificationOption {
                        label: format!("{} — {}", h.entry_id, h.entity),
                        entry_id: Some(h.entry_id.clone()),
                        entity: Some(h.entity.clone()),
                        qualifier: None,
                    })
                    .collect();
                metrics::record_option_count(opts.len() as u64);
                metrics::record_clarification("entity");
                metrics::record_request_outcome("clarify_entity");
                metrics::record_decision_duration(t_gate.elapsed());
                return Ok(DiscoveryDecision::ClarifyEntity {
                    prompt: ClarificationPrompt {
                        dimension: ClarificationDimension::Entity,
                        prompt: "Several resource types match. Which one did you mean?".into(),
                        options: opts,
                        evidence: vec![],
                    },
                });
            }
        }

        // Qualifier clarification if graph rejected extra noun phrases.
        let top = hypotheses.first().unwrap();
        let qualifiers: Vec<String> = decomposed
            .noun_phrases
            .iter()
            .filter(|p| {
                !top.matched_phrase.eq_ignore_ascii_case(p) && !top.entity.eq_ignore_ascii_case(p)
            })
            .cloned()
            .collect();
        let cgs = self.cgs_for_entry(&top.entry_id).unwrap();
        let bad_q: Vec<String> = qualifiers
            .into_iter()
            .filter(|q| q.len() >= 2 && !qualifier_supported(cgs, &top.entity, q))
            .take(3)
            .collect();
        if !bad_q.is_empty() {
            let mut opts = Vec::new();
            if let Some(ent) = cgs.entities.get(top.entity.as_str()) {
                for rel in ent.relations.values() {
                    let tgt = rel.target_resource.to_string();
                    if qualifier_supported(cgs, &tgt, &bad_q[0]) {
                        opts.push(ClarificationOption {
                            label: format!("{tgt} (via relation {})", rel.name.as_str()),
                            entry_id: Some(top.entry_id.clone()),
                            entity: Some(tgt),
                            qualifier: Some(bad_q[0].clone()),
                        });
                    }
                }
            }
            opts.push(ClarificationOption {
                label: "Ignore unsupported qualifiers".into(),
                entry_id: Some(top.entry_id.clone()),
                entity: Some(top.entity.clone()),
                qualifier: None,
            });
            opts.truncate(cap);
            if opts.len() > 1 {
                metrics::record_option_count(opts.len() as u64);
                metrics::record_clarification("qualifier");
                metrics::record_request_outcome("clarify_qualifier");
                metrics::record_decision_duration(t_gate.elapsed());
                return Ok(DiscoveryDecision::ClarifyQualifier {
                    prompt: ClarificationPrompt {
                        dimension: ClarificationDimension::Qualifier,
                        prompt: format!(
                            "The phrase {:?} is not clearly attached to `{}` on catalog `{}`. Pick a scope.",
                            bad_q, top.entity, top.entry_id
                        ),
                        options: opts,
                        evidence: vec![DiscoveryEvidence::new(
                            evidence_codes::GRAPH_QUALIFIER_BAD,
                            bad_q.join(", "),
                        )],
                    },
                });
            }
        }

        let winner = hypotheses.into_iter().next().unwrap();
        metrics::record_option_count(1);
        metrics::record_request_outcome("ready");
        metrics::record_decision_duration(t_gate.elapsed());
        Ok(DiscoveryDecision::Ready {
            target: ReadyTarget {
                entry_id: winner.entry_id,
                entity: winner.entity,
                capability_name: winner.capability_name,
                capability_kind: winner.capability_kind,
                score: winner.score,
                matched_phrase: winner.matched_phrase,
                qualifiers: winner.qualifiers,
                evidence: winner.evidence,
            },
        })
    }
}

#[async_trait]
impl crate::AgentDiscovery for TypedDiscovery {
    async fn discover(&self, query: DiscoveryQuery) -> Result<DiscoveryDecision, DiscoveryError> {
        self.discover_inner(&query).await
    }

    async fn answer_clarification(
        &self,
        state: ClarificationState,
        answer: ClarificationAnswer,
    ) -> Result<DiscoveryDecision, DiscoveryError> {
        let opt = state
            .options
            .get(answer.selected_index)
            .ok_or(DiscoveryError::InvalidClarificationAnswer)?;
        let mut q = DiscoveryQuery {
            utterance: state.utterance.clone(),
            allowed_entry_ids: state.allowed_entry_ids.clone(),
            prior_state: None,
            max_options: state.max_options,
            enable_embeddings: state.enable_embeddings,
            force_entry_id: None,
            force_entity: None,
        };
        if let Some(e) = &opt.entry_id {
            q.force_entry_id = Some(e.clone());
        }
        if let Some(e) = &opt.entity {
            q.force_entity = Some(e.clone());
        }
        self.discover_inner(&q).await
    }
}
