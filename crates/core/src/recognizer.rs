//! Framework-recognizer declarations — the vocabulary-free MECHANISM a parser crate uses to state,
//! next to its own adapters, WHICH frameworks that parser can recognize at all.
//!
//! # Why this exists
//! Every silence tripwire this engine ships (`framework_silence`'s S1-S8) fires only on a tree that
//! ALREADY shows the symptom: a controller-shaped file with zero http provides, a server-framework
//! import with nothing extracted. That is the right shape for "this run went quiet unexpectedly", and
//! it is structurally unable to answer the question a user has BEFORE the first run — *does this tool
//! know my stack?* Measured consequence: a Flask project gets a reply that looks like a clean tree
//! unless it happens to trip a tripwire, and nothing anywhere says "no Flask recognizer exists".
//!
//! A [`FrameworkRecognizer`] is the machine-readable answer, declared by the crate that owns the
//! adapter. It is CAPABILITY-kind data in the coverage surface's sense: a fact about this BUILD, true
//! before any tree is walked and independent of every run.
//!
//! # The same deal as [`crate::sightline`]
//! Mechanism only, zero framework vocabulary — no framework name, extension, or channel string lives
//! in this module. Each declaration's data lives in the parser crate that owns the recognizer, so the
//! adapter and its disclosure cannot drift apart, and a guard asserts the declared set against the
//! adapter modules actually compiled in. `zzop_engine::framework_recognizers` composes every crate's
//! declarations, the same aggregator shape `rule_sightlines` already uses.
//!
//! # What a declaration does NOT claim
//! Presence here means the recognizer RUNS on that extension, never that it models every idiom of the
//! framework. The long tail is deliberately out of scope (`parser-expansion.md` §0 layer 3: custom
//! shapes are the adapter-injection tier). So this list closes "is my framework known at all", which
//! is the question that had no answer; it does not promise completeness within a known one.

/// One framework recognizer a parser crate compiles in.
///
/// Ordering/uniqueness is the aggregator's business, not this type's — see
/// `zzop_engine::framework_recognizers`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameworkRecognizer {
    /// The framework as ITS OWN ecosystem spells it (`fastapi`, `gin`, `axum`), lowercased. Not a
    /// zzop-internal module name: a user matching this against their stack is reading their own
    /// dependency list, not our source tree. When the two coincide it is because the module was named
    /// after the framework, which is the convention — but the guard binds the module, not the string.
    pub framework: &'static str,
    /// Extensions this recognizer can fire on, lowercased and without the dot. Quoted from the owning
    /// crate's own dispatch constant where it has one, never restated as a literal beside it.
    pub extensions: &'static [&'static str],
    /// Which cross-layer channels this recognizer FILLS, as the wire spells them (`io.provides`,
    /// `io.consumes`, `io.provides:db-table`). This is the field that makes the disclosure load-bearing
    /// rather than decorative: a language can look covered by recognizer COUNT while emitting only one
    /// half of the join, which is exactly the state java-21 was measured in (routes yes, egress none) —
    /// a service that CALLS another service was invisible, and no count would have shown it.
    pub emits: &'static [&'static str],
}

/// Channel spellings, so a declaration cannot invent a fourth one by typo. These are the names the
/// cross-layer join itself uses; a consumer grouping by channel compares against these constants.
pub mod channel {
    /// A route/handler DECLARATION — the provide side of the join.
    pub const PROVIDES: &str = "io.provides";
    /// An outbound call — the consume side. A parser with provides but no consumes sees only the
    /// services being called, never the calls its own code makes.
    pub const CONSUMES: &str = "io.consumes";
    /// A table/model declaration or query — the db half, keyed separately from http.
    pub const DB: &str = "io.provides:db-table";
}
