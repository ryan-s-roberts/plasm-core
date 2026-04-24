//! Gmail `users.messages.send` body: RFC 5322 message bytes → URL-safe base64 `raw` JSON field.

use crate::cml::CmlEnv;
use crate::error::CmlError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use indexmap::IndexMap;
use plasm_core::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Reply send: requires preflight `parent_*` keys plus user `from` / `plainBody`.
pub(crate) fn eval_gmail_rfc5322_reply_send_body(env: &CmlEnv) -> Result<Value, CmlError> {
    let from = require_str(
        env,
        "from",
        "Gmail reply: `from` must be a non-empty string",
    )?;
    let plain_body = require_str(
        env,
        "plainBody",
        "Gmail reply: `plainBody` must be a non-empty string",
    )?;

    let to = if let Some(t) = optional_nonempty_str(env, "to") {
        t
    } else {
        let reply_to = optional_nonempty_str(env, "parent_headerReplyTo");
        let from_hdr = optional_nonempty_str(env, "parent_headerFrom");
        let addr = reply_to
            .as_ref()
            .and_then(|s| extract_first_mailbox(s))
            .or_else(|| from_hdr.as_ref().and_then(|s| extract_first_mailbox(s)))
            .ok_or_else(|| CmlError::InvalidTemplate {
                message: "Gmail reply: need `to` or parent From/Reply-To with a parseable address"
                    .to_string(),
            })?;
        addr
    };

    let subject = if let Some(s) = optional_nonempty_str(env, "subject") {
        s
    } else {
        let parent_sub = optional_nonempty_str(env, "parent_headerSubject").unwrap_or_default();
        ensure_re_subject(&parent_sub)
    };

    let parent_msg_id = optional_nonempty_str(env, "parent_headerMessageId").ok_or_else(|| {
        CmlError::InvalidTemplate {
            message: "Gmail reply: preflight must set `parent_headerMessageId` (Message-Id)"
                .to_string(),
        }
    })?;

    let references = {
        let refs = optional_nonempty_str(env, "parent_headerReferences");
        build_references_chain(refs.as_deref(), &parent_msg_id)
    };

    let mut inner: CmlEnv = IndexMap::new();
    inner.insert("from".into(), Value::String(from));
    inner.insert("to".into(), Value::String(to));
    inner.insert("subject".into(), Value::String(subject));
    inner.insert("plainBody".into(), Value::String(plain_body));
    inner.insert("inReplyTo".into(), Value::String(parent_msg_id));
    inner.insert("references".into(), Value::String(references));

    if let Some(tid) = value_as_nonempty_string(env.get("parent_threadId")) {
        inner.insert("threadId".into(), Value::String(tid));
    }

    eval_gmail_rfc5322_send_body(&inner)
}

fn value_as_nonempty_string(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Integer(i)) => Some(i.to_string()),
        _ => None,
    }
}

