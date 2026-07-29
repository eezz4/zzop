//! The per-SOURCE volume fold for `cross-layer/unconsumed-endpoint` — the cap and the collapse that
//! implements it. Split out of the parent purely to keep that file under the repo's per-file line cap; the
//! reasoning for folding at all (a backend-only or framework-package tree reports a census, not a to-do
//! list) lives in the parent's module doc, which is where a reader meets the rule.

use zzop_core::{disable_hint, Finding};

// The cap itself stays declared in the parent (its policy-census entry is keyed on that path); only the
// collapse that implements it lives here.
use super::MAX_LISTED_PER_SOURCE;

/// Volume fold: keep the first [`MAX_LISTED_PER_SOURCE`] findings of each source and replace that
/// source's tail with ONE finding that says how many were folded and where the uncapped list is. Input
/// must already be sorted by `(source, file, line)`.
///
/// Same `rule_id` as the findings it replaces, on purpose: `disabled_rules`, `severity_overrides` and
/// `failOn` must keep meaning one thing for this rule, and a second id would be a new public rule name
/// with its own catalog row and its own way of being left on when the user turned this rule off.
pub(super) fn fold_per_source(
    items: Vec<(String, Finding)>,
    unresolved_http: usize,
) -> Vec<Finding> {
    let mut out = Vec::with_capacity(items.len());
    let mut seen_in_source = 0usize;
    let mut folded: Vec<Finding> = Vec::new();
    let mut current: Option<String> = None;
    let flush = |folded: &mut Vec<Finding>, out: &mut Vec<Finding>, source: &str| {
        if folded.is_empty() {
            return;
        }
        let count = folded.len();
        // Anchored on the FIRST folded endpoint — a real site inside the set that is no longer listed,
        // never a synthetic `file:0`.
        let anchor = folded.remove(0);
        folded.clear();
        out.push(Finding {
            rule_id: anchor.rule_id,
            severity: anchor.severity,
            file: anchor.file,
            line: anchor.line,
            message: format!(
                "{count} further endpoint(s) in source `{source}` are not called by any source in this \
                 analysis, beyond the {MAX_LISTED_PER_SOURCE} listed individually above — folded into this \
                 one finding because a tree where every route is unconsumed by construction (a backend-only \
                 or framework-package repo whose callers are not in this run) reports a census, not a \
                 to-do list. Nothing is dropped: the full, UNCAPPED list is `crossLayer.unconsumedProvides` \
                 (`zzop facts` with the CLI binary; MCP hosts get the same array on the `cross_repo` \
                 reply), and the same caveats apply to every folded route as to the listed ones — \
                 including the {unresolved_http} unresolved dynamic-URL http consume(s) this run could not \
                 statically match. {} to silence the whole rule.",
                disable_hint("cross-layer/unconsumed-endpoint")
            ),
            evidence_paths: Vec::new(),
            data: Some(serde_json::json!({
                "source": source,
                "foldedEndpointCount": count,
                "listedEndpointCount": MAX_LISTED_PER_SOURCE,
                "unresolvedHttpConsumeCount": unresolved_http,
            })),
        });
    };
    for (source, finding) in items {
        if current.as_deref() != Some(source.as_str()) {
            if let Some(prev) = current.take() {
                flush(&mut folded, &mut out, &prev);
            }
            current = Some(source.clone());
            seen_in_source = 0;
        }
        seen_in_source += 1;
        if seen_in_source <= MAX_LISTED_PER_SOURCE {
            out.push(finding);
        } else {
            folded.push(finding);
        }
    }
    if let Some(prev) = current {
        flush(&mut folded, &mut out, &prev);
    }
    out
}
