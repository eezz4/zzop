//! Config-driven gating — the `RuleConfig` shape plus the suppression / disabled-rule /
//! severity-override matching semantics every rule layer is gated through. See the `registry`
//! module doc for the overall design call.
//!
//! The PATH-matching half lives in [`path_filter`] and is re-exported from here, so a caller still
//! reaches everything through `zzop_core::registry::config`. The seam is the question each half
//! answers: this file decides whether a rule is EVALUATED (ids only), `path_filter` decides whether a
//! produced finding is REPORTED (paths only). Neither calls the other.

mod path_filter;

pub use path_filter::{
    glob_matches, global_exclude_matches_path, is_suppressed, suppression_matches_path,
};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{finding::Finding, Severity};

/// One accepted-finding entry. Two mutually-exclusive path filters: `path` (plain substring) and `glob`
/// (a shell-style glob) — see `is_suppressed` for precedence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suppression {
    /// The finding's stable rule id (a DSL pack rule id `"<pack>/<rule>"` or a native analysis id) —
    /// matched for exact equality.
    pub rule: String,
    /// Optional path filter. Absent = suppress `rule` everywhere; present = suppress only findings whose
    /// file contains this string (case-sensitive substring containment). Kept alongside `glob` because a
    /// bare fragment like `legacy/` is the common case and needs no glob semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional glob filter (e.g. `**/app/**/{page,layout}.tsx`). Present = suppress only findings whose
    /// file matches the glob (full-path anchored: `*`/`?` stay within a path segment, `**` spans `/`,
    /// `{a,b}` alternates). Takes precedence over `path` when both are set. An unparseable glob matches
    /// nothing (fails safe — the finding is NOT suppressed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
}

/// A config-wide, rule-agnostic REPORT-level filter: a file matching a `global_excludes` entry has
/// findings from EVERY rule dropped, not just one. Same mutually-exclusive `path`/`glob` filter shape as
/// `Suppression` (minus `rule`, since there is no rule to match) — see `Suppression`'s own field docs for
/// the exact substring-vs-glob semantics, shared verbatim via `path_filter_matches`.
///
/// Never a scan-skip: a matching file is still parsed (so the dep graph / dead-code analysis stays
/// correct) — only what the run REPORTS about it is dropped. **Every reporting channel consumes it**, and
/// the count is deliberately not written here: it was "two consumers, deliberately" until 2026-07-29 and
/// was already stale by the end of that day. The channels are `is_suppressed` below (findings, alongside
/// per-rule suppressions), `zzop_metrics::build_recommendations` via `BuildRecInput::excludes`,
/// `cross_layer_findings`' merged config, `compute_criticality` (the summary's `architecture.criticalTop`),
/// and the per-metric violation lists under `scores.*` — each wired with its own test; ask those modules,
/// not this sentence. The principle is what belongs here: the author's "do not report on these paths" is
/// ONE statement, so a channel that ignores it makes the run contradict the config that produced it.
///
/// TWO ROLES, ONE KEY (2026-07-29). A path is either a finding's ANCHOR or its EVIDENCE, and the role
/// decides the treatment: an excluded anchor drops the finding whole, an excluded evidence path keeps the
/// finding and redacts that path out of it. Before this the filter read the anchor alone, so what it
/// enforced was "do not ANCHOR a finding here" while this doc promised "do not NAME this path" — and a
/// relational finding anchored on one side printed the other side's excluded path in its own message. See
/// `super::redact` for why that is one key rather than two, and `Finding::evidence_paths` for why the
/// paths are a typed field rather than a per-rule table of `data` keys.
///
/// TWO EXEMPTIONS, and they are exemptions rather than omissions. `health.pain` is a whole-tree rollup —
/// filtering it would make the number incomparable with any other run. `warnings` is the config-diagnostics
/// channel, and one of the things it warns about is an `exclude` so broad the problem only LOOKS absent;
/// filtering it would let the filter erase its own warning. Rows keyed by a slice or module rather than a
/// file path (`cohesion.slices`, `sdp.violations`, `mainSequence.modules`) are likewise unfiltered: each is
/// itself a whole-directory rollup, so the `pain` argument applies to them too.
///
/// FILTERING IS EMISSION-TIME, NEVER COMPUTATION-TIME. An excluded file is still parsed, still appears in
/// `nodes` and the dep graph, and still counts toward every score, every denominator, and every OTHER
/// file's blast radius — it is a real importer whether or not the run is allowed to name it. Cache-neutral, exactly like
/// `suppressions` (see `RuleConfig::suppressions`'s doc and `zzop_engine::cache`'s fingerprint doc) —
/// never part of the IR/fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalExclude {
    /// Optional path filter (plain substring). See `Suppression::path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional glob filter (full-path anchored). Takes precedence over `path` when both are set. See
    /// `Suppression::glob`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
}

