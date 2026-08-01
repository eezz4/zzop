//! `cross_repo`'s cross-layer join summary assembly (`cross_summary`) — see the crate doc: hosts
//! are thin protocol facades, all shaping logic lives here.

use crate::output::{self, FindingFilters};

/// Cross-repo analysis — zzop's headline. Config-first mode (`config_path`) runs the config's `trees`;
/// paths mode builds zero-config trees tagged by directory name (bundled packs + git defaults still
/// injected) and DISCLOSES any per-tree zzop.config.jsonc it deliberately did not load.
pub fn cross_summary(
    paths: &[String],
    config_path: Option<&str>,
    filters: &FindingFilters,
) -> Result<String, String> {
    // Source-mode exclusivity + config-method gating are enforced in `zzop_config::trees` (shared verbatim
    // with `manifest_json`), not (only) in the hosts — the same centralization `endpoint_summary` gets
    // from `resolve_trees_request`.
    // The operation name rides into the shared loader's error text, so it is the SURFACE-NEUTRAL name of
    // this analysis, never one host's tool spelling — see `zzop_config::trees`'s WIRE NEUTRALITY note.
    let loaded =
        zzop_config::trees::load_trees_request("the cross-layer join", paths, config_path)?;
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;

    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);
    // Sibling-directory scope disclosure (both modes — the engine echoes each tree's absolute root):
    // when every analyzed root sits under one common parent, that parent's unanalyzed immediate
    // subdirectories are enumerated as a configWarnings entry — the join never silently narrows to
    // "only the trees you happened to pass" (see `crate::siblings`).
    let mut config_warnings = loaded.warnings;
    let roots: Vec<std::path::PathBuf> = trees
        .iter()
        .filter_map(|t| t["root"].as_str().map(std::path::PathBuf::from))
        .collect();
    if let Some(w) = crate::siblings::sibling_scope_warning(&roots) {
        config_warnings.push(w);
    }
    // Config-loader warnings first, then each tree output's facade-level `configWarnings` entries
    // (tree order) — merged into the one config-honesty channel, see `crate::config_warnings`.
    let mut config_warnings: Vec<serde_json::Value> = config_warnings
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    for t in trees {
        config_warnings.extend(crate::config_warnings::facade_config_warnings(&t["output"]));
    }
    let sources: Vec<serde_json::Value> = trees
        .iter()
        .map(|t| {
            let mut source = serde_json::json!({
                "sourceId": t["sourceId"],
                "path": t["root"],
                "fileCount": t["output"]["fileCount"],
                "findingCount": t["output"]["findings"].as_array().map(Vec::len).unwrap_or(0),
                // Per-tree pack-load confirmation — bounded like analyze_repo's (see there).
                "packsLoaded": t["output"]["packsLoaded"],
                "warnings": t["output"]["warnings"],
                // Per-tree coverage census incl. `joinContributionZero` — see analyze_summary.
                "coverage": t["output"]["coverage"],
            });
            // Per-tree rule-override confirmation — omitted (not null) when absent, same `.get()`
            // guard as analyze_summary (see there for why this diverges from packsLoaded's bare
            // index).
            if let Some(rule_overrides_applied) = t["output"].get("ruleOverridesApplied") {
                source["ruleOverridesApplied"] = rule_overrides_applied.clone();
            }
            source
        })
        .collect();
    let cl = &v["crossLayer"];
    let bucket_len = |key: &str| cl[key].as_array().map(Vec::len).unwrap_or(0);
    let edges = cl["edges"].as_array().cloned().unwrap_or_default();
    let (edges_shown, edges_truncated) = output::shape_list(
        &edges,
        output::DEFAULT_EDGES_LIMIT,
        // No caller argument moves this cap (`limit` filters findings only) — the hint names the field
        // that carries the full count and the QUERY that can answer per-edge, never a knob that would
        // silently do nothing here. Spelling-free: "check_endpoint" named the MCP tool to CLI users too.
        "this list has a fixed cap and no argument raises it — `buckets.edges` carries the full, \
         uncapped count; drill into a specific route with the endpoint query",
    );
    let cl_findings = v["crossLayerFindings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // WHICH keys sit in each non-edge bucket, not just how many — UNCAPPED since 2026-07-29, so unlike
    // `edges` below there is no truncation field to pair with it (nothing is dropped, so nothing needs
    // disclosing; see `output::bucket_keys`' own doc for why the cap and its disclosure both went).
    // `distinct_bucket_key_first_sites` locates the FIRST site (`file:line`) backing each listed key, so
    // e.g. an `unresolvedConsumes` key is no longer a bare string with no call site to go look at.
    let (distinct_bucket_keys, distinct_bucket_key_first_sites) = output::distinct_bucket_keys(cl);

    let mut summary = serde_json::json!({
        "config": loaded.config_path.as_deref().map(|p| p.display().to_string()),
        "sources": sources,
        "buckets": {
            "edges": edges.len(),
            "unconsumedProvides": bucket_len("unconsumedProvides"),
            "unprovidedConsumes": bucket_len("unprovidedConsumes"),
            "unresolvedConsumes": bucket_len("unresolvedConsumes"),
            "externalConsumes": bucket_len("externalConsumes"),
            "ambiguousConsumes": bucket_len("ambiguousConsumes"),
        },
        // The arithmetic between the two bucket views, ANSWERED ON THE WIRE rather than in a name alone.
        // `buckets.X` counts raw rows and `distinctBucketKeys.X` dedupes them, so a reader checking
        // `buckets.X == len(distinctBucketKeys.X)` legitimately gets a mismatch (measured on this repo's
        // own corpus: 23 `unprovidedConsumes` rows over 14 distinct keys). A run-invariant sentence, not
        // a computed one — the relationship is a contract, and computing per-bucket deltas here would
        // publish the same numbers a reader can already subtract. Same repair the graph census made when
        // its `--top` cap described rows the picture did not draw ("60 rows are 4 relations").
        "bucketMeaning": "buckets counts ROWS for all six buckets, but a ROW is not the same thing in \
            each: the four consume-side buckets count recorded CALL SITES, unconsumedProvides counts \
            route/handler DECLARATION sites, and edges counts matched consume->provide PAIRS. \
            distinctBucketKeys covers the five non-edge buckets and lists the DISTINCT keys those rows \
            collapse into, so buckets.X is always >= the length of distinctBucketKeys.X and equality \
            only means no key repeated. buckets.edges has no key list beside it — the edges array \
            itself is the per-row view, capped, with edgesTruncated when the cap bit. \
            distinctBucketKeyFirstSites carries ONE site per distinct key: the first recorded one, \
            never every site behind it.",
        "distinctBucketKeys": distinct_bucket_keys,
        "distinctBucketKeyFirstSites": distinct_bucket_key_first_sites,
        "edges": edges_shown,
        "crossLayerFindings": output::shape_findings(&cl_findings, filters),
        "configWarnings": config_warnings,
        // Run-global blindness-class registry, FOLDED to counts + a pointer to its full text (see
        // `output::disclosure`) — the meta-honesty channel with its magnitude intact and its
        // run-invariant prose one lookup away, same as `analyze_summary`'s.
        "disclosure": output::fold_disclosure(&v["disclosure"]),
    });
    if let Some(truncated) = edges_truncated {
        summary["edgesTruncated"] = truncated;
    }
    // Run-level warnings (distinct from sources[].warnings) — e.g. the parallel-implementation
    // tripwire ("0 cross-source edges but N duplicate/ambiguous findings"). ALWAYS PRESENT, empty
    // array included: this key used to be written only when non-empty, which made "this run
    // self-reported nothing" and "this build has no run-level warning channel" the same bytes —
    // the silence this repo's always-present-key rule exists to abolish, and the same defect the
    // sibling `sources[].warnings` never had. `.get()` stays defensive about the SOURCE field
    // (it is newer than some outputs); the absence of a source is what yields `[]`, not a
    // judgment that there was nothing to say.
    summary["warnings"] = serde_json::Value::Array(
        v.get("warnings")
            .and_then(|w| w.as_array())
            .cloned()
            .unwrap_or_default(),
    );
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}
