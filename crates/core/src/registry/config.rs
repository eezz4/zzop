//! Config-driven gating — the `RuleConfig` shape plus the suppression / disabled-rule /
//! severity-override matching semantics every rule layer is gated through. See the `registry`
//! module doc for the overall design call.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

use regex::Regex;
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
    /// except these", so a caller who wants one pack out of the dozen bundled ones had to enumerate the
    /// other eleven and re-edit that list every time a pack ships. EMPTY MEANS NO ALLOWLIST (every
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
    /// Finding-level accept-list. See `is_suppressed`.
    pub suppressions: Vec<Suppression>,
    /// Config-wide finding-level filter applied to EVERY rule at once (the top-level `"exclude"` config
    /// key) — see `GlobalExclude`'s doc. Checked before the per-rule `suppressions` loop in
    /// `is_suppressed`. Default: empty (nothing globally excluded).
    #[serde(default)]
    pub global_excludes: Vec<GlobalExclude>,
}

/// Shared substring-vs-glob path-filter semantics: `glob` takes precedence over `path` when both are set;
/// a filter with neither matches every file; an unparseable glob fails safe (matches nothing). Both
/// `suppression_matches_path` and `global_exclude_matches_path` are thin wrappers over this so the two
/// filter shapes (`Suppression`, `GlobalExclude`) never diverge in matching behavior.
fn path_filter_matches(glob: &Option<String>, path: &Option<String>, file: &str) -> bool {
    if let Some(glob) = glob {
        return glob_matches(glob, file);
    }
    match path {
        None => true,
        Some(path) => file.contains(path.as_str()),
    }
}

/// True if a finding for `rule` (optionally in `file`) is suppressed by `config.global_excludes` (a
/// rule-agnostic match drops the finding regardless of `rule`) OR `config.suppressions`: an entry matches
/// when its `rule` equals `rule` AND its path filter matches (`suppression_matches_path`). A
/// path/glob-qualified entry never matches a fileless finding. Multiple entries are OR-ed.
pub fn is_suppressed(config: &RuleConfig, rule: &str, file: Option<&str>) -> bool {
    if let Some(f) = file {
        if config
            .global_excludes
            .iter()
            .any(|entry| global_exclude_matches_path(entry, f))
        {
            return true;
        }
    }
    config.suppressions.iter().any(|entry| {
        if entry.rule != rule {
            return false;
        }
        match file {
            Some(f) => suppression_matches_path(entry, f),
            // A path/glob-qualified entry never matches a fileless finding; only a filter-less entry does.
            None => entry.glob.is_none() && entry.path.is_none(),
        }
    })
}

/// True when `suppression`'s path filter matches `file` (glob takes precedence over the substring
/// `path`; a suppression with neither filter matches every file). Shares the exact semantics
/// `is_suppressed` applies, exposed so a caller can detect a path/glob filter that matches no scanned
/// file (a likely typo — see the engine's unmatched-suppression warning).
pub fn suppression_matches_path(suppression: &Suppression, file: &str) -> bool {
    path_filter_matches(&suppression.glob, &suppression.path, file)
}

/// True when `exclude`'s path filter matches `file`. Same substring-vs-glob semantics as
/// `suppression_matches_path`, over a `GlobalExclude` instead of a `Suppression` — exposed so a caller can
/// detect a top-level `exclude` entry that matches no scanned file (the engine's unmatched-exclude
/// warning, mirroring `unmatched_suppression_warnings`).
///
/// One deliberate divergence from `Suppression`: a FILTER-LESS entry (`path`/`glob` both `None`) matches
/// NOTHING here, whereas a filter-less `Suppression` matches everything for its one rule. A filter-less
/// global exclude would silently drop EVERY finding of EVERY rule — a whole-run blast radius no one can
/// mean (the CLI never emits one; only a malformed raw addon request can). Treating it as match-nothing
/// also routes it into the unmatched-exclude warning instead of a silent total suppression.
pub fn global_exclude_matches_path(exclude: &GlobalExclude, file: &str) -> bool {
    if exclude.glob.is_none() && exclude.path.is_none() {
        return false;
    }
    path_filter_matches(&exclude.glob, &exclude.path, file)
}

/// Whether `file` matches shell-style `glob` (full-path anchored). Compiled globs are memoized — a config
/// has a handful of distinct patterns but `is_suppressed` runs per finding, so recompiling per call would
/// be wasteful. An unparseable pattern is cached as `None` and matches nothing.
///
/// The cache is process-lifetime and never evicted. That's bounded for a one-shot CLI/`analyze` call (a
/// config carries only a few globs); a long-lived addon host that analyzes many distinct configs over its
/// lifetime would accumulate distinct glob keys without bound — swap in an LRU/per-call cache if that ever
/// becomes a real embedding.
fn glob_matches(glob: &str, file: &str) -> bool {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Regex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    let compiled = map
        .entry(glob.to_string())
        .or_insert_with(|| Regex::new(&glob_to_regex(glob)).ok());
    compiled.as_ref().is_some_and(|re| re.is_match(file))
}

/// Translate a shell-style path glob to an anchored regex source. `**` spans `/` (a `**/` or `/**`
/// boundary also matches zero directories); `*` and `?` stay within a single path segment; `{a,b}`
/// alternates (nesting not supported); every other character is matched literally.
fn glob_to_regex(glob: &str) -> String {
    let bytes = glob.as_bytes();
    let mut re = String::from("^");
    let mut brace_depth: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'*' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    // `**` — spans path separators.
                    i += 1;
                    if bytes.get(i + 1) == Some(&b'/') {
                        // `**/` — also match zero leading directories.
                        re.push_str("(?:.*/)?");
                        i += 1;
                    } else if re.ends_with('/') {
                        // `/**` at end — also match zero trailing directories.
                        re.truncate(re.len() - 1);
                        re.push_str("(?:/.*)?");
                    } else {
                        re.push_str(".*");
                    }
                } else {
                    // `*` — within a single segment.
                    re.push_str("[^/]*");
                }
            }
            b'?' => re.push_str("[^/]"),
            b'{' => {
                brace_depth += 1;
                re.push_str("(?:");
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                re.push(')');
            }
            b',' if brace_depth > 0 => re.push('|'),
            // Escape every regex metacharacter so the remaining glob text is matched literally.
            c => {
                let ch = c as char;
                if "\\.+()|[]^$".contains(ch) {
                    re.push('\\');
                }
                re.push(ch);
            }
        }
        i += 1;
    }
    re.push('$');
    re
}

/// True if `rule_id` is NOT in `config.disabled_rules` — exact string match, no prefix/glob semantics (see
/// `disabled_rules`'s own doc). Applies uniformly to a bare native-analysis id, a whole DSL pack id, or a
/// full `"<pack>/<rule>"` id — this function does not distinguish layers, it only compares strings. All
/// three id shapes are honored end to end: pack ids and `"<pack>/<rule>"` ids are both enforced
/// by `zzop_engine::pipeline::run_file_pass` before a pack ever reaches per-file evaluation (a disabled pack
/// id drops the whole pack; a disabled `"<pack>/<rule>"` id drops just that rule, via `gate_pack_rules`),
/// while bare native ids are enforced at their own call sites (e.g. `register_native_analyses`'s ids
/// checked directly against `is_enabled` before the corresponding analysis runs).
pub fn is_enabled(config: &RuleConfig, rule_id: &str) -> bool {
    !config.disabled_rules.iter().any(|d| d == rule_id)
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
