//! Postfix transforms on Plasm surface expressions (`.limit`, `.sort`, projections, etc.).
//!
//! This module **only** splits a string into a primary fragment and a left-to-right sequence of
//! postfix operations. It does not parse entity/query syntax — that remains [`super::parse`].
//!
//! **Bracket render** (`source[field,...] <<TAG … TAG`) is recognized by [`try_parse_bracket_render`]
//! using delimiter depth so `<<` inside calls/parens is not mistaken for a render opener.

/// Postfix operations peeled from the right of an expression, in **application order**
/// (index `0` applies first to the primary, then `1`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlasmPostfixOp {
    Singleton,
    PageSize(usize),
    Limit(usize),
    Sort { args: String },
    Aggregate { args: String },
    GroupBy { args: String },
    Projection { fields: String },
}

/// `source[field,...] <<TAG` … `TAG` render surface (program DAG lowering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BracketRender {
    pub source: String,
    /// Comma-separated field names from `[…]` (trimmed segments).
    pub fields: String,
    pub template: String,
}

/// `ident[field,...]` with no trailing junk after `]` (same contract as program DAG `parse_projection`).
fn parse_projection_head(head: &str) -> Option<(&str, &str)> {
    let head = head.trim_end();
    let open = head.rfind('[')?;
    if !head.ends_with(']') {
        return None;
    }
    Some((head[..open].trim_end(), &head[open + 1..head.len() - 1]))
}

fn parse_render_template_after_tag_opener(rest: &str) -> Result<String, String> {
    let mut lines = rest.lines();
    let tag = lines
        .next()
        .map(str::trim)
        .ok_or_else(|| "render heredoc missing tag".to_string())?;
    if tag.is_empty() {
        return Err("render heredoc missing tag".to_string());
    }
    let body = lines.collect::<Vec<_>>().join("\n");
    let end = body
        .rfind(&format!("\n{tag}"))
        .or_else(|| (body.trim() == tag).then_some(0))
        .ok_or_else(|| format!("render heredoc <<{tag} is not closed"))?;
    Ok(if end == 0 {
        String::new()
    } else {
        body[..end].to_string()
    })
}

/// If `rhs` is `source[field,...] <<TAG` … `TAG` at **delimiter depth 0** (so `<<` inside `method(…)`
/// is not treated as render), return the parts. Otherwise `Ok(None)`.
///
/// Chooses the **rightmost** `<<` at depth 0 that yields a valid projection head and closed template.
pub fn try_parse_bracket_render(rhs: &str) -> Result<Option<BracketRender>, String> {
    let mut positions: Vec<usize> = rhs.match_indices("<<").map(|(i, _)| i).collect();
    if positions.is_empty() {
        return Ok(None);
    }
    positions.sort_unstable_by_key(|&i| std::cmp::Reverse(i));

    for j in positions {
        if delimiter_depth_before(rhs, j) != 0 {
            continue;
        }
        let head = rhs[..j].trim_end();
        let Some((source, fields)) = parse_projection_head(head) else {
            continue;
        };
        let rest = &rhs[j + 2..];
        let template = parse_render_template_after_tag_opener(rest)?;
        return Ok(Some(BracketRender {
            source: source.to_string(),
            fields: fields.to_string(),
            template,
        }));
    }
    Ok(None)
}

/// Peel trailing postfix operators from `rhs`, returning `(primary, ops)`.
///
/// `ops` is ordered **inner → outer** (first apply `ops[0]` to `primary`, then `ops[1]`, …).
pub fn peel_postfix_suffixes(rhs: &str) -> Result<(String, Vec<PlasmPostfixOp>), String> {
    let mut cur = rhs.trim().to_string();
    let mut ops_rev: Vec<PlasmPostfixOp> = Vec::new();

    loop {
        let t = cur.trim();
        if t.is_empty() {
            return Err("empty expression after peeling postfix operators".into());
        }

        let mut progressed = false;

        if let Some(p) = strip_suffix_singleton(t) {
            ops_rev.push(PlasmPostfixOp::Singleton);
            cur = p;
            progressed = true;
        } else if let Some((p, n)) = strip_trailing_unary_int_call(t, "page_size")? {
            ops_rev.push(PlasmPostfixOp::PageSize(n));
            cur = p;
            progressed = true;
        } else if let Some((p, n)) = strip_trailing_unary_int_call(t, "limit")? {
            ops_rev.push(PlasmPostfixOp::Limit(n));
            cur = p;
            progressed = true;
        } else if let Some((p, args)) = strip_trailing_method_call(t, "sort")? {
            ops_rev.push(PlasmPostfixOp::Sort { args });
            cur = p;
            progressed = true;
        } else if let Some((p, args)) = strip_trailing_method_call(t, "aggregate")? {
            ops_rev.push(PlasmPostfixOp::Aggregate { args });
            cur = p;
            progressed = true;
        } else if let Some((p, args)) = strip_trailing_method_call(t, "group_by")? {
            ops_rev.push(PlasmPostfixOp::GroupBy { args });
            cur = p;
            progressed = true;
        } else if let Some((p, fields)) = strip_trailing_projection(t)? {
            ops_rev.push(PlasmPostfixOp::Projection { fields });
            cur = p;
            progressed = true;
        }

        if !progressed {
            break;
        }
    }

    let mut ops: Vec<PlasmPostfixOp> = ops_rev;
    ops.reverse();
    Ok((cur.trim().to_string(), ops))
}