/// The one user-facing config shape both rule layers (native analyses and DSL packs) are gated through.
/// Covers the enabled/severity/disabled/suppressions surface — deliberately NOT vocabulary/threshold
/// plumbing (out of scope here; see module doc).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleConfig {
    /// Rule/pack/native-analysis ids to skip entirely. Exact string match against a rule's full id — no
    /// prefix/glob semantics.
    pub disabled_rules: Vec<String>,
    /// DSL pack ALLOWLIST — when non-empty, a pack whose id is absent from it does not run. The opt-IN
    /// half of the pack axis, and the reason it exists: `disabled_rules` can only express "everything
    /// except these", so a caller who wants ONE bundled pack had to enumerate every
    /// other one and re-edit that list every time the bundle changed size. EMPTY MEANS NO ALLOWLIST (every
    /// loaded pack runs), never "allow nothing" — the absent-is-not-a-claim direction every optional
    /// filter in this struct takes.
    ///
    /// Scoped to DSL PACKS ONLY, deliberately: native analyses (`dead-candidates`, `scores`, …) carry
    /// bare ids in the same id space but are not packs, so an allowlist of pack ids says nothing about
    /// them and must not silently switch them off — `disabled_rules` remains their lever. Composes with
    /// `disabled_rules` rather than overriding it: the allowlist selects, `disabled_rules` still
    /// subtracts, so `only ["security"] + disabled ["security/hardcoded-secret"]` means what it reads
    /// like. Enforced in [`is_pack_enabled`], which is what every pack-level gate calls.
    pub only_packs: Vec<String>,
    /// Per-rule severity remap, keyed by the same id space as `disabled_rules`. Exists because that id
    /// space spans both layers — native analyses and DSL pack rules — and a user may want to
    /// promote/demote a specific id without forking the pack (a DSL rule's severity otherwise comes from
    /// the pack JSON's own `severity` field, a native one from wherever the finding is built). Applied in
    /// `apply_severity_override`, on the finding — not through the registry. `BTreeMap` (not `HashMap`)
    /// so config round-trips (serialize/compare/hash) are deterministic.
    pub severity_overrides: BTreeMap<String, Severity>,
    /// Ids that SHIP OFF: registered, gated, and not evaluated unless this config names them.
    ///
    /// The third state in an id's life, and it exists because the other two could not express it.
    /// `disabled_rules` says "the user switched this off" and an absent id says "it ran"; a rule the
    /// PROJECT ships off is neither, and folding it into `disabled_rules` would make every disclosure
    /// that reads that list attribute the choice to the user.
    ///
    /// TURNING ONE ON TAKES NO NEW VOCABULARY. `rules: { "<id>": "warn" }` already routes to
    /// `severity_overrides`, and naming an id with a severity is the user saying they want it — so
    /// [`is_enabled`] reads that map as the opt-in. A new `"on"` value would be a second spelling of a
    /// gesture the surface already has. The object form (`{ "exclude": [...] }`, no severity) counts too
    /// and lands in `suppressions` instead; [`names_rule`] holds both, and why one alone is not enough.
    ///
    /// EMPTY IS THE DEFAULT and means "nothing ships off" — the kernel holds no id, ever. The list is
    /// filled by the composing layer from the owning rules crates (see `zzop_engine::register_all_native`
    /// and `zzop_rules_graph::DEFAULT_OFF`), which is the same split every other native-id table takes.
    pub default_off: Vec<String>,
    /// Finding-level accept-list. See `is_suppressed`.
    pub suppressions: Vec<Suppression>,
    /// Config-wide finding-level filter applied to EVERY rule at once (the top-level `"exclude"` config
    /// key) — see `GlobalExclude`'s doc. Checked before the per-rule `suppressions` loop in
    /// `is_suppressed`. Default: empty (nothing globally excluded).
    #[serde(default)]
    pub global_excludes: Vec<GlobalExclude>,
}

