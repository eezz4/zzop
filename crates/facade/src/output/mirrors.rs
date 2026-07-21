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
    /// Per-pack applicability (D16 follow-up, `zzop_engine::PackLoaded::files_in_scope`'s doc): `0` on
    /// a loaded pack = "no analyzed file is in any of this pack's rules' scope" — zero findings from
    /// it means "out of scope", not "clean".
    files_in_scope: usize,
}

impl<'a> From<&'a zzop_engine::PackLoaded> for PackLoadedView<'a> {
    fn from(p: &'a zzop_engine::PackLoaded) -> Self {
        PackLoadedView {
            id: &p.id,
            rules: p.rules,
            source: &p.source,
            files_in_scope: p.files_in_scope,
        }
    }
}

/// JSON view over `zzop_engine::RuleOverridesApplied` — D13③'s positive "this disable/remap actually
/// took effect" confirmation (`AnalyzeOutputView::rule_overrides_applied`'s own doc has the full
/// rationale). Borrowed strings, same zero-copy-view convention as every other field.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RuleOverridesAppliedView<'a> {
    disabled: &'a [String],
    severity_remapped: &'a [String],
}

impl<'a> From<&'a zzop_engine::RuleOverridesApplied> for RuleOverridesAppliedView<'a> {
    fn from(r: &'a zzop_engine::RuleOverridesApplied) -> Self {
        RuleOverridesAppliedView {
            disabled: &r.disabled,
            severity_remapped: &r.severity_remapped,
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
/// it is invisible to the cross-layer join). camelCase like every other output-facing type at this
/// boundary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CoverageCensusView {
    files: usize,
    symbols: usize,
    import_edges: usize,
    io_provides: usize,
    io_consumes_keyed: usize,
    io_consumes_unresolved: usize,
    degraded: usize,
    join_contribution_zero: bool,
}

impl From<&zzop_engine::CoverageCensus> for CoverageCensusView {
    fn from(c: &zzop_engine::CoverageCensus) -> Self {
        CoverageCensusView {
            files: c.files,
            symbols: c.symbols,
            import_edges: c.import_edges,
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
