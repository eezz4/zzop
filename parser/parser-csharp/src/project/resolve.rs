//! Constant-reference resolution for the whole-corpus C# provides pass — deliberately SIMPLER than
//! `zzop_parser_java_21::project::resolve` (no `extends`-chain walk, no `+`-concatenation evaluation): a
//! SIMPLE constant reference resolves to a single string-literal value, or it does not resolve at all.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use super::ClassRow;

/// Resolves one attribute's raw args to its literal constant value, or `None` (unresolved -> the caller skips
/// and counts the route). A QUALIFIED `ClassName.CONST` looks up `(ClassName, CONST)` in the corpus (`None`
/// when the class is absent/ambiguous or declares no such constant); a BARE `CONST` looks up the global
/// bare-name index (`None` when the name is out-of-corpus or declared in 2+ classes — ambiguous). Args that
/// are not a simple constant reference at all (a concatenation, a computed expression) fail `const_ref` and
/// return `None`.
pub(super) fn resolve_ref(
    args: &str,
    classes: &HashMap<String, ClassRow>,
    bare: &HashMap<String, Option<String>>,
) -> Option<String> {
    let (qualifier, name) = const_ref(args)?;
    match qualifier {
        Some(class) => classes.get(&class)?.constants.get(&name).cloned(),
        None => bare.get(&name).cloned().flatten(),
    }
}

/// The global bare-name constant index: a const name declared in EXACTLY ONE class maps to its value
/// (`Some`); a name declared in 2+ classes is ambiguous (`None`, so a bare reference to it never guesses) —
/// the same ambiguity discipline Java applies to duplicate class names.
pub(super) fn build_bare_index(
    classes: &HashMap<String, ClassRow>,
) -> HashMap<String, Option<String>> {
    let mut counts: HashMap<String, (usize, String)> = HashMap::new();
    for row in classes.values() {
        for (name, value) in &row.constants {
            let entry = counts.entry(name.clone()).or_insert((0, String::new()));
            entry.0 += 1;
            entry.1 = value.clone();
        }
    }
    counts
        .into_iter()
        .map(|(name, (count, value))| (name, (count == 1).then_some(value)))
        .collect()
}

/// Splits a simple constant reference into `(optional qualifier, name)` — `Routes.List` ->
/// `(Some("Routes"), "List")`, `List` -> `(None, "List")`, `Ns.Routes.List` -> `(Some("Routes"), "List")`
/// (only the last segment before the name is the qualifier class). The FIRST comma-separated argument is
/// used (`[Route(ApiRoutes.Base, Name = "x")]`), so a trailing named attribute never defeats the reference.
/// `None` when the (trimmed) first argument is not a bare/qualified identifier at all.
fn const_ref(args: &str) -> Option<(Option<String>, String)> {
    let first = args.split(',').next()?.trim();
    let c = const_ref_re().captures(first)?;
    let qualifier = c.get(1).map(|m| m.as_str().to_string());
    Some((qualifier, c[2].to_string()))
}

fn const_ref_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:(?:[A-Za-z_]\w*\.)*([A-Za-z_]\w*)\.)?([A-Za-z_]\w*)$").unwrap()
    })
}
