//! `analyze_repo` (and CLI `zzop analyze <path>`). Also hosts
//! `analyze_envelope_summary` (`analyze_envelope` tool / CLI `zzop analyze-envelope <file>`, Mode
//! A) — the two share one post-facade shaper (`shape_analyze_output`) so the same cap/disclosure/
//! config-warning contract holds for both entry points.

use crate::output::{self, FindingFilters, Verbosity};

#[cfg(test)]
mod tests;

/// Analyze ONE tree: config auto-discovery via `zzop-config`, execution via `zzop-facade` (the same
/// engine code path as the Node addon), summary-first shaping. A config declaring multiple trees is a
/// guided error — that analysis is `cross_repo`'s job.
pub fn analyze_summary(path: &str, filters: &FindingFilters) -> Result<String, String> {
    // Absolutized at the host boundary (see `paths`): `zzop-config` requires an absolute root.
    if path.trim().is_empty() {
        return Err("path is empty — pass the tree's root directory".to_string());
    }
    let root = crate::paths::absolutize(path);
    if !root.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    let loaded = zzop_config::load_for_root(&root).map_err(|e| e.to_string())?;
    // `disclosure` is the facade's run-global blindness-class registry (which failure classes zzop
    // does/does NOT detect) — the meta-honesty channel an AI consumer needs alongside the active
    // `warnings`; it rides at the top level of every facade output and is forwarded, never dropped.
    let (output_view, disclosure) = match loaded.method {
        zzop_config::Method::Analyze => {
            let out = zzop_facade::analyze_json(&loaded.request.to_string())?;
            let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
            let disclosure = v["disclosure"].clone();
            (v, disclosure)
        }
        zzop_config::Method::AnalyzeTrees => {
            let tree_count = loaded.request["trees"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            if tree_count > 1 {
                return Err(format!(
                    "the config at {} defines {tree_count} trees — use the cross_repo tool with configPath to run the cross-layer join, or point analyze_repo at one tree root directly",
                    loaded
                        .config_path
                        .as_deref()
                        .unwrap_or(&root)
                        .display()
                ));
            }
            let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
            let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
            (v["trees"][0]["output"].clone(), v["disclosure"].clone())
        }
    };
    // Config-loader warnings, collected first; the facade-level `configWarnings` entries riding the
    // tree output (engine-side config diagnostics, e.g. unknown-rule-id overrides) are merged onto
    // these later, in `shape_analyze_output` (see `crate::config_warnings::facade_config_warnings`).
    let config_warnings: Vec<serde_json::Value> = loaded
        .warnings
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    // The RESOLVED absolute path, never the raw argument — a live-fire gap: `path: "."` echoed back
    // the literal `.` with the actual analyzed directory never disclosed anywhere in the reply. `root`
    // is this same absolutized value the analysis itself ran against (see `crate::paths`), so echoing
    // it costs nothing extra and closes the gap.
    let mut leading = serde_json::Map::new();
    leading.insert(
        "path".to_string(),
        serde_json::json!(root.display().to_string()),
    );
    leading.insert(
        "config".to_string(),
        serde_json::json!(loaded
            .config_path
            .as_deref()
            .map(|p| p.display().to_string())),
    );
    let summary = shape_analyze_output(leading, &output_view, disclosure, config_warnings, filters);
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}

/// `analyze_envelope` (and CLI `zzop analyze-envelope <envelope.json>`) — Mode A: a full
/// Normalized-AST envelope REPLACES native parsing entirely (contrast `validate_envelope`, which only
/// checks the envelope's shape and never runs analysis; and Mode B mount/overlay requests, which merge
/// external symbols ON TOP of a natively-parsed tree). Runs via `zzop_facade::analyze_envelope_json` —
/// the SAME `AnalyzeOutputView`-shaped output `analyze_json`/`analyze_trees_json` produce, so it goes
/// through the identical [`shape_analyze_output`] this module's tree-mode path uses: a shaping fix
/// (a cap, a warning merge) lands for both entry points at once instead of drifting per host.
pub fn analyze_envelope_summary(
    envelope_json: &str,
    filters: &FindingFilters,
) -> Result<String, String> {
    if envelope_json.trim().is_empty() {
        return Err(
            "envelopeJson is empty — pass a Normalized AST envelope JSON document".to_string(),
        );
    }
    // Envelope mode has no filesystem root/config file to auto-discover (unlike `analyze_summary`'s
    // `zzop_config::load_for_root`) — an envelope carries no location the engine can re-read
    // (`docs/NORMALIZED_AST.md`). `"{}"` is the SAME "zero-config = full analysis" default
    // `analyze_envelope_json` itself documents at the facade layer (bundled packs injected as inline
    // seeds, no disabledRules/severityOverrides/suppressions/mounts) — the MCP surface takes
    // `envelopeJson` only, so this is the minimal valid `EnvelopeAnalyzeRequest` construction, not a
    // shortcut around it.
    let out = zzop_facade::analyze_envelope_json(envelope_json, "{}")?;
    let output_view: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    let disclosure = output_view["disclosure"].clone();
    let summary = shape_analyze_output(
        serde_json::Map::new(),
        &output_view,
        disclosure,
        Vec::new(),
        filters,
    );
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}

/// Shapes a facade output (already parsed to `serde_json::Value`, `disclosure` split off as its own
/// sibling — see both callers above) into the summary reply body EVERY analyze-shaped tool shares: the
/// ONE shaping implementation `analyze_summary`/`analyze_envelope_summary` both call, so the token-bomb
/// cap / truncation-disclosure / config-warning-merge contract this crate's doc promises cannot drift
/// per entry point. `leading` seeds the returned object's first keys (`analyze_summary`'s `path`/
/// `config` tree-mode echo; an empty map for envelope mode, which has neither) — every field below is
/// appended in the SAME order the pre-extraction inline code produced, so this refactor is a pure
/// behavior-preserving split, not a reshape.
fn shape_analyze_output(
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
    let (degraded_shown, degraded_truncated) =
        output::shape_list(&degraded, output::DEFAULT_DEGRADED_LIMIT);
    // Config-loader warnings first, then the facade-level `configWarnings` entries riding the tree
    // output (engine-side config diagnostics, e.g. unknown-rule-id overrides) — merged into the one
    // config-honesty channel so the moved diagnostics are not silently dropped at this layer (see
    // `crate::config_warnings::facade_config_warnings` for the absent-field degradation contract).
    config_warnings.extend(crate::config_warnings::facade_config_warnings(output_view));
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
    summary.insert("disclosure".to_string(), disclosure);
    if let Some(truncated) = degraded_truncated {
        summary.insert("degradedTruncated".to_string(), truncated);
    }
    // Rule-override confirmation ({disabled, severityRemapped} id lists) — forwarded whole, no cap
    // needed (bounded by the caller's own disabledRules/severityOverrides config size), same as
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
    // MCP<->CLI parity (STAGED): the `Full` lane additionally emits the raw output fields this summary
    // drops or compacts (`ir`/`nodes`/`scores`/... and the un-compacted health/recommendations/critical
    // behind the compact `architecture` above). Dead today — every caller passes `Verbosity::Summary`
    // (see `output::Verbosity`), so this never runs and the reply is byte-identical; a later flip makes
    // a host reply match the raw `zzop-facade` output.
    if filters.verbosity == Verbosity::Full {
        output::insert_full_output_fields(&mut summary, output_view);
    }
    serde_json::Value::Object(summary)
}

/// Builds the reply's compact `architecture` object from the facade output's `health`/
/// `recommendations`/`critical` fields — `None` (never `serde_json::Value::Null`) when `health`
/// itself is absent or JSON `null` (git signals did not run this tree), so the reply OMITS the key
/// entirely rather than growing a null `architecture` field on every git-less run. Deliberately
/// capped to ~10 lines of JSON: `pain` (the health scalar), the top-ROI `recommendations[0]`
/// (`{id, severity, topItem}`, null-safe when there are no recommendations or the top one has no
/// items), and up to 3 paths off the engine's own blast-radius-ranked `critical` list — named
/// `criticalTop`, NOT "hotspot": the engine's `hotspotScore` is a DIFFERENT metric (churn
/// `changeCount x loc`, `nodes[].hotspotScore`), and reusing that word here would invite joining two
/// non-matching rankings. The full arrays never
/// ride this summary (see analyze_repo's own description: they are the direct `zzop-facade`
/// embedding lane's job).
fn architecture_summary(output_view: &serde_json::Value) -> Option<serde_json::Value> {
    let pain = output_view.get("health")?.as_object()?.get("pain")?.clone();
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
    Some(serde_json::json!({
        "pain": pain,
        "topRecommendation": top_recommendation,
        "criticalTop": critical_top,
    }))
}
