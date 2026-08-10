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
    summary.insert(
        "findings".to_string(),
        output::shape_findings(&findings, filters),
    );
    summary.insert("warnings".to_string(), output_view["warnings"].clone());
    // Per-tree structural coverage census, forwarded whole (a handful of scalars) — carries the
    // `joinContributionZero` blindness ASSERTION; a summary that drops the engine's own "this
    // tree contributed nothing to the join" fact is not a disclosure.
    summary.insert("coverage".to_string(), output_view["coverage"].clone());
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
    if let Some(architecture) = architecture_summary(output_view) {
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
    serde_json::Value::Object(summary)
}

/// Builds the reply's compact `architecture` object from the facade output's `health`/
/// `recommendations`/`critical` fields — `None` (never `serde_json::Value::Null`) when `health`
/// itself is absent or JSON `null` (git signals did not run this tree), so the reply OMITS the key
/// entirely rather than growing a null `architecture` field on every git-less run. Deliberately
/// capped to ~10 lines of JSON: `pain` (the health scalar), the top-ROI `recommendations[0]`
/// (`{id, severity, topItem}`, null-safe when there are no recommendations or the top one has no
/// items), and up to 3 paths off the engine's own SIZE-WEIGHTED `critical` list (`blast_radius * ln(loc+2)`, NOT blast radius alone — re-sorting by `blastRadius` does not reproduce these three) — named
/// `criticalTop`, NOT "hotspot": the engine's `hotspotScore` is a DIFFERENT metric (churn
/// `changeCount x loc`, `nodes[].hotspotScore`), and reusing that word here would invite joining two
/// non-matching rankings. The full arrays never
/// ride this summary (see analyze_repo's own description: they are the direct `zzop-facade`
/// embedding lane's job).
fn architecture_summary(output_view: &serde_json::Value) -> Option<serde_json::Value> {
    let health = output_view.get("health")?.as_object()?;
    let pain = health.get("pain")?.clone();
    // `pain`'s DENOMINATOR travels with it, always (2026-08-08). This summary is the only place the CLI
    // and MCP surfaces publish any score at all — the full `scores` object rides the direct
    // `zzop-facade` embedding lane and never reaches here — so before this, `pain` was a single folded
    // scalar with no way to tell how much of the structure it actually described. That is precisely the
    // shape `zzop_facade::query_coverage` forbids ("there is deliberately NO single score field, and one
    // must never be added"), and `pain` was sitting one crate away from the prohibition.
    //
    // `measuredWeight / totalWeight` is the fraction of the weighted metric table that had a population
    // to score over; `pain: null` with `measuredWeight: 0` is the honest "nothing was measurable" state,
    // which used to serialize as a confident `pain: 0`. Forwarded by name and `.get()`-defensive, the
    // same degradation contract every other field in this shaper uses.
    let measured_weight = health.get("measuredWeight").cloned();
    let total_weight = health.get("totalWeight").cloned();
    let top_recommendation = output_view["recommendations"]
        .as_array()
        .and_then(|recs| recs.first())
        .map(|rec| {
            let top_item = rec["items"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|item| item["path"].as_str());
            serde_json::json!({ "id": rec["id"], "severity": rec["severity"], "topItem": top_item })
        });
    let critical_top: Vec<&str> = output_view["critical"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .take(3)
                .filter_map(|f| f["path"].as_str())
                .collect()
        })
        .unwrap_or_default();
    let mut architecture = serde_json::Map::new();
    architecture.insert("pain".to_string(), pain);
    if let Some(measured_weight) = measured_weight {
        architecture.insert("painMeasuredWeight".to_string(), measured_weight);
    }
    if let Some(total_weight) = total_weight {
        architecture.insert("painTotalWeight".to_string(), total_weight);
    }
    architecture.insert(
        "painMeaning".to_string(),
        serde_json::json!(
            "Composite structural debt over the metrics that HAD something to measure, renormalized \
             onto the full weight table (0 = clean, higher = worse, ~186 = every weighted metric at \
             its worst). `painMeasuredWeight` / `painTotalWeight` is how much of that table was \
             actually measurable on this tree: read a low ratio as \"this number describes a minority \
             of the structure\", not as a better score. `pain: null` means NO metric had a population \
             — absence of data, never 0. Renormalizing is what stops an unmeasurable axis (a metric \
             defined over a convention this tree never adopted) from making the repo look healthier by \
             silently scoring 100. The per-metric populations behind it ride `scores.*` in the direct \
             zzop-facade output."
        ),
    );
    architecture.insert("topRecommendation".to_string(), top_recommendation.into());
    architecture.insert("criticalTop".to_string(), critical_top.into());
    Some(serde_json::Value::Object(architecture))
}
