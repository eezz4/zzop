//! Small, self-contained JSON-view mirrors of engine output types. Every type here mirrors ONE engine
//! type field-for-field (a plain
//! `From` conversion, no cross-type composition); the composed views (`AnalyzeOutputView` and friends)
//! stay in `output.rs` itself, which is where the composition logic — and the casing contract doc —
//! belongs.

use serde::Serialize;

use zzop_engine::{CacheStats, GitWindow};

/// A JSON-serializable mirror of `zzop_engine::CacheStats` (which does not itself derive `Serialize` — see
/// `AnalyzeOutputView`'s doc for why this crate mirrors rather than forks/modifies engine types).
/// `#[serde(rename_all = "camelCase")]` is a no-op today (`hits`/`misses` are already one word) — applied
/// for consistency with every other output-facing type at this boundary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CacheStatsView {
    hits: usize,
    misses: usize,
}

impl From<CacheStats> for CacheStatsView {
    fn from(c: CacheStats) -> Self {
        CacheStatsView {
            hits: c.hits,
            misses: c.misses,
        }
    }
}

/// JSON view over one `zzop_engine::PackLoaded` — the positive pack-load confirmation entry (pack id,
/// rule count as loaded, provenance `"dir"` | `"inline"`, per-pack in-scope file count). Borrowed
/// strings, same zero-copy-view convention as every other field; camelCase like every other
/// output-facing type at this boundary (`files_in_scope` -> `filesInScope`; the other three are
/// single words).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PackLoadedView<'a> {
    id: &'a str,
    rules: usize,
    source: &'a str,
    /// `"disabled"` | `"notAllowlisted"` — present ONLY on a pack that loaded and was never evaluated
    /// (`zzop_engine::PackNotRun`). Absent is the ordinary case and means the pack ran, so an ungated
    /// run's rows are byte-identical to what they were before this key existed: a disclosure that
    /// fires when there is nothing to disclose is the noise that teaches readers to skip disclosures.
    #[serde(skip_serializing_if = "Option::is_none")]
    did_not_run: Option<&'static str>,
    /// Per-pack applicability (D16 follow-up, `zzop_engine::PackLoaded::files_in_scope`'s doc): `0` on
    /// a loaded pack = "no analyzed file is in any of this pack's rules' scope" — zero findings from
    /// it means "out of scope", not "clean".
    ///
    /// Its PRESENCE is now also a claim: this pack ran. On a pack that did not, the same census rides
    /// [`Self::files_in_scope_if_enabled`] instead, because a positive scanned-file count on a pack
    /// that read nothing was the half of the old row that made "not analyzed" look like "analyzed and
    /// clean" (see `zzop_engine::PackLoaded`'s own doc for the measurement).
    #[serde(skip_serializing_if = "Option::is_none")]
    files_in_scope: Option<usize>,
    /// The SAME census as `filesInScope`, published under a counterfactual name on a pack that did not
    /// run: "this is what it would have looked at". Kept rather than dropped because it is exactly the
    /// number that says what re-enabling the pack would buy — 0 here means the pack has nothing to
    /// offer this tree even if switched on, which is a different remedy from a large number.
    #[serde(skip_serializing_if = "Option::is_none")]
    files_in_scope_if_enabled: Option<usize>,
    /// The rule-granularity half of the same census (`zzop_engine::PackLoaded::zero_admission_rules`'s
    /// doc): ids of this pack's rules whose own path gates admitted zero analyzed files — their zero
    /// findings are scope, never a clean bill. Serialized ONLY when non-empty (the `testPaths`
    /// additive-disclosure precedent: present exactly when it has something to say), so the common
    /// every-rule-admits-files entry — and the `filesInScope: 0` pack, where the pack-level zero
    /// already says "all of them" — stays byte-identical for existing consumers.
    ///
    /// Also dropped on a pack that DID NOT RUN, for the same reason it is dropped on a `filesInScope:
    /// 0` pack: admission ranks rules within a scan, and there was no scan. `didNotRun` already says
    /// "all of them" one level up.
    #[serde(skip_serializing_if = "slice_is_empty")]
    zero_admission_rules: &'a [String],
    /// The ids behind `rules` (`zzop_engine::PackLoaded::rule_ids`'s doc has the measurement that
    /// forced it): what this run could report from this pack, as a list rather than a count, so a
    /// consumer validating a `--rule`/`rule` filter can answer "is there such a rule here" instead of
    /// guessing from the pack prefix. Unconditionally serialized, unlike `zeroAdmissionRules` above —
    /// an omitted list would read as "this build declines to say", which is precisely the state a
    /// validator must be able to distinguish from "no such rule".
    rule_ids: &'a [String],
}