/// Prefer `<addr>` in RFC 5322 display; else first token that looks like an email.
fn extract_first_mailbox(s: &str) -> Option<String> {
    let s = s.trim();
    if let (Some(l), Some(r)) = (s.rfind('<'), s.rfind('>')) {
        if l < r {
            let inner = s[l + 1..r].trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    let first = s.split_whitespace().next()?;
    if first.contains('@') && !first.contains('<') {
        return Some(first.to_string());
    }
    None
}

fn ensure_re_subject(parent: &str) -> String {
    let t = parent.trim();
    if t.is_empty() {
        return "Re:".to_string();
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("re:") {
        t.to_string()
    } else {
        format!("Re: {t}")
    }
}

fn build_references_chain(parent_refs: Option<&str>, parent_message_id: &str) -> String {
    let mid = parent_message_id.trim();
    match parent_refs {
        Some(r) if !r.trim().is_empty() => format!("{} {}", r.trim(), mid),
        _ => mid.to_string(),
    }
}

/// Build `{ "raw": "<base64url>", "threadId": "..."? }` for Gmail from CML env keys:
/// required: `from`, `to`, `subject`, `plainBody`; optional: `threadId`, `inReplyTo`, `references`.
pub(crate) fn eval_gmail_rfc5322_send_body(env: &CmlEnv) -> Result<Value, CmlError> {
    let from = require_str(env, "from", "Gmail send: `from` must be a non-empty string")?;
    let to = require_str(env, "to", "Gmail send: `to` must be a non-empty string")?;
    let subject = require_str(
        env,
        "subject",
        "Gmail send: `subject` must be a non-empty string",
    )?;
    let plain_body = require_str(
        env,
        "plainBody",
        "Gmail send: `plainBody` must be a non-empty string",
    )?;

    let in_reply_to = optional_nonempty_str(env, "inReplyTo");
    let references = optional_nonempty_str(env, "references");

    let date = Utc::now().to_rfc2822();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let message_id = format!("<plasm.{nanos}@plasm.invalid>");

    let mut headers: Vec<String> = vec![
        format!("Message-ID: {message_id}"),
        format!("Date: {date}"),
        format!("From: {}", fold_header_value(&from)?),
        format!("To: {}", fold_header_value(&to)?),
        format!("Subject: {}", fold_header_value(&subject)?),
        "MIME-Version: 1.0".to_string(),
        r#"Content-Type: text/plain; charset="UTF-8""#.to_string(),
        "Content-Transfer-Encoding: 8bit".to_string(),
    ];
    if let Some(ref s) = in_reply_to {
        headers.push(format!("In-Reply-To: {}", fold_header_value(s)?));
    }
    if let Some(ref s) = references {
        headers.push(format!("References: {}", fold_header_value(s)?));
    }

    let header_block = headers.join("\r\n");
    let body_normalized = normalize_body_crlf(&plain_body);
    let rfc5322 = format!("{header_block}\r\n\r\n{body_normalized}");
    let bytes = rfc5322.into_bytes();
    let raw_b64 = URL_SAFE_NO_PAD.encode(&bytes);

    let mut out = IndexMap::new();
    out.insert("raw".to_string(), Value::String(raw_b64));

    if let Some(tid) = optional_nonempty_str(env, "threadId") {
        out.insert("threadId".to_string(), Value::String(tid));
    }

    Ok(Value::Object(out))
}

fn fold_header_value(s: &str) -> Result<String, CmlError> {
    if s.contains('\r') || s.contains('\n') {
        return Err(CmlError::InvalidTemplate {
            message: "Gmail send: header values must not contain CR or LF".to_string(),
        });
    }
    Ok(s.to_string())
}

fn normalize_body_crlf(s: &str) -> String {
    if !s.contains('\r') {
        return s.replace('\n', "\r\n");
    }
    // Already has CR — normalize CRLF pairs
    s.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

fn require_str(env: &CmlEnv, key: &str, err: &'static str) -> Result<String, CmlError> {
    match env.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(Value::String(_)) | None => Err(CmlError::TypeError {
            message: err.to_string(),
        }),
        Some(other) => Err(CmlError::TypeError {
            message: format!(
                "Gmail send: `{key}` must be a string, got {}",
                other.type_name()
            ),
        }),
    }
}

fn optional_nonempty_str(env: &CmlEnv, key: &str) -> Option<String> {
    match env.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    #[test]
    fn gmail_reply_send_body_uses_parent_headers() {
        let mut env = IndexMap::new();
        env.insert("from".into(), Value::String("me@example.com".into()));
        env.insert("plainBody".into(), Value::String("Thanks!".into()));
        env.insert(
            "parent_headerFrom".into(),
            Value::String("Bob <bob@other.com>".into()),
        );
        env.insert(
            "parent_headerSubject".into(),
            Value::String("Question".into()),
        );
        env.insert(
            "parent_headerMessageId".into(),
            Value::String("<abc@other.com>".into()),
        );
        env.insert(
            "parent_headerReferences".into(),
            Value::String("<older@x.com>".into()),
        );
        env.insert("parent_threadId".into(), Value::String("t1".into()));

        let v = eval_gmail_rfc5322_reply_send_body(&env).unwrap();
        let Value::Object(obj) = v else {
            panic!("expected object");
        };
        let Value::String(raw) = obj.get("raw").unwrap() else {
            panic!("raw");
        };
        let decoded = URL_SAFE_NO_PAD.decode(raw.as_bytes()).unwrap();
        let text = String::from_utf8(decoded).unwrap();
        assert!(text.contains("To: bob@other.com"));
        assert!(text.contains("Subject: Re: Question"));
        assert!(text.contains("In-Reply-To: <abc@other.com>"));
        assert!(text.contains("References: <older@x.com> <abc@other.com>"));
        assert_eq!(obj.get("threadId"), Some(&Value::String("t1".into())));
    }

    #[test]
    fn gmail_send_body_decodes_to_headers_and_text() {
        let mut env = IndexMap::new();
        env.insert("from".into(), Value::String("a@example.com".into()));
        env.insert("to".into(), Value::String("b@example.com".into()));
        env.insert("subject".into(), Value::String("Hi".into()));
        env.insert("plainBody".into(), Value::String("Line1\nLine2".into()));
        env.insert("threadId".into(), Value::String("thread-xyz".into()));

        let v = eval_gmail_rfc5322_send_body(&env).unwrap();
        let Value::Object(obj) = v else {
            panic!("expected object");
        };
        let Value::String(raw) = obj.get("raw").unwrap() else {
            panic!("raw");
        };
        assert_eq!(
            obj.get("threadId").unwrap(),
            &Value::String("thread-xyz".into())
        );

        let decoded = URL_SAFE_NO_PAD.decode(raw.as_bytes()).unwrap();
        let text = String::from_utf8(decoded).unwrap();
        assert!(text.contains("From: a@example.com"));
        assert!(text.contains("To: b@example.com"));
        assert!(text.contains("Subject: Hi"));
        assert!(text.contains("Line1\r\nLine2") || text.ends_with("Line1\r\nLine2\r\n"));
    }
}
