//! The FOREIGN-VS-OVERLAPPING FOLD — the parent module's section of that name, in code.
//!
//! The seam is judgment vs utterance. Everything in the parent decides WHICH consumes are unmatched
//! (the structural gates, the vocabulary vetoes, the zero-provides veto); everything here decides HOW
//! that surviving set is SAID — one finding each, or one aggregate that replaces N of them. The two
//! halves have separate contracts and separate failure modes (a veto bug loses a real defect; a fold bug
//! only changes how many messages carry the same keys), and the parent's own doc already drew the line
//! between them in prose before it was a file.

use std::collections::BTreeSet;

use super::message::{self, individual_finding};
use super::UnmatchedConsume;

/// Fold threshold for "foreign" unprovided consumes (first path segment outside the tree's provided key
/// space). Same rationale as `MIN_PREFIX_DRIFT_GROUP` in the cross-layer crate: 2 can be coincidence, 3+ is
/// a pattern (here: a partial-provider tree, e.g. a monorepo where only one app's routes are extracted).
/// Crate boundary prevents symbol sharing — the relationship is pinned by an equality test in the engine.
pub const MIN_FOREIGN_UNPROVIDED_GROUP: usize = 3;

/// Turns the split population into findings: overlapping consumes always speak individually, foreign ones
/// fold into ONE aggregate at [`MIN_FOREIGN_UNPROVIDED_GROUP`] or above and speak individually below it.
/// Output is sorted by `(file, line)` — the caller returns it verbatim.
pub(super) fn findings(
    overlapping: &[UnmatchedConsume<'_>],
    foreign: &[UnmatchedConsume<'_>],
    provide_first_segments: &BTreeSet<&str>,
    contributing_provide_count: usize,
) -> Vec<zzop_core::Finding> {
    let mut findings: Vec<zzop_core::Finding> = overlapping
        .iter()
        .map(|u| individual_finding(&u.key, u.raw, u.file, u.line))
        .collect();

    if foreign.len() >= MIN_FOREIGN_UNPROVIDED_GROUP {
        let mut anchor_order: Vec<&UnmatchedConsume> = foreign.iter().collect();
        anchor_order.sort_by(|a, b| {
            a.file
                .cmp(b.file)
                .then(a.line.cmp(&b.line))
                .then(a.key.cmp(&b.key))
        });
        let anchor = anchor_order[0];

        let mut routes: Vec<&str> = foreign.iter().map(|u| u.key.as_ref()).collect();
        routes.sort_unstable();
        routes.dedup();
        // A folded entry is enumerated under its JOIN key, so a declared-host consume shows an internal
        // path the author cannot grep for. Carry the absolute spellings alongside, present only when a
        // re-key actually happened (parent module doc "Structural gates").
        let mut raws: Vec<&str> = foreign.iter().filter_map(|u| u.raw).collect();
        raws.sort_unstable();
        raws.dedup();

        let example_segments: Vec<&str> = provide_first_segments.iter().copied().take(3).collect();
        // Only the first 3 provided first-segments are rendered inline; when more exist, append an ellipsis
        // so the message doesn't imply the tree provides only these 3 path families.
        let example_segments_str = if provide_first_segments.len() > 3 {
            format!("{}, …", example_segments.join(", "))
        } else {
            example_segments.join(", ")
        };
        // Edge case: a tree whose only http provides are root-path (`GET /`) contributes zero
        // first-segments (`first_path_segment` returns `None` for `/` — see that fn's own doc), so
        // `example_segments_str` is empty and the count is 0. The normal "{m} provide(s) under {segments}"
        // clause would then dangle a trailing "under" with nothing after it. Reword just that clause when
        // there are no segments; the test-pinned wording is unchanged whenever a segment exists.
        let path_space_clause = if provide_first_segments.is_empty() {
            "provides at least one route, but none under a named path prefix (e.g. only `GET /`)"
                .to_string()
        } else {
            format!("{contributing_provide_count} provide(s) under {example_segments_str}")
        };

        findings.push(message::aggregate_finding(message::Aggregate {
            call_count: foreign.len(),
            routes: &routes,
            raws: &raws,
            path_space_clause: &path_space_clause,
            provide_first_segments,
            file: anchor.file,
            line: anchor.line,
        }));
    } else {
        findings.extend(
            foreign
                .iter()
                .map(|u| individual_finding(&u.key, u.raw, u.file, u.line)),
        );
    }

    findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    findings
}
