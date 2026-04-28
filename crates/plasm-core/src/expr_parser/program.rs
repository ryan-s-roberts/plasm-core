//! Unified Plasm program AST.
//!
//! This parser owns the statement/root shape and postfix syntax for multi-line Plasm programs. It
//! intentionally keeps catalogue-specific path expressions as source fragments; callers that have a
//! CGS continue to parse those leaves with [`super::parse`].

use super::{PlasmPostfixOp, peel_postfix_suffixes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProgram {
    pub statements: Vec<Statement>,
    pub roots: Vec<ExprNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Bind { label: String, expr: ExprNode },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprNode {
    pub primary: String,
    pub postfix: Vec<PlasmPostfixOp>,
}

/// Parse line-oriented Plasm program shape (bindings, final roots, postfix transforms).
///
/// This parser does not validate entity names, relation names, or capabilities; it is the shared
/// grammar owner for program composition around existing surface expression leaves.
pub fn parse_program_shape(source: &str) -> Result<ParsedProgram, String> {
    let mut statements = Vec::new();
    let mut roots = None::<Vec<ExprNode>>;

    for raw in source.lines() {
        let line = raw.split_once(";;").map_or(raw, |(left, _)| left).trim();
        if line.is_empty() {
            continue;
        }
        if let Some((label, rhs)) = split_assignment(line) {
            validate_label(label)?;
            let expr = parse_expr_node(rhs)?;
            statements.push(Statement::Bind {
                label: label.to_string(),
                expr,
            });
        } else {
            if line.starts_with("return ") {
                return Err("return is not Plasm syntax; use bare final roots".into());
            }
            roots = Some(
                split_roots(line)?
                    .into_iter()
                    .map(parse_expr_node)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
    }

    let roots = roots.ok_or_else(|| "Plasm program needs a final root line".to_string())?;
    if roots.is_empty() {
        return Err("Plasm program final roots list is empty".into());
    }
    Ok(ParsedProgram { statements, roots })
}

pub fn parse_expr_node(raw: &str) -> Result<ExprNode, String> {
    let (primary, postfix) = peel_postfix_suffixes(raw)?;
    if primary.trim().is_empty() {
        return Err("expression primary is empty".into());
    }
    Ok(ExprNode { primary, postfix })
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            '=' if quote.is_none() && depth == 0 => {
                let left = line[..i].trim();
                let right = line[i + 1..].trim();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left, right));
                }
            }
            _ => {}
        }
    }
    None
}

fn split_roots(line: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote = None::<char>;
    for (i, c) in line.char_indices() {
        match c {
            '"' | '\'' if quote == Some(c) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(c),
            '(' | '[' | '{' if quote.is_none() => depth += 1,
            ')' | ']' | '}' if quote.is_none() => depth -= 1,
            ',' if quote.is_none() && depth == 0 => {
                out.push(line[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("unbalanced delimiters in `{line}`"));
    }
    out.push(line[start..].trim());
    Ok(out.into_iter().filter(|s| !s.is_empty()).collect())
}

fn validate_label(label: &str) -> Result<(), String> {
    let mut chars = label.chars();
    let valid = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(label, "_" | "$" | "return");
    if valid {
        Ok(())
    } else {
        Err(format!("invalid Plasm program label `{label}`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binding_and_direct_limit_root() {
        let p =
            parse_program_shape("repo = e2(owner=\"ryan\", repo=\"plasm\")\ne1{p4=repo}.limit(20)")
                .expect("program");
        assert_eq!(p.statements.len(), 1);
        assert_eq!(p.roots.len(), 1);
        assert_eq!(p.roots[0].primary, "e1{p4=repo}");
        assert!(matches!(p.roots[0].postfix[0], PlasmPostfixOp::Limit(20)));
    }

    #[test]
    fn parses_label_postfix_chain() {
        let p = parse_program_shape("commits = e1{}\ncommits.sort(date, desc).limit(10)")
            .expect("program");
        assert_eq!(p.roots[0].primary, "commits");
        assert_eq!(p.roots[0].postfix.len(), 2);
    }
}
