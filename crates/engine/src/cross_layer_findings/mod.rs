//! The 23 `cross-layer/*` native rules run over one `analyze_trees` join — see
//! `compute_cross_layer_findings`'s doc for the gating/derivation/sort contract.

mod blindness_caveat;
mod merge_config;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use zzop_core::{ConsumeBodyShape, Finding, ProvideBodyShape, SourceIo};

use crate::EngineConfig;

/// Runs the 23 `cross-layer/*` native rules (`zzop_rules_cross_layer::cross_layer`) over `cross_layer` and
/// returns their merged, sorted findings.
///
/// ## disabledRules gating and severity overrides
/// Both union across trees — see `merge_config::union_configs`'s doc for the exclude-only gating
/// rationale and the first-declarer conflict rule.
///
/// ## The provide-key universe
/// `method_mismatch`/`version_skew`/`path_near_miss` need every `http` provide across every tree, not
/// just what `CrossLayerResult` exposes — derived here (`http_provides`) from the same `source_ios` the
/// join itself was built from.
///
/// ## Sort and severity overrides
/// `zzop_core::merge_findings`, the same (severity, file, line, ruleId) order `AnalyzeOutput::findings`
/// uses. The config passed to it carries the severity-overrides union so the override runs INSIDE
/// the merge, before its sort — see `merge_config::union_configs`'s doc for why applying it after
/// would break the order.
///
/// ## Extraction-blindness caveat
/// `blindness_caveat::build`/`append` (split out to keep this file under the line-count ratchet) append
/// a shared caveat sentence to `unconsumed-endpoint`/`unconsumed-mutation-endpoint` findings when at
/// least one OTHER source in this join contributed zero joinable io — see that module's doc.
pub(crate) fn compute_cross_layer_findings(
    source_ios: &[SourceIo],
    cross_layer: &zzop_core::CrossLayerResult,
    trees: &[(PathBuf, EngineConfig)],
    package_imports: &[zzop_rules_cross_layer::PackageImportSite],
    trpc_participating_sources: &BTreeSet<String>,
) -> Vec<Finding> {
    let (gate, merge_config) = merge_config::union_configs(trees);

    let extraction_blindness_caveat = blindness_caveat::build(source_ios);

    let http_provides: Vec<zzop_rules_cross_layer::HttpProvideSite> = source_ios
        .iter()
        .flat_map(|s| {
            s.io.provides
                .iter()
                .filter(|p| p.kind == "http")
                .map(move |p| zzop_rules_cross_layer::HttpProvideSite {
                    source: s.source.clone(),
                    key: p.key.clone(),
                    file: p.file.clone(),
                    line: p.line,
                })
        })
        .collect();

    let http_consume_totals: Vec<(String, usize)> = source_ios
        .iter()
        .filter_map(|s| {
            let n = s.io.consumes.iter().filter(|c| c.kind == "http").count();
            (n > 0).then(|| (s.source.clone(), n))
        })
        .collect();

    // `cross-layer/body-field-drift`'s lookup maps — keyed exactly like `HttpProvideSite`/edge anchors
    // are derived, `(source, file, line)`, so a rule can join an edge's `from`/`to` straight into these
    // without re-deriving anything. Only entries whose `body` is `Some` are worth keeping (an edge whose
    // consume/provide never witnessed a body shape can never drift-compare). On a duplicate key, the
    // FIRST occurrence wins for consumes (same call site => same witnessed body, so any duplicate is
    // spurious re-collection, never a real second body); a duplicate provide key is likewise first-wins
    // (same handler declaration site).
    let mut consume_bodies: BTreeMap<(String, String, u32), ConsumeBodyShape> = BTreeMap::new();
    let mut provide_bodies: BTreeMap<(String, String, u32), ProvideBodyShape> = BTreeMap::new();
    for s in source_ios {
        for c in s.io.consumes.iter().filter(|c| c.kind == "http") {
            if let Some(body) = &c.body {
                consume_bodies
                    .entry((s.source.clone(), c.file.clone(), c.line))
                    .or_insert_with(|| body.clone());
            }
        }
        for p in s.io.provides.iter().filter(|p| p.kind == "http") {
            if let Some(body) = &p.body {
                provide_bodies
                    .entry((s.source.clone(), p.file.clone(), p.line))
                    .or_insert_with(|| body.clone());
            }
        }
    }

    let mut sources: Vec<Vec<Finding>> = Vec::with_capacity(23);

    // `route_near_miss_results` is called ONCE here (ahead of its own position in `sources` order below) so
    // both `unconsumed-endpoint` and `unconsumed-mutation-endpoint` can annotate a provide that is also a
    // near-miss target — see `zzop_rules_cross_layer::route_near_miss`'s module doc. Disabled ->
    // `near_miss_targets` stays empty (there is no near-miss finding to point at, so no annotation), and the
    // findings themselves are still only pushed into `sources` at their original position, under the same
    // `is_enabled` gate as before.
    let route_near_miss_result = if zzop_core::is_enabled(&gate, "cross-layer/route-near-miss") {
        Some(
            zzop_rules_cross_layer::cross_layer::route_near_miss::route_near_miss_results(
                &cross_layer.unprovided_consumes,
                &http_provides,
            ),
        )
    } else {
        None
    };
    let near_miss_targets = route_near_miss_result
        .as_ref()
        .map(|r| r.targets.clone())
        .unwrap_or_default();

    if zzop_core::is_enabled(&gate, "cross-layer/unconsumed-endpoint") {
        let mut findings = zzop_rules_cross_layer::unconsumed_endpoint_findings(
            &cross_layer.unconsumed_provides,
            &cross_layer.unresolved_consumes,
            &near_miss_targets,
            trpc_participating_sources,
        );
        blindness_caveat::append(&mut findings, &extraction_blindness_caveat);
        sources.push(findings);
    }
    if zzop_core::is_enabled(&gate, "cross-layer/method-mismatch") {
        sources.push(zzop_rules_cross_layer::method_mismatch_findings(
            &cross_layer.unprovided_consumes,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/version-skew") {
        sources.push(zzop_rules_cross_layer::version_skew_findings(
            &cross_layer.unprovided_consumes,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/path-near-miss") {
        sources.push(zzop_rules_cross_layer::path_near_miss_findings(
            &cross_layer.unprovided_consumes,
            &http_provides,
        ));
    }
    if let Some(result) = route_near_miss_result {
        // `cross-layer/prefix-drift` aggregates route-near-miss's `prefix_records`: when 3+ consumes share
        // one missing/extra base prefix (`/api`, ...) against the same target tree, one aggregate finding
        // replaces those per-route near-misses — subsumed via `retain_non_subsumed`. This is a replacement,
        // not silent suppression: the aggregate enumerates every folded route (`output-philosophy.md` §0/§1).
        // Structurally derived from route-near-miss's records, so it can only run inside this branch (route-
        // near-miss enabled); disabling prefix-drift alone leaves the per-route near-misses intact.
        if zzop_core::is_enabled(&gate, "cross-layer/prefix-drift") {
            let prefix_drift =
                zzop_rules_cross_layer::prefix_drift_findings(&result.prefix_records);
            sources.push(zzop_rules_cross_layer::retain_non_subsumed(
                result.findings,
                &prefix_drift.subsumed,
            ));
            if !prefix_drift.findings.is_empty() {
                sources.push(prefix_drift.findings);
            }
        } else {
            sources.push(result.findings);
        }
    }
    if zzop_core::is_enabled(&gate, "cross-layer/shared-db-table") {
        sources.push(zzop_rules_cross_layer::shared_db_table_findings(
            cross_layer,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/duplicate-route") {
        sources.push(zzop_rules_cross_layer::cross_layer_duplicate_route_findings(cross_layer));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-shadow-internal") {
        sources.push(zzop_rules_cross_layer::external_shadow_internal_findings(
            &cross_layer.external_consumes,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-secret-in-url") {
        sources.push(zzop_rules_cross_layer::external_secret_in_url_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-duplicated-integration") {
        sources.push(
            zzop_rules_cross_layer::external_duplicated_integration_findings(
                &cross_layer.external_consumes,
            ),
        );
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-host-fanout") {
        sources.push(zzop_rules_cross_layer::external_host_fanout_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-base-url-drift") {
        sources.push(zzop_rules_cross_layer::external_base_url_drift_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-version-inconsistent") {
        sources.push(
            zzop_rules_cross_layer::external_version_inconsistent_findings(
                &cross_layer.external_consumes,
            ),
        );
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-ip-literal") {
        sources.push(zzop_rules_cross_layer::external_ip_literal_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/ambiguous-consume") {
        sources.push(zzop_rules_cross_layer::ambiguous_consume_findings(
            &cross_layer.ambiguous_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unconsumed-mutation-endpoint") {
        // Same blindness predicate `cross-layer/unresolved-consume-ratio` self-reports with (below) —
        // computed via the shared helper so the two rules can never silently drift apart on what counts as
        // a BLIND source (mono-hub field review: a confident "unconsumed" verdict requires a resolved
        // consume side).
        let blind_sources = zzop_rules_cross_layer::majority_unresolved_http_sources(
            &cross_layer.unresolved_consumes,
            &http_consume_totals,
        );
        let mut findings = zzop_rules_cross_layer::unconsumed_mutation_endpoint_findings(
            &cross_layer.unconsumed_provides,
            &cross_layer.unresolved_consumes,
            &blind_sources,
            &near_miss_targets,
            trpc_participating_sources,
        );
        blindness_caveat::append(&mut findings, &extraction_blindness_caveat);
        sources.push(findings);
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unprovided-mutation-call") {
        // Provide-side blindness gate — the symmetric mirror of `unconsumed-mutation-endpoint`'s
        // consume-blind gate above: a confident "no provider anywhere" verdict cannot be trusted when a
        // framework-bearing tree in this run extracted almost no routes (the S2 framework-silence
        // tripwire condition, `framework_silence::provide_blind_sources`) — the provider may live in that
        // blind tree, unseen. `http_provide_counts` seeds every source in this run at 0 (not just sources
        // that appear in `http_provides`) so a framework-importer with zero extracted routes — the most
        // blind case — is never silently dropped from the count map.
        let mut http_provide_counts_map: BTreeMap<String, usize> = source_ios
            .iter()
            .map(|s| (s.source.clone(), 0usize))
            .collect();
        for p in &http_provides {
            *http_provide_counts_map.entry(p.source.clone()).or_insert(0) += 1;
        }
        let http_provide_counts: Vec<(String, usize)> =
            http_provide_counts_map.into_iter().collect();
        let provide_blind_sources =
            crate::framework_silence::provide_blind_sources(package_imports, &http_provide_counts);
        sources.push(zzop_rules_cross_layer::unprovided_mutation_call_findings(
            &cross_layer.unprovided_consumes,
            &provide_blind_sources,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/route-shadowing") {
        sources.push(zzop_rules_cross_layer::cross_tree_route_shadowing_findings(
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unresolved-consume-ratio") {
        sources.push(zzop_rules_cross_layer::unresolved_consume_ratio_findings(
            &cross_layer.unresolved_consumes,
            &http_consume_totals,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/sdk-import-no-visible-consume") {
        sources.push(
            zzop_rules_cross_layer::sdk_import_no_visible_consume_findings(
                package_imports,
                &http_consume_totals,
            ),
        );
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unconsumed-procedure") {
        sources.push(zzop_rules_cross_layer::unconsumed_procedure_findings(
            &cross_layer.unconsumed_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/body-field-drift") {
        sources.push(zzop_rules_cross_layer::body_field_drift_findings(
            &cross_layer.edges,
            &consume_bodies,
            &provide_bodies,
        ));
    }

    zzop_core::merge_findings(sources, &merge_config)
}
