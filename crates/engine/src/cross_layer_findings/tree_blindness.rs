//! The per-tree blindness measurements the run-wide cross-layer gates read.
//!
//! Its own module rather than a struct in the parent because it is an INPUT contract, not a step of
//! the computation: every field is measured somewhere else entirely (each tree's own analyze pass) and
//! carried here unchanged. Splitting it out is also what kept the parent under the 300-line cap when
//! the consume-side half arrived (review ledger V63).

use std::collections::{BTreeMap, BTreeSet};

/// The two per-tree blindness measurements the run-wide cross-layer gates read. Both are taken in each
/// tree's OWN pass, where its root and candidate file list exist — the gates below must never re-derive
/// them, or they would judge a different population than the extractor actually saw.
///
/// They are one input rather than two parameters because they are two sides of one question: whether a
/// tree's silence is evidence. `visible_by_source` answers it on the PROVIDE side (this tree registered
/// routes lexically, so zero extracted provides means extraction missed them), `mostly_unread_by_source`
/// on the CONSUME side (most of this tree's files were never read, so zero consumes means the same).
/// Neither is a verdict; the gates AND each with its own positive evidence.
#[derive(Clone, Copy)]
pub(crate) struct TreeBlindness<'a> {
    /// source -> route registrations visible LEXICALLY in that tree, for the sources where the
    /// provide-side tripwire measured. A source absent here was never measured; see
    /// `provide_blind_sources`.
    pub visible_by_source: &'a BTreeMap<String, usize>,
    /// Sources whose analyzed files are MAJORITY unread by any structural parser. A source absent here
    /// was measured and is NOT majority-unread — absence is a negative, never an unknown.
    pub mostly_unread_by_source: &'a BTreeSet<String>,
}
