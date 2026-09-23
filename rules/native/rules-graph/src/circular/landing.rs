//! The §27 landing clause `circular` splices AHEAD of its own imperative, in its own module so the one
//! spelling stays the one spelling — the position pin in `circular.rs` compares an index against this
//! constant, and a second copy would make that comparison meaningless.
//!
//! ## Why this rule needed one at all
//!
//! `circular` shipped an observation and then two instructions ("extract the shared pieces", "invert one
//! dependency direction") with nothing between them about what either instruction costs. It was the last
//! native rule that fires on this repo's corpus without a §27 leg-3 clause — the other six that fire
//! (`unimported-export`, `dead-candidates`, `mutating-route-no-auth`, `duplicate-route`, `unreachable`,
//! `orderby-unindexed`) each carry theirs, two of them through a shared constant and four in their own
//! prose because they have no family to share with.
//!
//! ## Why this is a constant of its own and not `unreachable`'s
//!
//! [`crate::unreachable`]'s `ISLAND_DELETION_LANDING` is about DELETING files and about the entrypoint
//! list that decides whether they are dead. Nothing is deleted here and no entrypoint set is involved:
//! the edit MOVES code across a module boundary and leaves every file in place. Splicing the sibling
//! sentence would send a reader to audit an entrypoint list that has no bearing on whether their
//! extraction reorders module evaluation — the "nearest sibling is the most dangerous reuse" trap
//! `rule-quality.md` §37 records.
//!
//! ## Why the cost is stated as CONDITIONAL rather than as a flat warning
//!
//! This analysis reads the import graph (`zzop_core::graph::circular_from_dep`) and not what any member
//! executes while it is being loaded. Both halves of that are load-bearing: a cycle of type-only
//! re-exports is erased before anything runs and breaking it costs nothing, while a cycle whose members
//! run statements at load time is reordered by the same edit. The rule cannot tell the reader which one
//! they are holding, so the sentence says so instead of asserting a cost it has not measured
//! (`rule-quality.md` §30/§32 — a message that teaches a reader to doubt true findings costs more than
//! the one it saves).
//!
//! Language-independent on purpose, because the rule is: the cycle's members, the languages in play and
//! the prescription's wording all belong to the finding and the imperative, so the position pin has ONE
//! spelling to compare an index against.

/// What breaking a cycle costs — spliced AHEAD of the `Break the cycle` imperative (`rule-quality.md`
/// §27 leg 3, §37).
///
/// The three runtime symptoms are named rather than summarised as "initialization order" because they
/// are what a reader greps their own logs for, and they differ by module system: a CommonJS `require`
/// of a partially-evaluated module yields `undefined`, an ESM import of a not-yet-initialized
/// `let`/`const`/`class` binding raises a TDZ `ReferenceError`, and a Python circular import surfaces as
/// a partially-initialized module. The "nothing fails to build" sentence is the part no reader can get
/// from the finding: the imports all still resolve after the edit and a type checker still sees a
/// declared value, so the first place this shows up is a run.
pub(super) const CYCLE_EXTRACTION_LANDING: &str = "BREAKING A CYCLE CAN CHANGE WHAT RUNS FIRST, AND THIS \
     READS IMPORTS RATHER THAN WHAT THEY EXECUTE: a cycle of type-only re-exports is erased before \
     anything runs and costs nothing to break, while a cycle whose members run statements at load time \
     is reordered by the same edit — a binding read today after the cycle has settled can end up read \
     during initialization instead, which is `undefined` under CommonJS, a TDZ ReferenceError for a \
     `let`/`const`/`class` under ESM, and a partially-initialized module under Python. Nothing fails to \
     build either way: every import still resolves and a type checker still sees a declared value, so \
     the first place this shows up is a run. Inverting the direction costs something this line cannot \
     name instead — the dependency becomes whoever constructs these objects to supply, and that caller \
     is outside the cycle printed above.";
