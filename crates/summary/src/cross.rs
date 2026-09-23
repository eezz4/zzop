//! `cross_repo`'s cross-layer join summary assembly (`cross_summary`) — see the crate doc: hosts
//! are thin protocol facades, all shaping logic lives here.

mod native_analyses;

use crate::output::{self, FindingFilters, RunKnobs};

/// Cross-repo analysis — zzop's headline. Config-first mode (`config_path`) runs the config's `trees`;
/// paths mode tags each root by directory name and LOADS that root's own zzop.config.jsonc (bundled
/// packs + git defaults still injected), disclosing the honored files in `configWarnings` while the
/// reply's `config` stays null — no single config governs a multi-root run. (This said the reverse
/// until 2026-08-01: it described the pre-2026-07-27 behaviour, where paths mode deliberately did NOT
/// load them. The honesty need did not change, only the answer.)
pub fn cross_summary(
    paths: &[String],
    config_path: Option<&str>,
    filters: &FindingFilters,
) -> Result<String, String> {
    cross_summary_with(paths, config_path, filters, RunKnobs::default())
}

/// [`cross_summary`] plus the per-invocation [`RunKnobs`] — same CLI-only rationale as
/// `analyze_summary_with`. The join's timing report is PER TREE (on each `sources[]` entry), never one
/// total: see the per-tree insert below for why summing them would describe no real run.
pub fn cross_summary_with(
    paths: &[String],
    config_path: Option<&str>,
    filters: &FindingFilters,
    knobs: RunKnobs,
) -> Result<String, String> {
    // Source-mode exclusivity + config-method gating are enforced in `zzop_config::trees` (shared verbatim
    // with `manifest_json`), not (only) in the hosts — the same centralization `endpoint_summary` gets
    // from `resolve_trees_request`.
    // The operation name rides into the shared loader's error text, so it is the SURFACE-NEUTRAL name of
    // this analysis, never one host's tool spelling — see `zzop_config::trees`'s WIRE NEUTRALITY note.
    let mut loaded =
        zzop_config::trees::load_trees_request("the cross-layer join", paths, config_path)?;
    crate::analyze::apply_run_knobs(&mut loaded.request, knobs);
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
    // (tree order) — merged into the one config-honesty channel, see
    // `crate::warnings::facade_config_warnings` for the absent-field degradation contract.
    let mut config_warnings: Vec<serde_json::Value> = config_warnings
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    for t in trees {
        config_warnings.extend(crate::warnings::facade_config_warnings(&t["output"]));
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
            // Per-tree rule timing, PER TREE and never summed: each tree is its own engine run with its
            // own cache state, so one tree can be cold (fully timed) while its neighbour is warm (timed
            // nowhere). A single joined total would average those two into a number describing neither.
            if let Some(rule_timings) = output::shape_rule_timings(&t["output"]) {
                source["ruleTimings"] = rule_timings;
            }
            // 🔴 The single-tree lane has published cache provenance since the day that silence was
            // named, and this one never did: `JSON.stringify(crossReply).includes("hitFiles")` was
            // `false` (review ledger V146). A reader of a join could not tell a recomputed finding
            // list from a replayed one — the exact question `cache` exists to answer — and the
            // comment three lines up already knew each tree has its own cache state, which is why
            // the timings are kept per tree. The provenance was the one thing it knew and did not say.
            //
            // PER TREE for the same reason, never summed: `12 hits of 12` in one tree and `0 of 4000`
            // in its neighbour is two facts, and their sum is a fact about neither.
            if let Some(cache) = output::shape_cache_numbers(&t["output"]) {
                source["cache"] = cache;
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

    // ONE legend for every tree's `packsLoaded`, at the reply root rather than repeated in each of the
    // N `sources[]` rows that carry the arrays. Merged (not "take the first"): the legend grows a
    // `didNotRun` entry only on a tree that actually gated a pack, so a cross run where ONE tree
    // switched a pack off must still explain the key — while a run where none did says nothing about
    // gating at all. Merging is order-independent because every value here is the same build constant,
    // so `sources[i]` and `sources[j]` can never contribute conflicting text for one key.
    let packs_loaded_meaning: serde_json::Map<String, serde_json::Value> = trees
        .iter()
        .filter_map(|t| t["output"].get("packsLoadedMeaning")?.as_object())
        .flat_map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())))
        .collect();

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
        // No manifest set here, and that is a judgment rather than an omission: a cross-layer finding
        // straddles TWO trees, so "which tree's package.json declared this file build surface" has no
        // single answer, and picking one tree's set would demote the other tree's paths on the strength of
        // a name collision. The path-shape half of the axis (`.github/`, `*.example`) is
        // tree-independent and still applies — it reads the finding's own path and nothing else.
        "crossLayerFindings": output::shape_findings(&cl_findings, filters, &Default::default()),
        "configWarnings": config_warnings,
        // Run-global blindness-class registry, FOLDED to counts + a pointer to its full text (see
        // `output::disclosure`) — the meta-honesty channel with its magnitude intact and its
        // run-invariant prose one lookup away, same as `analyze_summary`'s.
        "disclosure": output::fold_disclosure(&v["disclosure"]),
    });
    if let Some(truncated) = edges_truncated {
        summary["edgesTruncated"] = truncated;
    }
    // The NATIVE roster for THIS join, and its legend. The per-tree lane got one in 9272ee0 because
    // a cross-layer analysis with nowhere to report was byte-identical to one that ran clean; the
    // same argument holds here and this reply had no roster at all — not at the root, not on any
    // `sources[]` row. Its `crossLayerFindings.byRule` therefore listed the analyses that FIRED with
    // no population beside them, in the one reply where those analyses are the whole subject. The
    // keys are inserted by NAME (not as `json!` literals) and only when a tree published a roster, so
    // an older engine degrades to omission rather than a JSON `null` — the same `.get()`-gated lane
    // `packsLoadedMeaning` above takes. See `native_analyses` for the derivation, for why the legend
    // forks by lane while the key name does not, and for the floor.
    if let Some((roster, meaning)) = native_analyses::join_roster(trees, &cl_findings) {
        summary["nativeAnalyses"] = roster;
        summary["nativeAnalysesMeaning"] = meaning;
    }
    // Absent, never an empty object, when no tree loaded a pack — the same "omit rather than publish an
    // empty legend" rule the single-tree lane follows.
    // ONE legend for every tree's `cache`, at the root — the shape this lane already uses for
    // `bucketMeaning`/`nativeAnalysesMeaning`/`packsLoadedMeaning`. Omitted, never null, when no tree
    // used a cache at all: a run with caching off has no provenance question to answer, so it gains
    // no key, which is the same absent-vs-null contract the single-tree lane keeps.
    if summary["sources"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|r| r.get("cache").is_some()))
    {
        summary["cacheMeaning"] = serde_json::Value::String(output::CACHE_MEANING.to_string());
    }
    if !packs_loaded_meaning.is_empty() {
        // FOLDED, exactly as the per-tree lane folds it (2026-09-01, see `output::legends`). This
        // legend is build constants in BOTH lanes — the merge above only ever decided whether the
        // `didNotRun` entry appeared — so "identical every run" is as true here as there. Folding one
        // lane while the other shipped the full text would put ONE key in TWO shapes and leave a
        // consumer needing to know which reply it is holding, which is the drift this crate exists to
        // prevent. The merge stays because it is still what answers "did any pack load at all", and
        // this key is still OMITTED when none did.
        summary["packsLoadedMeaning"] = output::legends::folded_object("packsLoadedMeaning");
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
