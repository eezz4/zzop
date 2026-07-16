//! Class-scoped constant-reference resolution for class-level `@RequestMapping` prefixes — ported
//! near-verbatim from `zzop_parser_java::project::resolve` (this text-based `+`-concatenation resolver
//! operates on already-AST-extracted raw expression text, so the algorithm itself needs no AST-native
//! rewrite — see the parent module doc's "AST-native design change" section for what DID change).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use super::{ClassRow, PrefixState};
use crate::provides::first_quoted_string;

/// Resolves one class-level `@RequestMapping`'s raw argument text to a `PrefixState` — a literal quoted
/// string resolves directly; a bare/empty argument list resolves to `""`; anything else is treated as a
/// (possibly qualified) constant reference and resolved via `resolve_scoped`, starting at `class_name`.
pub(super) fn resolve_class_prefix(
    class_name: &str,
    raw: &Option<String>,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
) -> PrefixState {
    let Some(args) = raw else {
        return PrefixState::NoMapping;
    };
    if let Some(lit) = first_quoted_string(args) {
        return PrefixState::Resolved(lit);
    }
    if args.trim().is_empty() {
        return PrefixState::Resolved(String::new());
    }
    let Some((qualifier, name)) = const_ref_qualified(args) else {
        return PrefixState::Unresolved;
    };
    let mut visiting = HashSet::new();
    match resolve_scoped(
        class_name,
        qualifier.as_deref(),
        &name,
        classes,
        memo,
        &mut visiting,
    ) {
        Some(v) => PrefixState::Resolved(v),
        None => PrefixState::Unresolved,
    }
}

/// Resolves a possibly-qualified constant reference to its literal value, walking `scope_class`'s (or a
/// dotted reference's own qualifier's) `extends` chain. `None` if the starting class isn't in `classes`
/// or no class in the chain declares `const_name`.
fn resolve_scoped(
    scope_class: &str,
    qualifier: Option<&str>,
    const_name: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    visiting: &mut HashSet<(String, String)>,
) -> Option<String> {
    let start = qualifier.unwrap_or(scope_class);
    let mut cur = Some(start.to_string());
    let mut walked = HashSet::new();
    let mut depth = 0;
    while let Some(cname) = cur {
        if depth > 40 || !walked.insert(cname.clone()) {
            return None; // cycle or pathological depth — defensive, not expected in real inheritance.
        }
        depth += 1;
        let row = classes.get(&cname)?;
        if let Some(raw) = row.constants.get(const_name) {
            return resolve_declared(&cname, const_name, raw, classes, memo, visiting);
        }
        cur = row.extends.clone();
    }
    None
}

/// Resolves one already-located declaration, memoizing into `memo` and guarding against a reference
/// cycle via `visiting`.
fn resolve_declared(
    owner_class: &str,
    const_name: &str,
    raw: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    visiting: &mut HashSet<(String, String)>,
) -> Option<String> {
    let key = (owner_class.to_string(), const_name.to_string());
    if let Some(v) = memo.get(&key) {
        return v.clone();
    }
    if !visiting.insert(key.clone()) {
        return None;
    }
    let value = eval_concat_expr(owner_class, raw, classes, memo, visiting);
    visiting.remove(&key);
    memo.insert(key, value.clone());
    value
}

/// Evaluates a `+`-concatenation expression (`BASE_PATH + VERSION + "/assets"`), owned by `owner_class`,
/// to a literal String term by term — a quoted term is literal, an identifier term resolves via
/// `resolve_scoped`. `None` if any term fails to resolve.
fn eval_concat_expr(
    owner_class: &str,
    expr: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    visiting: &mut HashSet<(String, String)>,
) -> Option<String> {
    let mut out = String::new();
    for term in split_plus_terms(expr) {
        let term = term.trim();
        if let Some(lit) = quoted_literal(term) {
            out.push_str(&lit);
            continue;
        }
        let (qualifier, name) = split_qualified(term)?;
        out.push_str(&resolve_scoped(
            owner_class,
            qualifier.as_deref(),
            &name,
            classes,
            memo,
            visiting,
        )?);
    }
    Some(out)
}

/// Splits a `+`-concatenation expression into its terms, honoring `"..."` quoting so a `+` inside a
/// string literal is never treated as a term separator.
fn split_plus_terms(expr: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for c in expr.chars() {
        match c {
            '"' => {
                in_str = !in_str;
                cur.push(c);
            }
            '+' if !in_str => terms.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        terms.push(cur);
    }
    terms
}

fn quoted_literal(term: &str) -> Option<String> {
    if term.len() >= 2 && term.starts_with('"') && term.ends_with('"') {
        Some(term[1..term.len() - 1].to_string())
    } else {
        None
    }
}

/// Splits an identifier term into `(qualifier, name)` — `Entity.APPLICATIONS` ->
/// `(Some("Entity"), "APPLICATIONS")`, `BASE_PATH` -> `(None, "BASE_PATH")`.
fn split_qualified(term: &str) -> Option<(Option<String>, String)> {
    let c = qualified_ident_re().captures(term)?;
    let qualifier = c.get(1).map(|m| m.as_str().to_string());
    Some((qualifier, c[2].to_string()))
}

/// The (optional, dotted-or-bare) constant reference found anywhere in an annotation's raw argument
/// text, restricted to a `SCREAMING_SNAKE_CASE` final name so an attribute name like `value` is never
/// mistaken for a constant reference.
fn const_ref_qualified(args: &str) -> Option<(Option<String>, String)> {
    let c = const_ref_re().captures(args)?;
    let qualifier = c.get(1).map(|m| m.as_str().to_string());
    Some((qualifier, c[2].to_string()))
}

fn qualified_ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:(?:[A-Za-z_$][\w$]*\.)*([A-Za-z_$][\w$]*)\.)?([A-Za-z_$][\w$]*)$").unwrap()
    })
}

fn const_ref_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:([A-Za-z_][A-Za-z0-9_]*)\.)?([A-Z][A-Z0-9_]*)\b").unwrap())
}