/// True if `rule_id` is NOT in `config.disabled_rules`, AND — for an id in `config.default_off` — this
/// config NAMED it. Exact string match, no prefix/glob semantics (see `disabled_rules`'s own doc).
/// Applies uniformly to a bare native-analysis id, a whole DSL pack id, or a
/// full `"<pack>/<rule>"` id — this function does not distinguish layers, it only compares strings. All
/// three id shapes are honored end to end: pack ids and `"<pack>/<rule>"` ids are both enforced
/// by `zzop_engine::pipeline::run_file_pass` before a pack ever reaches per-file evaluation (a disabled pack
/// id drops the whole pack; a disabled `"<pack>/<rule>"` id drops just that rule, via `gate_pack_rules`),
/// while bare native ids are enforced at their own call sites (e.g. `register_native_analyses`'s ids
/// checked directly against `is_enabled` before the corresponding analysis runs).
pub fn is_enabled(config: &RuleConfig, rule_id: &str) -> bool {
    if config.disabled_rules.iter().any(|d| d == rule_id) {
        return false;
    }
    // An id that ships off runs only when this config named it. Placed HERE rather than at the three
    // orchestrator call sites that gate the shipped-off analyses today, because every gate in the
    // codebase already goes through this function: a fourth call site added later inherits the rule
    // instead of having to remember it, which is the failure this repo keeps paying for elsewhere.
    if config.default_off.iter().any(|d| d == rule_id) {
        return names_rule(config, rule_id);
    }
    true
}

/// Whether this config NAMED `rule_id` in a way that asks for it to run — the opt-in half of the
/// shipped-off gate, and deliberately BOTH spellings the `rules` surface already has.
///
/// The surface maps one config key to two engine lists depending on the value's shape:
/// `"<id>": "info"` becomes a `severity_overrides` entry, while `"<id>": { "exclude": [...] }` with no
/// `severity` becomes only `suppressions` entries (`zzop_config::mapper::options::rules_map`). Reading
/// the severity map alone would therefore have made the second spelling a SILENT NO-OP: a user who wrote
/// out which paths a shipped-off rule should skip would get no findings at all, with nothing in the reply
/// saying the rule they had just configured never ran — and `docs/getting-started.md` uses exactly that
/// spelling, on exactly one of these ids, as its worked example.
///
/// Excluding paths from a rule is asking for the rule on every OTHER path; nobody writes an exclusion for
/// an analysis they do not want. So a suppression keyed to the id counts as naming it, and the two
/// spellings mean the same thing here, which is the only reading under which the surface stays one
/// surface. `global_excludes` deliberately does not count — it names no id, so it cannot be a statement
/// about one.
fn names_rule(config: &RuleConfig, rule_id: &str) -> bool {
    config.severity_overrides.contains_key(rule_id)
        || config.suppressions.iter().any(|s| s.rule == rule_id)
}

/// The gate every PACK-level call site uses: [`is_enabled`] plus [`RuleConfig::only_packs`]. Split from
/// `is_enabled` rather than folded into it because the two take different id spaces — `is_enabled` is
/// called with pack ids, `"<pack>/<rule>"` ids AND bare native ids, and an allowlist of pack ids must
/// not answer for the other two (a rule id is never in it, so folding would disable every rule the
/// moment an allowlist existed).
pub fn is_pack_enabled(config: &RuleConfig, pack_id: &str) -> bool {
    if !config.only_packs.is_empty() && !config.only_packs.iter().any(|p| p == pack_id) {
        return false;
    }
    is_enabled(config, pack_id)
}

/// Returns `finding` with its severity replaced by `config.severity_overrides[finding.rule_id]`, if any
/// override is configured for that id; otherwise returns `finding` unchanged. See
/// `RuleConfig::severity_overrides` doc.
pub fn apply_severity_override(config: &RuleConfig, finding: Finding) -> Finding {
    match config.severity_overrides.get(&finding.rule_id) {
        Some(&severity) => Finding {
            severity,
            ..finding
        },
        None => finding,
    }
}