/// `skip_serializing_if` helper for a borrowed-slice field (serde hands the serializer `&&[String]`,
/// which `<[_]>::is_empty` cannot take directly).
fn slice_is_empty(s: &&[String]) -> bool {
    s.is_empty()
}

/// The one place the "loading is not running" split becomes JSON. `did_not_run` decides which of the
/// two scope keys the row carries, so the pair is mutually exclusive by construction — a row can never
/// publish both, and the census itself is copied unchanged into whichever key is honest for this run.
impl<'a> From<&'a zzop_engine::PackLoaded> for PackLoadedView<'a> {
    fn from(p: &'a zzop_engine::PackLoaded) -> Self {
        let ran = p.did_not_run.is_none();
        PackLoadedView {
            id: &p.id,
            rules: p.rules,
            source: &p.source,
            did_not_run: p.did_not_run.map(zzop_engine::PackNotRun::as_str),
            files_in_scope: ran.then_some(p.files_in_scope),
            files_in_scope_if_enabled: (!ran).then_some(p.files_in_scope),
            zero_admission_rules: if ran { &p.zero_admission_rules } else { &[] },
            rule_ids: &p.rule_ids,
        }
    }
}

/// JSON view over `zzop_engine::RuleOverridesApplied` — D13③'s positive "this disable/remap/allowlist
/// actually took effect" confirmation (`AnalyzeOutputView::rule_overrides_applied`'s own doc has the
/// full rationale). Borrowed strings, same zero-copy-view convention as every other field.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuleOverridesAppliedView<'a> {
    disabled: &'a [String],
    severity_remapped: &'a [String],
    only: &'a [String],
}

impl<'a> From<&'a zzop_engine::RuleOverridesApplied> for RuleOverridesAppliedView<'a> {
    fn from(r: &'a zzop_engine::RuleOverridesApplied) -> Self {
        RuleOverridesAppliedView {
            disabled: &r.disabled,
            severity_remapped: &r.severity_remapped,
            only: &r.only,
        }
    }
}

/// JSON view over `zzop_engine::GitWindow` — the operative git-window knobs (`gitWindow.recentDays` /
/// `gitWindow.since`) echoed alongside `scores`/`health`/`critical`/`seams` so a consumer diffing two
/// runs can tell which window produced which numbers (`AnalyzeOutput::git_window`'s own doc has the
/// full rationale). camelCase like every other output-facing type at this boundary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GitWindowView<'a> {
    recent_days: u32,
    since: &'a Option<String>,
}

impl<'a> From<&'a GitWindow> for GitWindowView<'a> {
    fn from(g: &'a GitWindow) -> Self {
        GitWindowView {
            recent_days: g.recent_days,
            since: &g.since,
        }
    }
}

