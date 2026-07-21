//! Class-scoped constant-reference resolution for class-level `@RequestMapping` prefixes — ported
//! near-verbatim from `zzop_parser_java::project::resolve` (this text-based `+`-concatenation resolver
//! operates on already-AST-extracted raw expression text, so the algorithm itself needs no AST-native
//! rewrite — see the parent module doc's "AST-native design change" section for what DID change).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use super::{ClassRow, PrefixState};
use crate::provides::route_path_arg;

/// Resolves one class-level `@RequestMapping`'s raw argument text to a `PrefixState` — a literal
/// `value=`/`path=`/positional string (`route_path_arg`, attribute-aware — see its doc) resolves
/// directly; a bare/empty argument list resolves to `""`; anything else (including a NAMED
/// `value = SOME_CONSTANT` with no quotes, which `route_path_arg` deliberately does not treat as a
/// literal) is treated as a (possibly qualified) constant reference — extracted by `const_ref_qualified`,
/// which is ITSELF attribute-aware (see its doc: `value=`/`path=`/positional, never an unrelated
/// attribute like `produces=`/`headers=`) — and resolved via `resolve_scoped`, starting at `class_name`.
pub(super) fn resolve_class_prefix(
    class_name: &str,
    raw: &Option<String>,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
) -> PrefixState {
    let Some(args) = raw else {
        return PrefixState::NoMapping;
    };
    if let Some(lit) = route_path_arg(args) {
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

/// Resolves a NON-LITERAL method-level route path (a constant reference like `@GetMapping(ApiPaths.USERS)`
/// or `@RequestMapping(value = SOME_CONST, method = GET)`) against the corpus, scoped to the DECLARING
/// class's own `extends` chain — the method-path analog of [`resolve_class_prefix`]'s constant branch,
/// reusing the exact same attribute-aware `const_ref_qualified` extractor and `resolve_scoped` walk. `raw`
/// is the annotation's raw argument text (`MethodPath::Unresolved`). `None` when no constant reference is
/// present (a genuinely computed expression) or the reference does not resolve to a literal in the corpus —
/// the caller then SKIPS the route (counted in `skipped_unresolved_method_path`), never keying the empty base.
pub(super) fn resolve_method_path(
    class_name: &str,
    raw: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
) -> Option<String> {
    let (qualifier, name) = const_ref_qualified(raw)?;
    let mut visiting = HashSet::new();
    resolve_scoped(
        class_name,
        qualifier.as_deref(),
        &name,
        classes,
        memo,
        &mut visiting,
    )
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

/// The (optional, dotted-or-bare) constant reference that is the actual PATH for this class-level
/// `@RequestMapping` — attribute-aware exactly like `route_path_arg` (`provides/annotations.rs`, whose
/// doc explains the same named-attribute-wins-over-positional-scan rationale for the literal case): a
/// named `value=` attribute wins, then a named `path=` attribute, and only a genuinely positional first
/// argument (`@RequestMapping(BASE_PATH)`) when neither names a constant. This is the
/// constant-reference counterpart of `route_path_arg`'s quoted-literal extraction — implemented
/// separately (not by literally sharing `route_path_arg`'s regexes) because the terminal token shape
/// differs (a bare/qualified `SCREAMING_SNAKE_CASE` identifier here vs. a `"..."` literal there), but the
/// SAME attribute-boundary discipline applies: an unrelated attribute (`produces=`, `headers=`, ...)
/// that happens to carry a constant, or a quoted string literal, earlier in the raw text must never be
/// mistaken for the path.
///
/// The three rules are tried in order and the FIRST that yields a constant wins (`value=`'s RHS, then
/// `path=`'s RHS, then a positional bare constant) — a rule that finds no constant FALLS THROUGH to the
/// next rather than committing (mirroring `route_path_arg`, which likewise falls through on a failed
/// `value=`/`path=` match): so `@RequestMapping(params = "value=1", path = ApiPaths.USERS)`, where a
/// `params` string literal literally contains the substring `value=`, still resolves `path=`'s constant
/// instead of being swallowed. `None` only when no rule finds a constant at all.
fn const_ref_qualified(args: &str) -> Option<(Option<String>, String)> {
    capture_qualified(value_attr_const_re(), args)
        .or_else(|| capture_qualified(path_attr_const_re(), args))
        .or_else(|| capture_qualified(bare_leading_const_re(), args))
}

fn capture_qualified(re: &Regex, args: &str) -> Option<(Option<String>, String)> {
    let c = re.captures(args)?;
    let qualifier = c.get(1).map(|m| m.as_str().to_string());
    Some((qualifier, c[2].to_string()))
}

fn qualified_ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:(?:[A-Za-z_$][\w$]*\.)*([A-Za-z_$][\w$]*)\.)?([A-Za-z_$][\w$]*)$").unwrap()
    })
}

/// Captures the (optional qualifier, `SCREAMING_SNAKE_CASE` name) constant reference immediately on the
/// right-hand side of a `value=` attribute. The `value` keyword must sit at a REAL attribute boundary —
/// start of the (paren-stripped) argument text, or just after a `(`/`,` — so a `value=` substring buried
/// inside another attribute's string literal (`params = "value=1"`) can never be mistaken for the named
/// `value` attribute. Otherwise mirrors `provides::annotations::value_attr_re`'s shape
/// (`value\s*=\s*\{?\s*`) but captures a constant identifier instead of a quoted literal.
fn value_attr_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?:^|[(,])\s*value\s*=\s*\{?\s*(?:([A-Za-z_][A-Za-z0-9_]*)\.)?([A-Z][A-Z0-9_]*)\b",
        )
        .unwrap()
    })
}

/// Same role and same real-attribute-boundary anchoring as `value_attr_const_re`, for `path=`.
fn path_attr_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?:^|[(,])\s*path\s*=\s*\{?\s*(?:([A-Za-z_][A-Za-z0-9_]*)\.)?([A-Z][A-Z0-9_]*)\b",
        )
        .unwrap()
    })
}

/// Captures a constant reference that is the genuinely POSITIONAL first argument (`@RequestMapping(
/// BASE_PATH)`, `@RequestMapping(Api.BASE_PATH, method = RequestMethod.GET)`) — anchored at the start of
/// `args` (Java requires positional arguments before any named ones, so the leading token is always the
/// positional one when present) and required to be immediately followed by a comma or end-of-string
/// (never `=`) so a named attribute whose NAME happens to be all-uppercase can never be mistaken for a
/// bare positional constant.
fn bare_leading_const_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*(?:([A-Za-z_][A-Za-z0-9_]*)\.)?([A-Z][A-Z0-9_]*)\s*(?:,|$)").unwrap()
    })
}
