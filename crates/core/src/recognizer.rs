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
    /// Which channels this recognizer FILLS, as the engine spells them: the cross-layer io sides
    /// (`io.provides`, `io.consumes`, and the db kind split by side into `io.provides:db-table` and
    /// `io.consumes:db-table`) plus the one non-io channel,
    /// auth-guard evidence (`evidence.auth-guarded`). This is the field that makes the disclosure
    /// load-bearing rather than decorative: a language can look covered by recognizer COUNT while
    /// emitting only one half of the join, which is exactly the state java-21 was measured in (routes
    /// yes, egress none) — a service that CALLS another service was invisible, and no count would have
    /// shown it.
    ///
    /// Machine-checked against what each recognizer's own code constructs by
    /// `rule_contracts::recognizer_channels`, so a row cannot claim a side it does not fill (the `hono`
    /// defect) nor a channel it never builds. One row sits outside that binding and is pinned there by
    /// name — read that contract's `CHANNEL_NOT_CODE_DERIVED` before trusting this field on it.
    ///
    /// The db kind is split BY SIDE for exactly that reason. Until 2026-08-26 both sides collapsed
    /// into the single `io.provides:db-table` spelling, and the check that was supposed to catch an
    /// over-claim blessed it instead: its `channel_of` recomputed `"{PROVIDES}:db-table"` without
    /// consulting the side, so a module that only ever constructs `IoConsume` evidenced a channel
    /// whose NAME says provides. Three shipped rows read that way (`prisma client`, typescript's and
    /// rust's `raw sql`), and the coverage cross that lists "recognizers this build has for this
    /// channel" offered all three as evidence a table-DECLARATION channel could have been filled.
    pub emits: &'static [&'static str],
}

/// Channel spellings, so a declaration cannot invent a new one by typo. The io three are the names the
/// cross-layer join itself uses; a consumer grouping by channel compares against these constants.
pub mod channel {
    /// A route/handler DECLARATION — the provide side of the join.
    pub const PROVIDES: &str = "io.provides";
    /// An outbound call — the consume side. A parser with provides but no consumes sees only the
    /// services being called, never the calls its own code makes.
    pub const CONSUMES: &str = "io.consumes";
    /// A table/model DECLARATION — the db half of the provide side, keyed separately from http.
    /// Spelled `"{PROVIDES}:{kind}"`, the composite `RuleIoChannel::label` reproduces (pinned by
    /// `crate::rule_channels`'s own test).
    pub const DB_PROVIDES: &str = "io.provides:db-table";
    /// A QUERY against a table — the db half of the consume side. Split out on 2026-08-26: one
    /// spelling for both sides made a consume-only recognizer declare a channel whose name reads
    /// `io.provides`, and the coverage cross (`ioChannels.zeroExtraction`), which counts the two
    /// sides separately, then listed it as a build capability for the side it never fills. Which
    /// direction a db recognizer works in is the whole question on a tree that declares its tables
    /// somewhere zzop cannot read them, so the two cannot share a name.
    pub const DB_CONSUMES: &str = "io.consumes:db-table";
    /// Auth-guard EVIDENCE — the one channel that is not an io side at all. It says the row's modules
    /// ALSO construct guard evidence, not that they construct nothing else: `spring security` emits it
    /// alone (its modules build no io), while `fastapi`/`django`/`nestjs` emit it beside their io
    /// channels — a module may build both. The evidence feeds the framework-neutral decorator-guard
    /// side channel the engine's callgraph gate consumes (`analyze/native_rules/callgraph/decorator_gate.rs`), which
    /// SUPPRESSES or fills OTHER rules' findings (`mutating-route-no-auth`'s route-auth exemption)
    /// rather than adding rows to the join. Spelled after the `auth-guarded` attribute that evidence is
    /// ultimately minted into (`zzop_rules_http::mutating_route_no_auth::AUTH_GUARDED_ATTR` — quoted
    /// here as a literal because `zzop-core` sits below the rules crates). Without this channel a
    /// recognizer whose whole output is guard evidence had to either lie (`spring security` declared
    /// `io.provides` for months) or be undeclarable (`emits` must be non-empty).
    pub const AUTH_EVIDENCE: &str = "evidence.auth-guarded";
}