fn strip_suffix_singleton(s: &str) -> Option<String> {
    let t = s.trim_end();
    let suf = ".singleton()";
    t.strip_suffix(suf).map(|p| p.trim_end().to_string())
}

/// Strips a trailing `.name(integer)` call at paren depth 0.
fn strip_trailing_unary_int_call(s: &str, name: &str) -> Result<Option<(String, usize)>, String> {
    let Some((prefix, args)) = strip_trailing_method_call(s, name)? else {
        return Ok(None);
    };
    let n = args
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("{name}(...) requires a positive integer"))?;
    if n == 0 {
        return Err(format!("{name}(...) requires a positive integer"));
    }
    Ok(Some((prefix, n)))
}

/// Finds the **last** `.name(` at delimiter depth 0 whose closing `)` ends the string.
fn strip_trailing_method_call(s: &str, name: &str) -> Result<Option<(String, String)>, String> {
    let needle = format!(".{name}(");
    let mut search_end = s.len();
    while search_end > 0 {
        let slice = &s[..search_end];
        let Some(pos) = slice.rfind(&needle) else {
            return Ok(None);
        };
        if delimiter_depth_before(s, pos) != 0 {
            search_end = pos;
            continue;
        }
        let open_paren = pos + needle.len() - 1;
        let close = matching_paren_close(s, open_paren)?;
        if close + 1 != s.len() {
            search_end = pos;
            continue;
        }
        let args = s[open_paren + 1..close].to_string();
        return Ok(Some((s[..pos].trim_end().to_string(), args)));
    }
    Ok(None)
}

fn strip_trailing_projection(s: &str) -> Result<Option<(String, String)>, String> {
    let t = s.trim_end();
    if !t.ends_with(']') {
        return Ok(None);
    }
    let Some(open) = t[..t.len() - 1].rfind('[') else {
        return Ok(None);
    };
    let fields = t[open + 1..t.len() - 1].to_string();
    Ok(Some((t[..open].trim_end().to_string(), fields)))
}

fn delimiter_depth_before(s: &str, end: usize) -> i32 {
    let mut depth = 0i32;
    let mut quote = None::<char>;
    for (idx, c) in s.char_indices() {
        if idx >= end {
            break;
        }
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            _ if quote.is_some() => {}
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn matching_paren_close(s: &str, open_idx: usize) -> Result<usize, String> {
    let mut depth = 1i32;
    let mut quote = None::<char>;
    for (idx, c) in s.char_indices().skip(open_idx + 1) {
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            _ if quote.is_some() => {}
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(idx);
                }
            }
            _ => {}
        }
    }
    Err("unbalanced parentheses in method call".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peel_limit_on_surface_primary() {
        let (p, ops) =
            peel_postfix_suffixes("e1{p4=e2(owner=\"x\", repo=\"y\")}.limit(20)").unwrap();
        assert_eq!(p, "e1{p4=e2(owner=\"x\", repo=\"y\")}");
        assert_eq!(ops, vec![PlasmPostfixOp::Limit(20)]);
    }

    #[test]
    fn peel_limit_then_sort() {
        let (p, ops) = peel_postfix_suffixes("a.limit(5).sort(x, desc)").unwrap();
        assert_eq!(p, "a");
        assert_eq!(
            ops,
            vec![
                PlasmPostfixOp::Limit(5),
                PlasmPostfixOp::Sort {
                    args: "x, desc".into()
                }
            ]
        );
    }

    #[test]
    fn peel_sort_then_limit() {
        let (p, ops) = peel_postfix_suffixes("a.sort(x).limit(3)").unwrap();
        assert_eq!(p, "a");
        assert_eq!(
            ops,
            vec![
                PlasmPostfixOp::Sort { args: "x".into() },
                PlasmPostfixOp::Limit(3),
            ]
        );
    }

    #[test]
    fn peel_projection_and_limit() {
        let (p, ops) = peel_postfix_suffixes("e1{}.limit(10)[sha,message]").unwrap();
        assert_eq!(p, "e1{}");
        assert_eq!(
            ops,
            vec![
                PlasmPostfixOp::Limit(10),
                PlasmPostfixOp::Projection {
                    fields: "sha,message".into()
                },
            ]
        );
    }

    #[test]
    fn bracket_render_parses_at_depth_zero() {
        let r = try_parse_bracket_render("repo[p1,p2]<<T\nline one\nT").unwrap();
        let br = r.expect("bracket render");
        assert_eq!(br.source, "repo");
        assert_eq!(br.fields, "p1,p2");
        assert_eq!(br.template, "line one");
    }

    #[test]
    fn bracket_render_ignores_heredoc_inside_parens() {
        let r = try_parse_bracket_render("e3.m12(p76=<<M\nx\nM)").unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn bracket_render_err_when_projection_ok_but_heredoc_unclosed() {
        let e = try_parse_bracket_render("row[a,b]<<H\nno close").unwrap_err();
        assert!(
            e.contains("not closed") || e.contains("<<H"),
            "unexpected err: {e}"
        );
    }
}
