//! SHAPING — the one post-facade shaper every analyze-shaped entry point runs its engine output
//! through. Split out of `mod.rs` (which keeps the two ENTRY points: tree mode and envelope mode) when
//! `analyze --config`'s source-mode resolution pushed that file over the 300-line cap: "which tree is
//! this call about" and "what does the reply look like" are two questions, and only the second one is
//! shared with the envelope lane. Pure move — no behavior change.

use crate::output::{self, FindingFilters};

/// Shapes a facade output (already parsed to `serde_json::Value`, `disclosure` split off as its own
/// sibling — see both callers above) into the summary reply body EVERY analyze-shaped tool shares: the
/// ONE shaping implementation `analyze_summary`/`analyze_envelope_summary` both call, so the token-bomb
/// cap / truncation-disclosure / config-warning-merge contract this crate's doc promises cannot drift
/// per entry point. `leading` seeds the returned object's first keys (`analyze_summary`'s `path`/
/// `config` tree-mode echo; an empty map for envelope mode, which has neither) — every field below is
/// appended in the SAME order the pre-extraction inline code produced, so this refactor is a pure
/// behavior-preserving split, not a reshape.
pub(super) fn shape_analyze_output(
    mut summary: serde_json::Map<String, serde_json::Value>,
    output_view: &serde_json::Value,
    disclosure: serde_json::Value,
    mut config_warnings: Vec<serde_json::Value>,
    filters: &FindingFilters,
) -> serde_json::Value {
    let findings = output_view["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // The degraded-file path list gets the SAME shaping every other list gets (cap + disclosed
    // truncation, see `output::shape_list`) — forwarding it verbatim would bypass this module's own
    // token-bomb guard on a repo with thousands of degraded files. `coverage.degraded` (below) already
    // carries the full, uncapped COUNT, so this list is supplementary detail, never the only source of
    // the number.
    let degraded = output_view["degraded"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let (degraded_shown, degraded_truncated) = output::shape_list(
        &degraded,
        output::DEFAULT_DEGRADED_LIMIT,
        // No argument moves this cap (`limit` filters findings only), so the hint names the field
        // that DOES answer the question instead of a knob that would silently do nothing.
        "this list has a fixed cap and no argument raises it — `coverage.degraded` carries the \
         full, uncapped count",
    );
    // Config-loader warnings first, then the facade-level `configWarnings` entries riding the tree
    // output (engine-side config diagnostics, e.g. unknown-rule-id overrides) — merged into the one
    // config-honesty channel so the moved diagnostics are not silently dropped at this layer (see
    // `crate::warnings::facade_config_warnings` for the absent-field degradation contract).
    config_warnings.extend(crate::warnings::facade_config_warnings(output_view));
    summary.insert("fileCount".to_string(), output_view["fileCount"].clone());
    summary.insert(
        "degraded".to_string(),
        serde_json::Value::Array(degraded_shown),
    );
    // Positive pack-load confirmation ({id, rules, source}[], id-sorted, small and bounded — one
    // entry per loaded pack, never per finding) — forwarded whole, no cap needed.
    summary.insert(
        "packsLoaded".to_string(),
        output_view["packsLoaded"].clone(),
    );
    // Its legend — what `filesInScope` counts, what `zeroAdmissionRules` claims, and, only when a pack
    // really was gated off, what `didNotRun` means. `.get()`-gated like `ruleOverridesApplied` above
    // and for both of the same reasons: the facade OMITS the key when no pack loaded (a bare index
    // would turn that into JSON `null` noise), and an older engine/facade build without the field
    // degrades to "nothing to forward" rather than a null. An array of numbers whose legend reached
    // only the raw facade lane would leave the MCP reader — the primary reader — with the numbers
    // alone, which is the state the auditor measured.
    if let Some(packs_loaded_meaning) = output_view.get("packsLoadedMeaning") {
        summary.insert(
            "packsLoadedMeaning".to_string(),
            packs_loaded_meaning.clone(),
        );
    }
    // The NATIVE half of the same question, and its legend — which of this build's native analyses
    // could not have keyed the `findings` map above, split by cause. Forwarded HERE and not only in
    // the raw facade lane because this lane is where the defect was measured: `zzop analyze` over a
    // single-tree config reported no `cross-layer/*` key on any of nine corpus trees while the very
    // same config, run through the cross-layer join, produced 975 findings — and a reader of THIS
    // reply had no channel saying so. A field only the embedder lane carries would leave the CLI and
    // MCP readers exactly where the auditor found them. `.get()`-gated for the older-build
    // degradation reason above; both keys move together, since the numbers without the legend are
    // the half the same auditor already called a pointer rather than a statement.
    if let Some(native_analyses) = output_view.get("nativeAnalyses") {
        summary.insert("nativeAnalyses".to_string(), native_analyses.clone());
    }
    if let Some(native_analyses_meaning) = output_view.get("nativeAnalysesMeaning") {
        summary.insert(
            "nativeAnalysesMeaning".to_string(),
            native_analyses_meaning.clone(),
        );
    }
    // The tree's own manifest declaration of which files are BUILD surface, read off the engine view and
    // handed to the shaper as the third ordering input (`output::deployment_role`). Read by NAME and
    // defensively: an engine build that does not carry the field yet degrades to an empty set — the same
    // "no build surface declared" state every non-npm tree is genuinely in — rather than panicking.
    // Deliberately NOT re-inserted into `summary`: this is an ordering INPUT, not a reply field, and
    // forwarding it would change every reply's bytes on trees where the axis finds nothing to say.
    let build_script_paths: std::collections::HashSet<&str> = output_view["buildScriptPaths"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    summary.insert(
        "findings".to_string(),
        output::shape_findings(&findings, filters, &build_script_paths),
    );
    // The engine's own warnings, plus one this LAYER owns: a `rule` filter that can be proven to match
    // no rule this run could report (see `crate::warnings::unknown_rule_filter_warning`). It is
    // appended here rather than merged into `configWarnings` because the filter is not config — it is a
    // question asked of one reply, and the channel split this crate publishes is exactly that
    // (`configWarnings` = how the config was handled, `warnings` = what happened in this run).
    let mut warnings = output_view["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Some(rule) = filters.rule.as_deref() {
        if let Some(note) = crate::warnings::unknown_rule_filter_warning(output_view, rule) {
            warnings.push(serde_json::Value::String(note));
        }
    }
    summary.insert("warnings".to_string(), serde_json::Value::Array(warnings));
    // Per-tree structural coverage census, forwarded whole (a handful of scalars) — carries the
    // `joinContributionZero` blindness ASSERTION; a summary that drops the engine's own "this
    // tree contributed nothing to the join" fact is not a disclosure.
    summary.insert("coverage".to_string(), output_view["coverage"].clone());
    // The census above is tree-WIDE, and that is exactly what it cannot answer: which of this tree's
    // principal filetypes the resolved dependency graph does not contain. Those two facts decided
    // whether 306 `unimported-export` findings on directus were about the code or about a 587-file
    // frontend no parser claimed — and until now they existed only in `zzop coverage`, a subcommand
    // this reply never mentions. Compact and always present, next to the census it qualifies; see
    // `super::coverage_gaps` for what earns a row and what is pointed at instead of restated.
    summary.insert(
        "coverageGaps".to_string(),
        super::coverage_gaps::coverage_gaps(output_view),
    );
    summary.insert(
        "configWarnings".to_string(),
        serde_json::Value::Array(config_warnings),
    );
    // FOLDED, not forwarded (2026-07-29): the registry's counts and a pointer to its full text, never
    // the ~10.6KB of run-invariant prose the facade emits — see `output::disclosure`'s module doc for
    // why that stays inside decision 1c, and for the run-VARYING channels (`coverage`, `warnings`) it
    // deliberately does not touch. This was also the one list this shaper forwarded uncapped while
    // capping even `degraded` two fields above.
    summary.insert(
        "disclosure".to_string(),
        output::fold_disclosure(&disclosure),
    );
    if let Some(truncated) = degraded_truncated {
        summary.insert("degradedTruncated".to_string(), truncated);
    }
    // Rule-override confirmation ({disabled, severityRemapped, only} id lists) — forwarded whole, no cap
    // needed (bounded by the caller's own disabledRules/severityOverrides/packsOnly config size), same as
    // packsLoaded. Unlike packsLoaded (always present), the engine OMITS this field when no overrides
    // were requested, so a bare `output_view["ruleOverridesApplied"]` index would turn that omission
    // into JSON `null` noise; `.get()` preserves the omission instead — a MISSING field (older engine
    // output — shouldn't happen in-tree) degrades the same way, never surfacing as `null`.
    if let Some(rule_overrides_applied) = output_view.get("ruleOverridesApplied") {
        summary.insert(
            "ruleOverridesApplied".to_string(),
            rule_overrides_applied.clone(),
        );
    }
    // Compact git-signal summary (D-git-signal-asymmetry): the facade output carries full
    // `health`/`recommendations`/`critical` but this shaped summary otherwise drops all three
    // entirely — a mismatch with `analyze_repo`'s own description, which promises zero-config
    // "git signals included". Present only when git signals actually ran this tree (see
    // `architecture_summary`'s own doc); absent, never `null`, otherwise. Envelope mode never runs git
    // signals (no working tree to diff), so this key is naturally omitted for `analyze_envelope_summary`
    // too — the SAME "absent, not null" contract, no envelope-specific branch needed.
    if let Some(architecture) = super::architecture::architecture_summary(output_view) {
        summary.insert("architecture".to_string(), architecture);
    }
    // `gitWindow` ({recentDays, since}) — the engine's own always-serialized "which window produced
    // these numbers" echo (`null` when git signals did not run). `.get()`-defensive: forwarded
    // verbatim by name so an engine build that has not yet added the field degrades to "nothing to
    // forward" instead of a missing-key panic.
    if let Some(git_window) = output_view.get("gitWindow") {
        summary.insert("gitWindow".to_string(), git_window.clone());
    }
    // Rule timing — present ONLY when the run was instrumented (`zzop analyze --profile-rules`). The
    // facade always serializes its own `ruleTimings` key (`null` when profiling was off), so this is
    // gated on the VALUE being an array rather than on the key existing: an unprofiled reply must stay
    // byte-identical to what it was before this surface existed, not grow a `null` field. The reply's
    // key carries its own `meaning` string (see `output::timings`), which is why no sibling
    // `ruleTimingsMeaning` key appears here — the disclosure rides INSIDE the object it describes,
    // so a consumer that reads the numbers cannot fail to also have read what they omit.
    if let Some(rule_timings) = output::shape_rule_timings(output_view) {
        summary.insert("ruleTimings".to_string(), rule_timings);
    }
    // Cache PROVENANCE — how much of this reply was replayed rather than recomputed. Present only when a
    // cache was in play (absent, never null, like `architecture`/`ruleTimings` above), and carrying its
    // own `meaning` for the same reason those do. Until 2026-08-17 these counts reached a reader only
    // through `--profile-rules`, so the default reply could not answer "did this run actually look at my
    // code?" — see `output::cache_signal` for the manufactured-zero this closes.
    if let Some(cache) = output::shape_cache_signal(output_view) {
        summary.insert("cache".to_string(), cache);
    }
    serde_json::Value::Object(summary)
}
