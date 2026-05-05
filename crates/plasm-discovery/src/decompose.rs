//! Utterance → operation hints, API hints, and noun phrases.

use crate::types::DecomposedIntent;

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "and", "or", "with", "from", "by", "at", "is",
    "are", "was", "were", "be", "been", "being", "it", "this", "that", "these", "those", "me",
    "my", "your", "our", "their", "i", "you", "we", "they", "as", "about", "into", "over",
];

const VERBS: &[&str] = &[
    "list", "show", "get", "fetch", "find", "search", "query", "create", "update", "delete",
    "open", "pull", "send", "post",
];

/// Tokenize on non-alphanumeric boundaries (ASCII-centric; sufficient for catalog NL tests).
pub fn tokenize(lower_utterance: &str) -> Vec<String> {
    lower_utterance
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// True when `lower` (already lowercased) mentions this catalog `entry_id`.
///
/// Matches the literal id as a substring, or — for hyphenated ids such as `google-calendar` —
/// requires every hyphen segment to appear as a substring (so "google calendar …" matches
/// `google-calendar` without embedding the exact slug).
fn entry_id_matches_utterance(entry_id: &str, lower: &str) -> bool {
    let id_l = entry_id.to_lowercase();
    if lower.contains(&id_l) {
        return true;
    }
    if id_l.contains('-') {
        let parts: Vec<&str> = id_l.split('-').filter(|p| !p.is_empty()).collect();
        if parts.len() >= 2 {
            return parts.iter().all(|p| lower.contains(p));
        }
    }
    false
}

pub fn decompose(utterance: &str, catalog_entry_ids: &[String]) -> DecomposedIntent {
    let lower = utterance.trim().to_lowercase();
    let tokens = tokenize(&lower);

    let mut api_hints = Vec::new();
    for id in catalog_entry_ids {
        if entry_id_matches_utterance(id, &lower) {
            api_hints.push(id.clone());
        }
    }

    let mut operation_verbs = Vec::new();
    for t in &tokens {
        if VERBS.contains(&t.as_str()) {
            operation_verbs.push(t.clone());
        }
    }

    let noun_phrases = extract_noun_phrases(&tokens);

    DecomposedIntent {
        operation_verbs,
        api_hints,
        noun_phrases,
    }
}

fn extract_noun_phrases(tokens: &[String]) -> Vec<String> {
    let filtered: Vec<&str> = tokens
        .iter()
        .map(|s| s.as_str())
        .filter(|t| !STOPWORDS.contains(t) && !VERBS.contains(t))
        .collect();

    let mut out = Vec::new();
    // Unigrams / bigrams / trigrams of remaining tokens (dedup).
    let n = filtered.len();
    for i in 0..n {
        for len in 1..=3usize {
            if i + len > n {
                break;
            }
            let slice = &filtered[i..i + len];
            let phrase = slice.join(" ");
            if phrase.len() >= 2 && !out.contains(&phrase) {
                out.push(phrase);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_api_hint_and_noun_phrases() {
        let d = decompose(
            "List GitHub issues for my org",
            &["github".to_string(), "slack".to_string()],
        );
        assert!(d.api_hints.contains(&"github".to_string()));
        assert!(d.noun_phrases.iter().any(|p| p.contains("issue")));
    }

    #[test]
    fn hyphenated_entry_id_matches_segment_substrings() {
        let ids = vec![
            "google-calendar".to_string(),
            "google-docs".to_string(),
            "github".to_string(),
        ];
        let d = decompose("find google calendar events this week", &ids);
        assert_eq!(d.api_hints, vec!["google-calendar".to_string()]);
        let d2 = decompose("open a google docs document", &ids);
        assert_eq!(d2.api_hints, vec!["google-docs".to_string()]);
    }
}
