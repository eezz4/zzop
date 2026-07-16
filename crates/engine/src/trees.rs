//! Cross-layer multi-tree API — `analyze_trees` and its `MultiAnalyzeOutput`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use zzop_core::{Finding, RuleConfig, SourceIo};

use crate::cross_layer_findings::compute_cross_layer_findings;
use crate::{analyze_tree, AnalyzeOutput, EngineConfig};

/// One `analyze_tree` call's output, per tree, plus the cross-layer join over every tree's IoFacts.
pub struct MultiAnalyzeOutput {
    /// `(root, config.source_id, output)` for each input tree, in the same order as `trees`.
    pub trees: Vec<(PathBuf, String, AnalyzeOutput)>,
    pub cross_layer: zzop_core::CrossLayerResult,
    /// The 23 `cross-layer/*` native rules run over `cross_layer` — see `compute_cross_layer_findings`'s
    /// doc for the gating/derivation/sort contract. Always populated: even a single-tree `analyze_trees`
    /// call runs these (most find nothing, since e.g. `shared-db-table`/`duplicate-route` need 2+
    /// distinct source trees to ever fire).
    pub cross_layer_findings: Vec<Finding>,
}

/// Cross-layer multi-tree API: runs `analyze_tree` once per `(root, config)` pair, then joins every
/// tree's `CommonIr.ir.io` via `zzop_core::link_cross_layer_io` (an exact `(kind, key)` join). Each tree
/// keeps its own `EngineConfig::source_id` as the join's per-tree tag, so a consume in tree A and a
/// provide in tree B join into a `cross_source: true` edge when their normalized keys match. A tree with
/// `ir.io = None` contributes an empty `IoFacts` to the join — never a panic, never a skipped tree.
/// One non-relative (package) import specifier's per-tree summary — see `AnalyzeOutput::package_imports`.
#[derive(Debug, Clone)]
pub struct PackageImportSummary {
    pub specifier: String,
    pub file_count: usize,
    pub example_file: String,
}

