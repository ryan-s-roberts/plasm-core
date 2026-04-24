//! Session-scoped compaction for MCP `plasm` tool `_meta.plasm` (dict_ref + index_delta).

use crate::output::LossySummaryFieldNames;
use crate::run_artifacts::RunArtifactHandle;
use plasm_core::PagingHandle;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const MIME_JSON: &str = "application/json";
/// Default `resource_link` / dictionary description for execute run snapshots.
pub const DESC_RUN_SNAPSHOT: &str = "Plasm execute run snapshot (application/json)";

/// Compact paging follow-up for `_meta.plasm` (opaque `page(pg#)` handles; session-scoped).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlasmPagingStepMeta {
    Next {
        /// 1-based index within the `expressions` batch (always `1` for a single-line `plasm` call).
        batch_step: usize,
        returned_count: usize,
        next_page_handle: PagingHandle,
    },
}

pub(crate) fn plasm_paging_json_value(paging: &[PlasmPagingStepMeta]) -> Option<Value> {
    let arr: Vec<Value> = paging
        .iter()
        .map(|p| match p {
            PlasmPagingStepMeta::Next {
                batch_step,
                returned_count,
                next_page_handle,
            } => {
                json!({
                    "batch_step": batch_step,
                    "has_more": true,
                    "count": returned_count,
                    "next_page_handle": next_page_handle.as_str(),
                })
            }
        })
        .collect();
    if arr.is_empty() {
        None
    } else {
        Some(Value::Array(arr))
    }
}

/// Per MCP transport session + execute session: intern repeated `_meta.plasm` strings and fingerprint lists.
#[derive(Debug)]
pub struct PlasmMetaIndex {
    /// Monotonic generation; bumped on each successful compact build.
    pub index_id: u64,
    mime: HashMap<String, u32>,
    desc: HashMap<String, u32>,
    path: HashMap<String, u32>,
    fp: HashMap<FpKey, u32>,
    next_mime: u32,
    next_desc: u32,
    next_path: u32,
    next_fp: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FpKey(Vec<String>);

impl PlasmMetaIndex {
    pub fn new() -> Self {
        Self {
            index_id: 1,
            mime: HashMap::new(),
            desc: HashMap::new(),
            path: HashMap::new(),
            fp: HashMap::new(),
            next_mime: 1,
            next_desc: 1,
            next_path: 1,
            next_fp: 1,
        }
    }

