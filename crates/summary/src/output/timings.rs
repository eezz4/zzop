//! RULE TIMING — the shaping and the DISCLOSURE for `--profile-rules`.
//!
//! Split into its own file rather than inlined at the two call sites because the disclosure is the
//! larger half of this surface and it must be worded identically on every lane: a timing report that
//! does not say what it is missing hands the reader a confident wrong answer, which is the defect class
//! this repo guards hardest. The numbers are run-VARYING and ride in the data; the prose is
//! run-INVARIANT and ships once, the same split `output::disclosure` applies to the blindness registry.

/// The per-invocation run knobs a HOST passes to the shared analysis entry points — today just rule
/// timing. Separate from [`super::FindingFilters`] on purpose: those three knobs choose WHICH FINDINGS
/// the reply shows (a view over the result), while this one chooses WHETHER THE RUN IS INSTRUMENTED (a
/// property of the run itself). Folding them together would have put a profiler switch behind a name
/// that promises filtering.
///
/// `Default` = no instrumentation, which is what every existing 3-argument entry point passes; see
/// `analyze_summary`'s doc for why those wrappers stayed rather than every host growing an argument.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunKnobs {
    /// Populate `AnalyzeOutput::rule_timings` for this run (`zzop analyze --profile-rules`).
    pub profile_rules: bool,
}

/// The run-invariant half of the report. Every sentence here is a thing a reader would otherwise get
/// wrong, and the FIRST one is the load-bearing one: on a warm cache this list is not merely
/// approximate, it silently loses a whole CLASS of rule.
///
/// The warm-run wording is MEASURED, not reasoned (2026-08-02, 4-file tree, `cacheDir` set): a cold run
/// timed 145 rules, the immediately-following warm run timed 10 — and those 10 were exactly the
/// whole-graph native analyses (`dead-candidates`, `circular`, `unreachable`, `unimported-export`,
/// `duplicate-route`, `unprovided-consume`, `route-shadowing`, `schema-usage`, and the two whole-graph
/// `http/*` passes), which run post-assembly and are not per-file cached. Saying "empty on a warm run"
/// would have been the confident wrong answer this field exists to prevent.
const MEANING: &str = "Wall-clock time attributed to each DSL rule and whole-graph native analysis that \
     actually executed this run, sorted by `nanos` descending (deterministic `ruleId`-ascending \
     tie-break). READ `cacheHitFiles` FIRST: a file served whole from the cache never re-runs its \
     per-file rules and therefore contributes NO timing to them, so on a warm run this report is \
     structurally incomplete. When `cacheHitFiles` equals `fileCount` the PER-FILE rules are missing \
     ENTIRELY and what remains is only the whole-graph native analyses, which run post-assembly and are \
     never cache-served — a much shorter list that is not a ranking of your rule costs. Profile against \
     a cold cache (delete the `cacheDir`, or set `\"cacheDir\": null`) to time the whole tree. `nanos` \
     is wall-clock and jitters run to run: rank rules by relative cost WITHIN one run rather than \
     diffing raw `nanos` across runs. Timing never changes which rules run or what they report.";

/// Shapes the facade's `ruleTimings` array into the reply's `ruleTimings` object, or `None` when
/// profiling was off (the facade serializes `null` there, and an absent key — never a `null` one — is
/// what keeps an unprofiled reply byte-identical to what it was before this surface existed).
///
/// `output_view` is one tree's facade output; the cache counts are read off that SAME tree's `cache`
/// field so the disclosure's numbers cannot describe a different run than the timings do. A facade
/// output with no `cache` (caching disabled) reports `cacheHitFiles: 0`, which is the truth: nothing was
/// served from a cache, so nothing is missing for that reason.
pub(crate) fn shape_rule_timings(output_view: &serde_json::Value) -> Option<serde_json::Value> {
    let rules = output_view.get("ruleTimings")?.as_array()?;
    let total_nanos: u128 = rules
        .iter()
        .filter_map(|r| r.get("nanos").and_then(serde_json::Value::as_u64))
        .map(u128::from)
        .sum();
    let cache_hits = output_view
        .get("cache")
        .and_then(|c| c.get("hits"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    Some(serde_json::json!({
        "rules": rules,
        "ruleCount": rules.len(),
        "totalNanos": serde_json::Value::from(u64::try_from(total_nanos).unwrap_or(u64::MAX)),
        // The two numbers the meaning text tells the reader to check before believing the list.
        "cacheHitFiles": cache_hits,
        "fileCount": output_view.get("fileCount").cloned().unwrap_or(serde_json::Value::Null),
        "meaning": MEANING,
    }))
}
