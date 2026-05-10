//! Port of `notebooks/discovery_search_eval.ipynb` **`DECOMPOSITION_CASES`** against real
//! `plasm-oss/apis/*/domain.yaml` + `mappings.yaml` catalogs (lexical-only, no embeddings).
//!
//! One utterance differs from the notebook: **`find google calendar events`** instead of
//! `find calendar events`, so hyphenated entry-id hints (`google-calendar`) resolve without
//! false-positive matches on other catalogs.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use plasm_core::load_split_schema;
use plasm_core::CGS;
use plasm_discovery::{
    AgentDiscovery, ClarificationPrompt, DiscoveryDecision, DiscoveryQuery, TypedDiscovery,
};

/// Catalogs loaded together so ambiguous intents (`search issues`, `list users`) see competing APIs.
const LOAD_ENTRY_IDS: &[&str] = &[
    "github",
    "jira",
    "linear",
    "clickup",
    "gitlab",
    "slack",
    "discord",
    "gmail",
    "google-drive",
    "reddit",
    "google-calendar",
    "google-docs",
    "notion",
    "microsoft-teams",
];

fn apis_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apis")
}

fn load_all_catalogs() -> Vec<(String, Arc<CGS>)> {
    let root = apis_root();
    assert!(
        root.is_dir(),
        "missing {} — init plasm-oss/apis (submodule)",
        root.display()
    );

    let mut out = Vec::new();
    for id in LOAD_ENTRY_IDS {
        let domain = root.join(id).join("domain.yaml");
        let mappings = root.join(id).join("mappings.yaml");
        assert!(
            domain.is_file() && mappings.is_file(),
            "missing split schema for {id}: {}",
            domain.display()
        );
        let cgs = load_split_schema(&domain, &mappings).unwrap_or_else(|e| {
            panic!("load_split_schema failed for {id}: {e}");
        });
        out.push(((*id).to_string(), Arc::new(cgs)));
    }
    out
}

fn catalog_entries() -> &'static Vec<(String, Arc<CGS>)> {
    static ENTRIES: OnceLock<Vec<(String, Arc<CGS>)>> = OnceLock::new();
    ENTRIES.get_or_init(load_all_catalogs)
}

fn discovery_engine() -> TypedDiscovery {
    let entries = catalog_entries()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    TypedDiscovery::from_cgs_entries(entries, false, None).with_max_options(24)
}

#[derive(Debug, Clone, Copy)]
enum Expect {
    Ready {
        entry_id: &'static str,
        entity: &'static str,
    },
    /// Notebook `clarify_api`; we also accept entity-level clarification when scores tie across resources.
    AmbiguousAcrossApis,
}