    fn intern_map(
        map: &mut HashMap<String, u32>,
        next: &mut u32,
        s: &str,
        delta: &mut Map<String, Value>,
        key: &str,
    ) -> u32 {
        if let Some(&id) = map.get(s) {
            return id;
        }
        let id = *next;
        *next = next.saturating_add(1);
        map.insert(s.to_string(), id);
        let obj = delta
            .entry(key.to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("object");
        obj.insert(id.to_string(), json!(s));
        id
    }

    fn intern_fp(&mut self, fps: &[String], delta: &mut Map<String, Value>) -> u32 {
        let key = FpKey(fps.to_vec());
        if let Some(&id) = self.fp.get(&key) {
            return id;
        }
        let id = self.next_fp;
        self.next_fp = self.next_fp.saturating_add(1);
        self.fp.insert(key.clone(), id);
        let obj = delta
            .entry("fp".to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("object");
        obj.insert(id.to_string(), json!(fps));
        id
    }

    /// Build `_meta.plasm` with `index_delta`, compact `steps`, and optional `omitted_from_summary`.
    /// Returns `(plasm_meta, desc_id_per_handle)` (desc ids are unused when no `resource_link` rows).
    ///
    /// When `batch_steps` is `Some`, it must be the same length as `handles`; each value is the
    /// 1-based batch step index for that snapshot (only truncated steps are included upstream).
    ///
    /// When `lossy_per_step` is `Some`, it must be the same length as `handles`; each entry lists
    /// field names summarized with a lossy cap for that step (same shape as non-compact `plasm` meta).
    pub(crate) fn build_plasm_meta(
        &mut self,
        handles: &[RunArtifactHandle],
        omitted_from_summary: &[String],
        lossy_per_step: Option<&[LossySummaryFieldNames]>,
        expr_previews: &[String],
        batch_steps: Option<&[usize]>,
        paging: Option<&[PlasmPagingStepMeta]>,
    ) -> (Map<String, Value>, Vec<u32>) {
        self.index_id = self.index_id.saturating_add(1);
        let mut delta = Map::new();
        let mut steps = Vec::with_capacity(handles.len());
        let mut desc_ids = Vec::with_capacity(handles.len());

        for (i, h) in handles.iter().enumerate() {
            let preview = expr_previews
                .get(i)
                .map(String::as_str)
                .unwrap_or("")
                .to_string();

            let mime_id = Self::intern_map(
                &mut self.mime,
                &mut self.next_mime,
                MIME_JSON,
                &mut delta,
                "mime",
            );
            let desc_id = Self::intern_map(
                &mut self.desc,
                &mut self.next_desc,
                DESC_RUN_SNAPSHOT,
                &mut delta,
                "desc",
            );
            desc_ids.push(desc_id);
            let path_id = Self::intern_map(
                &mut self.path,
                &mut self.next_path,
                &h.http_path,
                &mut delta,
                "artifact_path",
            );
            let fp_id = self.intern_fp(&h.request_fingerprints, &mut delta);

            let mut step = Map::new();
            step.insert("run_id".into(), json!(h.run_id.to_string()));
            step.insert("artifact_uri".into(), json!(h.plasm_uri));
            step.insert(
                "dict_ref".into(),
                json!({
                    "mime": mime_id,
                    "desc": desc_id,
                    "artifact_path": path_id,
                    "fp": fp_id,
                }),
            );
            step.insert("expr_preview".into(), json!(preview));
            if let Some(bs) = batch_steps {
                if let Some(&batch_step) = bs.get(i) {
                    step.insert("batch_step".into(), json!(batch_step));
                }
            }
            if let Some(ls) = lossy_per_step {
                if let Some(lossy) = ls.get(i) {
                    if !lossy.is_empty() {
                        step.insert("lossy_summary_fields".into(), json!(lossy.as_slice()));
                    }
                }
            }
            steps.push(Value::Object(step));
        }

        let mut plasm = Map::new();
        plasm.insert("index_id".into(), json!(self.index_id));
        if !delta.is_empty() {
            plasm.insert("index_delta".into(), Value::Object(delta));
        }
        if !steps.is_empty() {
            plasm.insert("steps".into(), Value::Array(steps));
        }
        if !omitted_from_summary.is_empty() {
            plasm.insert("omitted_from_summary".into(), json!(omitted_from_summary));
        }
        if let Some(ps) = paging {
            if let Some(v) = plasm_paging_json_value(ps) {
                plasm.insert("paging".into(), v);
            }
        }
        (plasm, desc_ids)
    }

    /// Short description for `resource_link` pointing at dictionary entry.
    pub fn resource_link_description(desc_id: u32) -> String {
        format!("Run snapshot (ref: desc#{desc_id})")
    }
}

impl Default for PlasmMetaIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::LossySummaryFieldNames;
    use crate::run_artifacts::{
        artifact_http_path, plasm_run_resource_uri, plasm_short_resource_uri,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn sample_handle(run: Uuid, ph: &str, sid: &str) -> RunArtifactHandle {
        RunArtifactHandle {
            run_id: run,
            plasm_uri: plasm_short_resource_uri(1),
            canonical_plasm_uri: plasm_run_resource_uri(ph, sid, &run),
            http_path: artifact_http_path(ph, sid, &run),
            payload_len: 128,
            request_fingerprints: vec!["deadbeef".into()],
        }
    }

    #[test]
    fn second_build_emits_smaller_index_delta_for_same_paths() {
        let id = Uuid::nil();
        let ph = "ab".repeat(32);
        let sid = "a".repeat(32);
        let h = sample_handle(id, &ph, &sid);
        let mut idx = PlasmMetaIndex::new();
        let (m1, _) = idx.build_plasm_meta(
            std::slice::from_ref(&h),
            &[],
            None,
            &["".into()],
            None,
            None,
        );
        let delta1 = m1
            .get("index_delta")
            .and_then(|v| v.as_object())
            .map(|o| serde_json::to_string(o).unwrap().len())
            .unwrap_or(0);

        let (m2, _) = idx.build_plasm_meta(&[h], &[], None, &["".into()], None, None);
        let delta2 = m2
            .get("index_delta")
            .and_then(|v| v.as_object())
            .map(|o| o.len())
            .unwrap_or(0);

        assert!(delta1 > 0, "first delta should define dictionaries");
        assert_eq!(
            delta2, 0,
            "second call with same paths should emit empty index_delta"
        );
    }

    #[test]
    fn steps_contain_dict_ref_and_run_ids() {
        let id = Uuid::nil();
        let ph = "ab".repeat(32);
        let sid = "a".repeat(32);
        let h = sample_handle(id, &ph, &sid);
        let mut idx = PlasmMetaIndex::new();
        let (plasm, desc_ids) = idx.build_plasm_meta(
            &[h],
            &[],
            None,
            &["Pet.query()".into()],
            Some(std::slice::from_ref(&1)),
            None,
        );
        let steps = plasm
            .get("steps")
            .and_then(|v| v.as_array())
            .expect("steps");
        assert_eq!(steps.len(), 1);
        let step = steps[0].as_object().expect("step object");
        assert!(step.contains_key("dict_ref"));
        assert_eq!(step.get("run_id"), Some(&json!(id.to_string())));
        assert_eq!(step.get("batch_step"), Some(&json!(1)));
        assert_eq!(desc_ids.len(), 1);
    }

    #[test]
    fn step_includes_lossy_summary_fields_when_provided() {
        let id = Uuid::nil();
        let ph = "ab".repeat(32);
        let sid = "a".repeat(32);
        let h = sample_handle(id, &ph, &sid);
        let lossy = LossySummaryFieldNames::from_vec_sorted_dedup(vec!["desc".into()]);
        let mut idx = PlasmMetaIndex::new();
        let (plasm, _) = idx.build_plasm_meta(
            std::slice::from_ref(&h),
            &[],
            Some(std::slice::from_ref(&lossy)),
            &["".into()],
            None,
            None,
        );
        let step = plasm
            .get("steps")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_object())
            .expect("step");
        assert_eq!(step.get("lossy_summary_fields"), Some(&json!(["desc"])));
    }

    #[test]
    fn plasm_meta_includes_paging_when_has_more() {
        let id = Uuid::nil();
        let ph = "ab".repeat(32);
        let sid = "a".repeat(32);
        let h = sample_handle(id, &ph, &sid);
        let mut idx = PlasmMetaIndex::new();
        let paging = [PlasmPagingStepMeta::Next {
            batch_step: 1,
            returned_count: 20,
            next_page_handle: PagingHandle::mint_namespaced("s0", 1),
        }];
        let (plasm, _) = idx.build_plasm_meta(
            std::slice::from_ref(&h),
            &[],
            None,
            &["".into()],
            None,
            Some(&paging),
        );
        let arr = plasm
            .get("paging")
            .and_then(|v| v.as_array())
            .expect("paging");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["next_page_handle"], json!("s0_pg1"));
        assert_eq!(arr[0]["count"], json!(20));
    }
}
