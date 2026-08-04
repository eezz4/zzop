//! S8: call-graph LANGUAGE-coverage self-report — names, per run, the languages whose HTTP routes this
//! engine extracted but whose call graph it cannot walk, and the rule that therefore goes silent on them.
//!
//! ## Why the census cannot see this class
//! Every other coverage signal in this crate keys on an EMPTY channel: no provides, no consumes, no
//! db-table facts. This one is the opposite shape — the files parse, the symbols project, the routes are
//! extracted, the per-tree census reads healthy — and yet one rule family is structurally inert because
//! no parser produces `RawCall` sites for that language. `coverage.*` counts cannot express it; only
//! naming "which FACT × which LANGUAGE" can. That is exactly the failure this repo calls silent failure:
//! a zero that reads as an all-clear.
//!
//! ## Read from the code, never from a copy
//! The covered set is `zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS` itself and
//! the silenced rule is named by its own id — so a future language lift updates this warning by editing
//! the constant, and a stale hand-written list can never drift out from under it. The rule id is spelled
//! once here and pinned against the shipped rule registry by this module's own test.
//!
//! ## Direction: over-disclosure is safe
//! Like every sibling tripwire this is a `warnings: Vec<String>` self-report, not a `Finding` — it
//! suppresses nothing and changes no verdict (the coverage-disclosure decision's "disclosure only, never
//! suppression" line). A tree with no uncovered-language routes stays silent.

use std::collections::BTreeMap;

use zzop_core::IoProvide;

/// The rule this gap silences. Spelled here rather than imported because `rules-http` exposes the id only
/// as a literal inside `scan_mutating_route_no_auth`'s emitted `Finding`; the pin below asserts the
/// spelling against the shipped rule registry so a rename cannot leave this warning naming a ghost.
const SILENCED_RULE_ID: &str = "mutating-route-no-auth";

/// Cap on example route files listed per uncovered extension — the "up to 3 example paths" convention
/// every sibling tripwire in this module uses.
const MAX_EXAMPLES: usize = 3;

/// `Some(warning)` when this tree extracted `http` route provides from at least one file whose extension
/// is OUTSIDE the call-graph-covered set — module doc. `None` when every extracted route is in a covered
/// language (including the common case of no routes at all).
pub fn call_graph_language_gap_warning(io_provides: &[IoProvide]) -> Option<String> {
    let covered = zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS;
    let mut uncovered: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for p in io_provides.iter().filter(|p| p.kind == "http") {
        let Some(ext) = std::path::Path::new(&p.file)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
        else {
            continue; // extensionless route file — no language signal to name
        };
        if covered.contains(&ext.as_str()) {
            continue;
        }
        let entry = uncovered.entry(ext).or_insert((0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < MAX_EXAMPLES && !entry.1.contains(&p.file) {
            entry.1.push(p.file.clone());
        }
    }
    if uncovered.is_empty() {
        return None;
    }

    let per_ext: Vec<String> = uncovered
        .iter()
        .map(|(ext, (count, examples))| {
            format!(".{ext} ({count} route(s), e.g. {})", examples.join(", "))
        })
        .collect();
    Some(format!(
        "Call-graph coverage gap: this tree's http routes include {} whose language has no call-site \
         extractor in this build, so `{SILENCED_RULE_ID}` is structurally silent for them — its \
         handler-reachability BFS has no edges to walk there, which is why those routes are exempted \
         rather than reported clean. Languages this build DOES walk: {}. Three ways to close it for \
         yours: inject `auth-guarded` on the guarded route or router prefix through an adapter overlay \
         (Mode B), supply call sites through `files[].calls` in a Mode A analyze-envelope run \
         (`zzop analyze-envelope` / MCP tool `analyze_envelope` — Mode A ONLY: a Mode B overlay's \
         `calls` are not consumed on a native tree), or teach the parser for that language a \
         `RawCall` extractor. Every other rule is unaffected — this gap is specific to the \
         call graph.",
        per_ext.join("; "),
        zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS.join("/"),
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

    /// Seals the silence side: a tree whose routes are all in call-graph-covered languages must produce
    /// no warning at all — an unconditional one would train readers to ignore it.
    #[test]
    fn covered_languages_produce_no_warning() {
        let provides = vec![
            provide("src/routes.ts", "POST /a"),
            provide("src/main/java/A.java", "POST /b"),
            provide("app/api/routes/items.py", "POST /c"),
        ];
        assert!(call_graph_language_gap_warning(&provides).is_none());
        assert!(call_graph_language_gap_warning(&[]).is_none());
    }

    /// Seals the disclosure itself: an uncovered language is named by EXTENSION, with a route count, an
    /// example path, and the id of the rule that goes silent — the three things the coverage-disclosure
    /// decision requires an agent to be told ("which language / which rule / how to open it").
    #[test]
    fn uncovered_language_is_named_with_its_silenced_rule_and_an_escape_hatch() {
        let provides = vec![
            provide("internal/api/handler.go", "POST /a"),
            provide("internal/api/other.go", "DELETE /b"),
            provide("src/routes.ts", "POST /c"),
        ];
        let w = call_graph_language_gap_warning(&provides).expect("gap must be disclosed");
        assert!(w.contains(".go (2 route(s)"), "{w}");
        assert!(w.contains("internal/api/handler.go"), "{w}");
        assert!(w.contains("mutating-route-no-auth"), "{w}");
        assert!(w.contains("Mode B"), "{w}");
        // The `calls` escape hatch must name the ONE lane that consumes it (Mode A analyze-envelope):
        // this warning fires on a NATIVE tree, whose only envelope surface is Mode B overlays — and
        // Mode B drops `calls` — so an unqualified "envelope call channel" pointed the reader at a
        // channel their run would discard (crates F4).
        assert!(w.contains("Mode A analyze-envelope"), "{w}");
        // The covered list is rendered from the constant, never a copy.
        assert!(w.contains("java/py/pyi"), "{w}");
        // A covered-language route never appears in the gap list.
        assert!(!w.contains(".ts ("), "{w}");
    }

    /// Seals that the id this warning publishes is the id the engine actually ships — a rename that
    /// missed this file would otherwise leave the disclosure pointing at a rule nobody can look up.
    #[test]
    fn silenced_rule_id_is_a_real_shipped_rule() {
        let mut registry = zzop_core::RuleRegistry::new();
        crate::register_all_native(&mut registry);
        let ids = registry.ids();
        assert!(
            ids.iter().any(|id| id == SILENCED_RULE_ID),
            "{SILENCED_RULE_ID} is not a shipped native rule id: {ids:?}"
        );
    }
}