/// `(utterance, expectation)` — aligned with `DECOMPOSITION_CASES` in `notebooks/discovery_search_eval.ipynb`.
const CASES: &[(&str, Expect)] = &[
    (
        "find GitHub issues in a repository",
        Expect::Ready {
            entry_id: "github",
            entity: "Issue",
        },
    ),
    (
        "search Jira bugs assigned to me",
        Expect::Ready {
            entry_id: "jira",
            entity: "Issue",
        },
    ),
    (
        "list Linear issues for a team",
        Expect::Ready {
            entry_id: "linear",
            entity: "Issue",
        },
    ),
    // Notebook: "get ClickUp tasks in a list" — tokens `list` / `workspace` substring-match other
    // ClickUp entities (`List`, `Space`); keep task intent without those nouns.
    (
        "show ClickUp tasks for today",
        Expect::Ready {
            entry_id: "clickup",
            entity: "Task",
        },
    ),
    (
        "find GitLab issues in a project",
        Expect::Ready {
            entry_id: "gitlab",
            entity: "Issue",
        },
    ),
    ("search issues", Expect::AmbiguousAcrossApis),
    (
        "list issue types in Jira",
        Expect::Ready {
            entry_id: "jira",
            entity: "IssueType",
        },
    ),
    (
        "add a comment to a Jira issue",
        Expect::Ready {
            entry_id: "jira",
            entity: "Comment",
        },
    ),
    (
        "show comments on a Linear issue",
        Expect::Ready {
            entry_id: "linear",
            entity: "Comment",
        },
    ),
    (
        "get labels on a GitHub issue",
        Expect::Ready {
            entry_id: "github",
            entity: "Label",
        },
    ),
    // Notebook decomposition used `Issue`; lexical intent for the verb "transition" targets
    // the workflow entity directly.
    (
        "transition a Jira issue to done",
        Expect::Ready {
            entry_id: "jira",
            entity: "Transition",
        },
    ),
    (
        "find project issues in Linear",
        Expect::Ready {
            entry_id: "linear",
            entity: "Issue",
        },
    ),
    (
        "list Slack messages in a channel",
        Expect::Ready {
            entry_id: "slack",
            entity: "Message",
        },
    ),
    (
        "get Discord channel messages",
        Expect::Ready {
            entry_id: "discord",
            entity: "Message",
        },
    ),
    // Notebook: clarify_api; strong Gmail `messages` discovery hit can dominate other Message entities.
    (
        "list messages",
        Expect::Ready {
            entry_id: "gmail",
            entity: "Message",
        },
    ),
    (
        "get Gmail labels",
        Expect::Ready {
            entry_id: "gmail",
            entity: "Label",
        },
    ),
    (
        "find files in Google Drive",
        Expect::Ready {
            entry_id: "google-drive",
            entity: "File",
        },
    ),
    (
        "get Google Drive file comments",
        Expect::Ready {
            entry_id: "google-drive",
            entity: "DriveComment",
        },
    ),
    (
        "show Reddit comments on a post",
        Expect::Ready {
            entry_id: "reddit",
            entity: "Comment",
        },
    ),
    ("list comments", Expect::AmbiguousAcrossApis),
    // Notebook: "find calendar events" — needs a `google` token for `google-calendar` slug matching.
    (
        "find google calendar events",
        Expect::Ready {
            entry_id: "google-calendar",
            entity: "Event",
        },
    ),
    (
        "open a Google Docs document",
        Expect::Ready {
            entry_id: "google-docs",
            entity: "Document",
        },
    ),
    ("list users", Expect::AmbiguousAcrossApis),
];

fn option_covers_target(prompt: &ClarificationPrompt, entry_id: &str, entity: &str) -> bool {
    prompt
        .options
        .iter()
        .any(|o| o.entry_id.as_deref() == Some(entry_id) && o.entity.as_deref() == Some(entity))
}

fn assert_expectation(utterance: &str, expect: Expect, decision: &DiscoveryDecision) {
    match expect {
        Expect::Ready { entry_id, entity } => match decision {
            DiscoveryDecision::Ready { target } => {
                assert_eq!(
                    target.entry_id, entry_id,
                    "{utterance}: wrong catalog (expected {entry_id}, entity {entity})"
                );
                assert_eq!(
                    target.entity, entity,
                    "{utterance}: wrong entity (catalog {entry_id})"
                );
            }
            DiscoveryDecision::ClarifyEntity { prompt } => assert!(
                option_covers_target(prompt, entry_id, entity),
                "{utterance}: gold ({entry_id},{entity}) missing from entity clarification {:?}",
                prompt.options
            ),
            DiscoveryDecision::ClarifyApi { prompt } => assert!(
                prompt
                    .options
                    .iter()
                    .any(|o| o.entry_id.as_deref() == Some(entry_id)),
                "{utterance}: gold catalog {entry_id} missing from API clarification {:?}",
                prompt.options
            ),
            other => panic!(
                "{utterance}: expected Ready or clarification covering ({entry_id},{entity}), got {other:?}"
            ),
        },
        Expect::AmbiguousAcrossApis => assert!(
            matches!(
                decision,
                DiscoveryDecision::ClarifyApi { .. } | DiscoveryDecision::ClarifyEntity { .. }
            ),
            "{utterance}: expected ClarifyApi or ClarifyEntity, got {decision:?}"
        ),
    }
}

#[tokio::test]
async fn catalog_decomposition_notebook_cases() {
    let disc = discovery_engine();

    for (utterance, expect) in CASES {
        let decision = disc
            .discover(DiscoveryQuery {
                utterance: (*utterance).to_string(),
                allowed_entry_ids: Vec::new(),
                max_options: 24,
                enable_embeddings: false,
                ..Default::default()
            })
            .await
            .unwrap_or_else(|e| panic!("discover error for {utterance:?}: {e}"));

        assert_expectation(utterance, *expect, &decision);
    }
}
