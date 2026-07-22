//! The 25 `cross-layer/*` native rules run over one `analyze_trees` join — see `compute_cross_layer_findings` for the gating/derivation/sort contract.

mod blindness_caveat;
mod merge_config;
mod partition;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use zzop_core::{ConsumeBodyShape, Finding, ProvideBodyShape, SourceIo};

use crate::EngineConfig;

/// Runs the 25 `cross-layer/*` native rules (`zzop_rules_cross_layer::cross_layer`) over `cross_layer`, returning their merged, sorted findings.
///
/// ## disabledRules gating and severity overrides
/// Both union across trees — see `merge_config::union_configs` for the exclude-only gating rationale and
/// the first-declarer conflict rule.
///
/// ## The provide-key universe
/// `method_mismatch`/`version_skew`/`path_near_miss` need every `http` provide across every tree, not just
/// what `CrossLayerResult` exposes — derived here (`http_provides`) from the same `source_ios` as the join.
///
/// ## Sort and severity overrides
/// `zzop_core::merge_findings`, the same (severity, file, line, ruleId) order `AnalyzeOutput::findings`
/// uses. Its config carries the severity-overrides union so the override runs INSIDE the merge, before its
/// sort (see `merge_config::union_configs` for why applying it after would break the order).
///
/// ## Extraction-blindness caveat
/// `blindness_caveat::build`/`append` (split out to keep this file under the line-count ratchet) append a
/// shared caveat to `unconsumed-endpoint`/`unconsumed-mutation-endpoint` findings when at least one OTHER
/// source in this join contributed zero joinable io — see that module's doc.
pub(crate) fn compute_cross_layer_findings(
    source_ios: &[SourceIo],
    cross_layer: &zzop_core::CrossLayerResult,
    trees: &[(PathBuf, EngineConfig)],
    package_imports: &[zzop_rules_cross_layer::PackageImportSite],
    trpc_participating_sources: &BTreeSet<String>,
    attribute_stores: &BTreeMap<String, &zzop_core::AttributeStore>,
) -> Vec<Finding> {
    let (gate, merge_config) = merge_config::union_configs(trees);

    let extraction_blindness_caveat = blindness_caveat::build(source_ios);

    // Verb-unknown routes (`UNKNOWN_VERB` sentinel: `pages/api` serve-all / pathname-dispatch / Go
    // `HandleFunc` pinning no method) lift OUT of the exact-key join to a served-set — `http_provides` drops
    // them (never dead), `unprovided_filtered` drops consumes they serve (no FP); surface via `unknown-verb-route`.
    let verb_unknown_sites = partition::verb_unknown_sites(source_ios);
    let verb_unknown_paths = partition::served_path_set(&verb_unknown_sites);
    let unprovided_filtered =
        partition::without_verb_unknown(&cross_layer.unprovided_consumes, &verb_unknown_paths);
    let unconsumed_provides =
        partition::without_verb_unknown_provides(&cross_layer.unconsumed_provides);
    let http_provides = partition::http_provide_sites(source_ios);

    let http_consume_totals: Vec<(String, usize)> = source_ios
        .iter()
        .filter_map(|s| {
            let n = s.io.consumes.iter().filter(|c| c.kind == "http").count();
            (n > 0).then(|| (s.source.clone(), n))
        })
        .collect();

    // `cross-layer/body-field-drift`'s lookup maps, keyed `(source, file, line)` like edge `from`/`to` anchors
    // so a rule joins straight in; only `Some`-body entries kept, first occurrence wins per key.
    let mut consume_bodies: BTreeMap<(String, String, u32), ConsumeBodyShape> = BTreeMap::new();
    let mut provide_bodies: BTreeMap<(String, String, u32), ProvideBodyShape> = BTreeMap::new();
    // Retry-configured write consume sites (write-only tag), same `from`-anchor key — join set for `cross-layer/retrying-write-no-idempotency`.
    let mut retry_sites: BTreeSet<zzop_rules_cross_layer::RetrySite> = BTreeSet::new();
    for s in source_ios {
        for c in s.io.consumes.iter().filter(|c| c.kind == "http") {
            if let Some(body) = &c.body {
                consume_bodies
                    .entry((s.source.clone(), c.file.clone(), c.line))
                    .or_insert_with(|| body.clone());
            }
            if c.retry_configured == Some(true) {
                retry_sites.insert((s.source.clone(), c.file.clone(), c.line));
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

    let mut sources: Vec<Vec<Finding>> = Vec::with_capacity(25);

    if zzop_core::is_enabled(&gate, "cross-layer/unknown-verb-route") {
        sources.push(zzop_rules_cross_layer::unknown_verb_route_findings(
            &partition::disclosure_sites(&verb_unknown_sites, trpc_participating_sources),
        ));
    }

    // `route_near_miss_results` is called ONCE here (ahead of its `sources` position below) so both
    // `unconsumed-endpoint`/`unconsumed-mutation-endpoint` can annotate a provide that is also a near-miss
    // target — see `route_near_miss`'s doc. Disabled -> `near_miss_targets` empty (no annotation); the
    // findings themselves still push at their original position under the same `is_enabled` gate.
    let route_near_miss_result = if zzop_core::is_enabled(&gate, "cross-layer/route-near-miss") {
        Some(
            zzop_rules_cross_layer::cross_layer::route_near_miss::route_near_miss_results(
                &unprovided_filtered,
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
            &unconsumed_provides,
            &cross_layer.unresolved_consumes,
            &near_miss_targets,
            trpc_participating_sources,
        );
        blindness_caveat::append(&mut findings, &extraction_blindness_caveat);
        sources.push(findings);
    }
    if zzop_core::is_enabled(&gate, "cross-layer/method-mismatch") {
        sources.push(zzop_rules_cross_layer::method_mismatch_findings(
            &unprovided_filtered,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/version-skew") {
        sources.push(zzop_rules_cross_layer::version_skew_findings(
            &unprovided_filtered,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/path-near-miss") {
        sources.push(zzop_rules_cross_layer::path_near_miss_findings(
            &unprovided_filtered,
            &http_provides,
        ));
    }
    if let Some(result) = route_near_miss_result {
        // `cross-layer/prefix-drift` aggregates route-near-miss's `prefix_records`: 3+ consumes sharing one
        // missing/extra base prefix (`/api`, ...) against the same target collapse into one finding that
        // subsumes (`retain_non_subsumed`) the per-route near-misses — a replacement, not suppression (the
        // aggregate enumerates every folded route, `output-philosophy.md` §0/§1). Derived from route-near-
        // miss's records, so it only runs inside this branch; disabling it alone leaves the near-misses.
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
        // Same blindness predicate `cross-layer/unresolved-consume-ratio` self-reports with, via the shared
        // helper so the two rules never drift on what counts BLIND (a confident "unconsumed" verdict needs
        // a resolved consume side).
        let blind_sources = zzop_rules_cross_layer::majority_unresolved_http_sources(
            &cross_layer.unresolved_consumes,
            &http_consume_totals,
        );
        let mut findings = zzop_rules_cross_layer::unconsumed_mutation_endpoint_findings(
            &unconsumed_provides,
            &cross_layer.unresolved_consumes,
            &blind_sources,
            &near_miss_targets,
            trpc_participating_sources,
        );
        blindness_caveat::append(&mut findings, &extraction_blindness_caveat);
        sources.push(findings);
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unprovided-mutation-call") {
        // Provide-side blindness gate — mirror of `unconsumed-mutation-endpoint`'s consume-blind gate: a
        // "no provider anywhere" verdict is untrusted when a framework-bearing tree extracted almost no
        // routes (S2 tripwire, `framework_silence::provide_blind_sources`) — the provider may live there
        // unseen. `http_provide_counts` seeds every source at 0 (not just those in `http_provides`) so a
        // framework-importer with zero routes — the most blind case — is never dropped from the count map.
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
            &unprovided_filtered,
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
            &unconsumed_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/body-field-drift") {
        sources.push(zzop_rules_cross_layer::body_field_drift_findings(
            &cross_layer.edges,
            &consume_bodies,
            &provide_bodies,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/retrying-write-no-idempotency") {
        sources.push(
            zzop_rules_cross_layer::retrying_write_no_idempotency_findings(
                &cross_layer.edges,
                &retry_sites,
                attribute_stores,
            ),
        );
    }

    zzop_core::merge_findings(sources, &merge_config)
}