pub fn analyze_trees(trees: &[(PathBuf, EngineConfig)]) -> MultiAnalyzeOutput {
    let mut outputs = Vec::with_capacity(trees.len());
    let mut source_ios = Vec::with_capacity(trees.len());
    for (root, config) in trees {
        let output = analyze_tree(root, config);
        source_ios.push(SourceIo {
            source: config.source_id.clone(),
            io: output.ir.ir.io.clone().unwrap_or_default(),
        });
        outputs.push((root.clone(), config.source_id.clone(), output));
    }
    // Deployment-topology hosts (config-declared): union of every tree's `EngineConfig::hosts` into the
    // linker's `internal_hosts`, deduped with the FIRST-declaring tree's source id kept (`host_owners`) —
    // input order preserved, mirroring `zzop_core::CrossLayerResult::host_rekey_counts`'s own ordering
    // contract.
    let mut internal_hosts: Vec<String> = Vec::new();
    let mut host_owners: BTreeMap<String, String> = BTreeMap::new();
    for (_, config) in trees {
        for h in &config.hosts {
            if !internal_hosts.contains(h) {
                internal_hosts.push(h.clone());
                host_owners.insert(h.clone(), config.source_id.clone());
            }
        }
    }
    let link_opts = zzop_core::LinkOptions {
        // Default generic-path vocabulary (health/ping/metrics/...) is analysis-domain, not join
        // mechanism, so it lives in `zzop-metrics` rather than `zzop-core`.
        low_confidence_key_patterns: zzop_metrics::default_generic_interface_key_patterns(),
        internal_hosts,
    };
    let cross_layer = zzop_core::link_cross_layer_io(&source_ios, &link_opts);

    // Topology-host zero-effect tripwire: a declared host with `host_rekey_counts == 0` is either stale
    // (nothing calls it) or its consumers use relative paths instead of the absolute URL this feature
    // targets — either way, silent no-op would hide a config mistake. Pushed onto the DECLARING tree's own
    // `AnalyzeOutput::warnings` — the same per-tree engine self-report channel the tRPC mount-route
    // suppression note below already uses (chosen over a run-level `MultiAnalyzeOutput` field for
    // consistency with that precedent, since both are "this tree's own config had no observable effect"
    // disclosures).
    for (host, count) in &cross_layer.host_rekey_counts {
        if *count > 0 {
            continue;
        }
        let Some(owner) = host_owners.get(host) else {
            continue; // defensive: every entry in host_rekey_counts came from host_owners' own keys
        };
        if let Some((_, _, output)) = outputs.iter_mut().find(|(_, s, _)| s == owner) {
            output.warnings.push(format!(
                "topology host \"{host}\" had no effect: 0 absolute-URL consumes matched — stale host, the consumers use relative paths, a declared host:port needs the consumer to match host:port exactly, or the consumers use ws/wss (only http/https absolute-URL consumes re-key)"
            ));
        }
    }
    let package_imports: Vec<zzop_rules_cross_layer::PackageImportSite> = outputs
        .iter()
        .flat_map(|(_, source, output)| {
            output
                .package_imports
                .iter()
                .map(move |p| zzop_rules_cross_layer::PackageImportSite {
                    source: source.clone(),
                    specifier: p.specifier.clone(),
                    file_count: p.file_count,
                    example_file: p.example_file.clone(),
                })
        })
        .collect();
    // Per-tree, not run-global: a source tree "participates" in a `trpc`-kind edge when it appears on
    // EITHER side (`from.source` or `to.source` — a tree can be the router-defining provider, the caller,
    // or occasionally both for a same-tree edge). `trpc_edge_counts_by_source` counts each edge once per
    // distinct participating source (a same-tree edge, `from.source == to.source`, counts once for that
    // source, not twice). A run-global count here would let tree A's trpc edges suppress/misattribute a
    // literal `/trpc/`-segment route that tree B provides on its own, unrelated deployment — see
    // `zzop_rules_cross_layer::is_trpc_mount_route_key`'s doc.
    let mut trpc_edge_counts_by_source: BTreeMap<String, usize> = BTreeMap::new();
    for e in cross_layer.edges.iter().filter(|e| e.kind == "trpc") {
        let mut participants: Vec<&str> = vec![e.from.source.as_str()];
        if e.to.source != e.from.source {
            participants.push(e.to.source.as_str());
        }
        for source in participants {
            *trpc_edge_counts_by_source
                .entry(source.to_string())
                .or_insert(0) += 1;
        }
    }
    let trpc_participating_sources: BTreeSet<String> =
        trpc_edge_counts_by_source.keys().cloned().collect();
    let cross_layer_findings = compute_cross_layer_findings(
        &source_ios,
        &cross_layer,
        trees,
        &package_imports,
        &trpc_participating_sources,
    );

    // tRPC mount-route suppression disclosure — `unconsumed-endpoint`/`unconsumed-mutation-endpoint`
    // (inside `compute_cross_layer_findings` above) silently excluded any http provide identified as a
    // tRPC mount route whose OWN source tree is in `trpc_participating_sources`; per `output-philosophy.md`
    // §0/§1 (no silent suppression), that exclusion must surface somewhere — pushed onto the OWNING source
    // tree's own `AnalyzeOutput::warnings`, the same per-tree engine self-report channel every other
    // silent-failure disclosure in this crate uses. See
    // `zzop_rules_cross_layer::trpc_mount_route_suppression_notes`'s doc for the message shape and dogfood
    // motivation (round 9). Gated on the SAME rule-enable union the suppression itself runs under: with
    // both unconsumed rules disabled, no finding was suppressed, so a note would disclose a suppression
    // that never happened (a phantom disclosure).
    let disclosure_gate = RuleConfig {
        disabled_rules: trees
            .iter()
            .flat_map(|(_, c)| c.rule_config.disabled_rules.iter().cloned())
            .collect(),
        ..RuleConfig::default()
    };
    if zzop_core::is_enabled(&disclosure_gate, "cross-layer/unconsumed-endpoint")
        || zzop_core::is_enabled(&disclosure_gate, "cross-layer/unconsumed-mutation-endpoint")
    {
        for (source, note) in zzop_rules_cross_layer::trpc_mount_route_suppression_notes(
            &cross_layer.unconsumed_provides,
            &trpc_edge_counts_by_source,
        ) {
            if let Some((_, _, output)) = outputs.iter_mut().find(|(_, s, _)| *s == source) {
                output.warnings.push(note);
            }
        }
    }

    MultiAnalyzeOutput {
        trees: outputs,
        cross_layer,
        cross_layer_findings,
    }
}
