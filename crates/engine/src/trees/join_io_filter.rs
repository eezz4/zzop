//! The cross-layer JOIN input filter — see [`filter_join_io`]. Split out of `trees/mod.rs` on
//! 2026-08-08 when the run-scoped git memo pushed that file over the line-count ratchet; the cut is
//! along the seam these two items already formed (one predicate + its drop census, read by exactly
//! one caller) rather than an arbitrary one.

use zzop_core::IoFacts;

/// Per-tree drop counts + a capped file-path sample from [`filter_join_io`] — substrate for that
/// function's caller's own per-tree warning (see the call site's doc for the disclosure rationale).
/// `examples` combines BOTH dropped provides and dropped consumes (provides first, in their original
/// order, then consumes), capped at 3 total — the same "up to 3 example paths" convention
/// `unparsed_extension_warning` already uses for its own per-extension sample. DISTINCT file paths
/// only: one test file usually carries several dropped facts, and "a.go, a.go, a.go" tells the
/// reader nothing the count didn't (observed in the first live run of this warning).
#[derive(Default)]
pub(super) struct JoinIoDrop {
    pub(super) provides: usize,
    pub(super) consumes: usize,
    pub(super) examples: Vec<String>,
}

/// Cross-layer JOIN input filter: drops every provide/consume whose `file` is test-classified
/// (`zzop_core::is_test_file`) before it ever reaches `link_cross_layer_io`/`compute_cross_layer_findings`.
/// The published disclosure (`disclosure.rs`'s "classified-skip" class) claims test-classified io is
/// excluded from the cross-layer join — before this filter existed that claim was false: the join input
/// was built straight from each tree's raw `output.ir.ir.io`, so e.g. a Go `unit_test.go` route
/// registration became an ordinary production "provide" and could join a real cross-tree edge (observed
/// live: 4 of 5 provides on a real repo were test-harness routes). Deliberately does NOT touch
/// `output.ir` — the per-file raw facts (test-classified included) must stay visible in that tree's own
/// single-tree output; only the JOIN input built here is narrowed.
pub(super) fn filter_join_io(io: IoFacts) -> (IoFacts, JoinIoDrop) {
    let mut drop = JoinIoDrop::default();
    let provides = io
        .provides
        .into_iter()
        .filter(|p| {
            let is_test = zzop_core::is_test_file(&p.file);
            if is_test {
                drop.provides += 1;
                if drop.examples.len() < 3 && !drop.examples.contains(&p.file) {
                    drop.examples.push(p.file.clone());
                }
            }
            !is_test
        })
        .collect();
    let consumes = io
        .consumes
        .into_iter()
        .filter(|c| {
            let is_test = zzop_core::is_test_file(&c.file);
            if is_test {
                drop.consumes += 1;
                if drop.examples.len() < 3 && !drop.examples.contains(&c.file) {
                    drop.examples.push(c.file.clone());
                }
            }
            !is_test
        })
        .collect();
    (IoFacts { provides, consumes }, drop)
}
