//! The lexical half of S2: how many route registrations can a READER see?
//!
//! Split out of `server_framework_import.rs` when that file crossed the 300-line cap (2026-09-06).
//! The seam is real rather than arithmetic — everything here answers one question with no opinion about
//! frameworks or findings, and it has TWO consumers with one meaning: the per-tree S2 warning next door,
//! and the run-wide provide-blind severity gate. Both suppress in the same direction on the same number,
//! so the number gets one definition.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::super::controller_silence::MIN_PROVIDES_FLOOR;
use super::is_server_framework_specifier;
/// A line that REGISTERS a route through the runtime method-call idiom this tripwire is named after:
/// an optional receiver chain, a route verb, and a STRING-LITERAL first argument.
///
/// The string-literal requirement is what makes this countable rather than noisy — it is the same thing
/// the extractor keys on, so `map.get(key)`, `headers.get(name)` and every other same-named accessor
/// falls out without a vocabulary. The optional receiver covers a chained continuation line
/// (`.route("/x", get(h))` under axum's `Router::new()`), and the case-insensitive verb covers Go
/// (`r.GET("/x", h)`) at no cost to the others.
///
/// A path this cannot see (a template literal, a computed path, a constant) is a path the EXTRACTOR
/// cannot key either, so not counting it keeps the two sides measuring the same thing — which is the
/// only property the comparison below depends on.
fn route_registration_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)^(?:[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z0-9_$]+)*)?\.(?:get|post|put|patch|delete|del|all|options|head|use|route)\s*\(\s*['"`]"#,
        )
        .expect("route registration regex is a compile-time constant")
    })
}

/// Whether the lexical scan is worth running at all: S2's own firing precondition, lifted so the caller
/// can decide before paying for a disk re-read.
///
/// Naming it here rather than re-spelling the two clauses at the call site is the point — this predicate
/// and [`server_framework_import_warning`]'s early returns have to stay the same question, or a tree could
/// be scanned and never judged (waste) or judged on a number never taken (wrong).
pub(crate) fn needs_visible_scan(
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    http_provides_count: usize,
) -> bool {
    http_provides_count < MIN_PROVIDES_FLOOR
        && package_import_files
            .keys()
            .any(|specifier| is_server_framework_specifier(specifier))
}

/// How many route registrations are LEXICALLY visible across the same files the extractor was given.
///
/// Deliberately the whole candidate set, not just the framework-importing files: a route registered in a
/// module that imports its router from elsewhere would otherwise go uncounted, and an undercount here is
/// the one direction that can silence a real gap (see the callers' suppression rule).
///
/// `pub(crate)` because the number has TWO consumers with one meaning: this file's S2 warning (per tree)
/// and [`provide_blind_sources`] (run-wide, via a count carried on `AnalyzeOutput`). It is measured once,
/// here, where `root` and the candidate list exist — the run-wide gate cannot re-derive the file set
/// without risking a different population than the extractor actually saw.
pub(crate) fn visible_route_registrations(root: &Path, candidate_rels: &[String]) -> usize {
    let re = route_registration_re();
    let mut n = 0usize;
    for rel in candidate_rels {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        n += text
            .lines()
            .filter(|line| re.is_match(line.trim_start()))
            .count();
    }
    n
}