/// JSON view over `zzop_engine::CoverageCensus` — the vocab-free structural coverage census (see that
/// type). Every field is a plain scalar copy (`join_contribution_zero` is the active-blindness FACT: this
/// tree extracted no JOINABLE io — zero provides AND zero keyed consumes — while analyzing `files > 0`, so
/// it is invisible to the cross-layer join) except the borrowed `declared_imports_by_ext` map. camelCase
/// like every other output-facing type at this boundary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CoverageCensusView<'a> {
    files: usize,
    /// The parser-claimed subset of `files` — see `zzop_engine::CoverageCensus::parser_dispatched`. `files`
    /// counts every file walked (docs/data/assets included) and was being misread as the repo's code size.
    parser_dispatched: usize,
    symbols: usize,
    /// RENAMED from `import_edges`/`importEdges` on 2026-07-31 — the name now states the membership
    /// rule (resolved in-tree edges only). See `zzop_engine::CoverageCensus::resolved_import_edges`.
    resolved_import_edges: usize,
    /// F4: the declared-side denominator for `resolved_import_edges`, per extension — counted BEFORE
    /// resolution, so package imports and unresolvable specifiers are still in it. ALWAYS serialized
    /// (no skip-if-empty): an extension key's ABSENCE means "never measured" (channel-less parser, or a
    /// Mode A envelope run, which measures nothing here), never 0 — see
    /// `zzop_engine::CoverageCensus::declared_imports_by_ext` for the full contract.
    declared_imports_by_ext: &'a std::collections::BTreeMap<String, usize>,
    io_provides: usize,
    io_consumes_keyed: usize,
    io_consumes_unresolved: usize,
    degraded: usize,
    join_contribution_zero: bool,
}

impl<'a> From<&'a zzop_engine::CoverageCensus> for CoverageCensusView<'a> {
    fn from(c: &'a zzop_engine::CoverageCensus) -> Self {
        CoverageCensusView {
            files: c.files,
            parser_dispatched: c.parser_dispatched,
            symbols: c.symbols,
            resolved_import_edges: c.resolved_import_edges,
            declared_imports_by_ext: &c.declared_imports_by_ext,
            io_provides: c.io_provides,
            io_consumes_keyed: c.io_consumes_keyed,
            io_consumes_unresolved: c.io_consumes_unresolved,
            degraded: c.degraded,
            join_contribution_zero: c.join_contribution_zero,
        }
    }
}

/// JSON view over one `zzop_engine::BlindnessClass` — an entry in the pinned silent-failure-class
/// registry (see that type). Static content, identical every run, surfaced so a consumer learns which
/// classes of blindness zzop does and does NOT yet detect (`status`). All fields are `&'static str`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlindnessClassView {
    id: &'static str,
    group: &'static str,
    summary: &'static str,
    status: &'static str,
}

/// The full registry as a serializable list — attached at the top level of every entry point's output
/// (a run-global honesty channel, never per-tree, so it is emitted once regardless of tree count).
pub(crate) fn disclosure_views() -> Vec<BlindnessClassView> {
    zzop_engine::blindness_registry()
        .iter()
        .map(|c| BlindnessClassView {
            id: c.id,
            group: c.group,
            summary: c.summary,
            status: c.status.as_str(),
        })
        .collect()
}

/// JSON view over `zzop_engine::NativeAnalyses` — the native-analysis roster (`nativeAnalyses`).
///
/// Both lists are serialized UNCONDITIONALLY, including when empty, and that breaks with the
/// skip-if-empty convention its DSL sibling `zeroAdmissionRules` follows. The break is the point: this
/// object exists to make a zero visible, and a channel whose population is "what I happened to find"
/// reports nothing on a clean run in bytes indistinguishable from a channel that never ran. `disabled:
/// []` beside `reportedInCrossLayerFindings` with 27 entries is a row a reader can compare; an absent
/// key is not.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativeAnalysesView<'a> {
    registered: usize,
    disabled: &'a [String],
    reported_in_cross_layer_findings: &'a [String],
}

impl<'a> From<&'a zzop_engine::NativeAnalyses> for NativeAnalysesView<'a> {
    fn from(n: &'a zzop_engine::NativeAnalyses) -> Self {
        NativeAnalysesView {
            registered: n.registered,
            disabled: &n.disabled,
            reported_in_cross_layer_findings: &n.reported_in_cross_layer_findings,
        }
    }
}
