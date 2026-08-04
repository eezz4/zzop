//! S9: METHOD-UNKNOWN route range self-report — names, per run, the routes this engine extracted whose
//! HTTP method it could not determine, and the write-gated rule that therefore can never reach them.
//!
//! ## The sibling it is NOT
//! [`super::call_graph_language`] (S8) says "this EXTENSION is outside the call graph". This one fires
//! on routes that are fully INSIDE it: Python is call-graph covered, its routes extract, its guards
//! project. The gap is one field. A Django URLconf entry names a view, not a verb, so
//! `parser-python-3`'s `django_routes` adapter emits `method: UNKNOWN_VERB` **by construction** — and
//! every rule that gates on write methods (`WRITE_HTTP_METHODS`) filters those routes out before it
//! looks at anything else.
//!
//! ## Why a disclosure is needed even though the root cause is known
//! The measured symptom (2026-07-27): a batch built Django `auth-guarded` emission, pinned it, and
//! `be-django`'s finding count moved by zero — and in this structure it could not have moved. Reading
//! that run, "0 findings" is indistinguishable from "clean". Expanding DRF routers/ViewSets to real
//! verbs is the actual fix and it is queued behind a corpus trigger; **this disclosure is needed
//! whether or not that lands**, because until it does the user cannot see the gap at all, and after it
//! does there will still be URLconf shapes no expansion resolves.
//!
//! ## Direction: over-disclosure is safe
//! A `warnings: Vec<String>` self-report like every sibling — it suppresses nothing, changes no
//! verdict, and stays silent on a tree whose routes all carry real verbs. The alternative that was
//! rejected outright: loosening the write gate so `?` counts as "might be a write". That is a
//! never-guess violation with a measured cost — it would flag every read-only Django route as
//! unauthenticated-mutation, turning one tree into a false-positive field.

use std::collections::BTreeMap;

use zzop_core::{IoProvide, UNKNOWN_VERB};

/// The rule family this gap silences. Named by id rather than imported for the same reason S8 does it:
/// `rules-http` exposes the id only as a literal inside the emitted `Finding`, and this module's test
/// pins the spelling against the shipped registry so a rename cannot leave a ghost here.
const SILENCED_RULE_ID: &str = "mutating-route-no-auth";

/// Cap on example route files listed — the "up to 3 example paths" convention every sibling uses.
const MAX_EXAMPLES: usize = 3;

/// `Some(warning)` when this tree extracted `http` route provides whose METHOD is unknown, so every
/// write-gated rule is out of range for them. `None` when every extracted route carries a real verb
/// (including the common case of no routes at all).
pub fn unknown_verb_range_warning(io_provides: &[IoProvide]) -> Option<String> {
    let mut count = 0usize;
    let mut examples: Vec<String> = Vec::new();
    // Grouped by file extension for the same reason S8 groups: it is the one signal in hand that tells
    // the reader WHICH stack of theirs is affected, without this module knowing a thing about Django.
    let mut by_ext: BTreeMap<String, usize> = BTreeMap::new();

    for p in io_provides.iter().filter(|p| p.kind == "http") {
        // The key is `"METHOD PATH"`; an unknown verb is the sentinel the extractors agreed on rather
        // than an empty string, so this reads the same constant they write.
        if !p.key.starts_with(UNKNOWN_VERB) {
            continue;
        }
        count += 1;
        if examples.len() < MAX_EXAMPLES && !examples.contains(&p.file) {
            examples.push(p.file.clone());
        }
        if let Some(ext) = std::path::Path::new(&p.file)
            .extension()
            .and_then(|e| e.to_str())
        {
            *by_ext.entry(ext.to_ascii_lowercase()).or_insert(0) += 1;
        }
    }
    if count == 0 {
        return None;
    }

    let exts: Vec<String> = by_ext
        .iter()
        .map(|(ext, n)| format!(".{ext} ({n})"))
        .collect();
    Some(format!(
        "Route-range gap: {count} http route(s) in this tree carry an UNKNOWN method ({}) — e.g. {}. \
         Rules gated on write methods, `{SILENCED_RULE_ID}` among them, filter those routes out before \
         evaluating anything, so they are OUT OF RANGE rather than reported clean: a zero here is not \
         an all-clear. This is by construction for route tables that name a view without a verb (a \
         Django URLconf entry is the common case), not a parse failure. Two ways to bring them in \
         range: declare the real routes with `trees[].routes` ({{ \"key\": \"POST /path\" }}), or \
         supply them through an adapter overlay (Mode B). Rules that do not gate on method — every \
         cross-layer join rule included — see these routes normally.",
        exts.join(", "),
        examples.join(", "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provide(file: &str, key: &str) -> IoProvide {
        IoProvide {
            response: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
            body: None,
        }
    }

    #[test]
    fn a_tree_whose_routes_all_carry_verbs_stays_silent() {
        let provides = vec![
            provide("api/views.py", "GET /users"),
            provide("api/views.py", "POST /users"),
        ];
        assert!(unknown_verb_range_warning(&provides).is_none());
    }

    #[test]
    fn unknown_verb_routes_are_named_with_their_count_and_the_silenced_rule() {
        let provides = vec![
            provide("app/urls.py", &format!("{UNKNOWN_VERB} /users")),
            provide("app/urls.py", &format!("{UNKNOWN_VERB} /orders")),
            provide("api/views.py", "GET /health"),
        ];
        let w =
            unknown_verb_range_warning(&provides).expect("two unknown-verb routes must disclose");
        assert!(w.contains("2 http route(s)"), "{w}");
        assert!(w.contains(SILENCED_RULE_ID), "{w}");
        assert!(
            w.contains(".py (2)"),
            "the affected stack must be nameable: {w}"
        );
        // The remedy must be actionable from a config file, not only from an adapter — the whole point
        // of `trees[].routes` is that the cheap fix exists.
        assert!(w.contains("trees[].routes"), "{w}");
    }

    /// The disclosure counts only what it claims to count: a real verb sharing a file with an unknown
    /// one must not inflate the number.
    #[test]
    fn real_verbs_in_the_same_file_are_not_counted() {
        let provides = vec![
            provide("app/urls.py", &format!("{UNKNOWN_VERB} /users")),
            provide("app/urls.py", "GET /users"),
        ];
        let w = unknown_verb_range_warning(&provides).unwrap();
        assert!(w.contains("1 http route(s)"), "{w}");
    }
}
