//! Cross-layer multi-tree API — `analyze_trees` and its `MultiAnalyzeOutput`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use zzop_core::{Finding, RuleConfig, SourceIo};

use crate::cross_layer_findings::compute_cross_layer_findings;
use crate::{AnalyzeOutput, EngineConfig};

mod cross_tree_imports;
mod join_io_filter;
mod parallel_impl;

pub use parallel_impl::MIN_PARALLEL_IMPL_SIGNALS;

use join_io_filter::filter_join_io;

pub struct MultiAnalyzeOutput {
    /// `(root, config.source_id, output)` for each input tree, in the same order as `trees`.
    pub trees: Vec<(PathBuf, String, AnalyzeOutput)>,
    pub cross_layer: zzop_core::CrossLayerResult,
    /// The `cross-layer/*` native rules run over `cross_layer` (how many there are is owned by
    /// `zzop_rules_cross_layer::register_native_analyses`) — see `compute_cross_layer_findings`'s
    /// doc for the gating/derivation/sort contract. Always populated: even a single-tree `analyze_trees`
    /// call runs these (most find nothing, since e.g. `db-table-name-in-multiple-sources`/`duplicate-route` need 2+
    /// distinct source trees to ever fire).
    pub cross_layer_findings: Vec<Finding>,
    /// Run-level self-reports that belong to the JOIN itself, not any one tree — currently only the
    /// parallel-implementation tripwire (see `parallel_impl::maybe_warn`'s doc). ALWAYS populated (no
    /// skip-if-empty upstream), same "empty is the honest signal" convention every other warnings
    /// channel in this crate uses. Distinct from any tree's own `AnalyzeOutput::warnings` (which blame
    /// one config-declaring tree) and from `cross_layer_findings` (which are per-finding native-rule
    /// output, not free-text self-reports).
    pub warnings: Vec<String>,
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

impl PackageImportSummary {
    /// Collapses a per-tree import census (`specifier -> importing files`) into one summary per
    /// specifier. Lives here rather than at either call site because BOTH assembly paths — the source
    /// pass (`analyze::assemble`) and the envelope pass (`envelope::ingest`) — end with this identical
    /// fold, and two copies of a determinism argument is one copy too many.
    ///
    /// Determinism: `BTreeSet` iteration is sorted, so `example_file` is the lexicographically first
    /// importing file, not an arbitrary one.
    pub(crate) fn census(census: BTreeMap<String, BTreeSet<String>>) -> Vec<Self> {
        census
            .into_iter()
            .map(|(specifier, files)| Self {
                file_count: files.len(),
                example_file: files.into_iter().next().unwrap_or_default(),
                specifier,
            })
            .collect()
    }
}

pub fn analyze_trees(trees: &[(PathBuf, EngineConfig)]) -> MultiAnalyzeOutput {
    let mut outputs = Vec::with_capacity(trees.len());
    let mut source_ios = Vec::with_capacity(trees.len());
    // ONE git-collection memo for the whole run — why, and why it is behavior-preserving, is
    // `GitCache`'s own doc. Run-scoped BY CONSTRUCTION is the part that lives here: it is born on this
    // line and dropped when this call returns, so a long-lived MCP process can never serve a later
    // analysis from an earlier run's history.
    let git_cache = crate::analyze::GitCache::default();
    for (root, config) in trees {
        let mut output = crate::analyze_tree_with(root, config, &git_cache);
        let raw_io = output.ir.ir.io.clone().unwrap_or_default();
        let (join_io, dropped) = filter_join_io(raw_io);
        // Honest-disclosure side of `filter_join_io`'s exclusion (see that function's doc): when it
        // dropped anything, self-report it on the OWNING tree's own per-tree warnings channel — the same
        // "this tree's own config/facts had a filtered effect" precedent the topology-host tripwire and
        // tRPC mount-route suppression note below both use.
        if dropped.provides > 0 || dropped.consumes > 0 {
            let sample = dropped.examples.join(", ");
            output.warnings.push(format!(
                "cross-layer join input dropped {} test-classified provide(s) and {} test-classified \
                 consume(s) (file paths matching `zzop_core::is_test_file`, e.g. Go `_test.go`, TS \
                 `.test.ts`/`.spec.tsx`, Python `test_*.py`) before the cross-tree join, since a route or \
                 call registered only in test/fixture code is not real deployed surface: {sample}. Raw \
                 per-file facts still remain visible in this tree's own `ir.io` (the raw `zzop-facade` \
                 JSON output from embedding the engine directly; MCP tool replies and the `zzop` CLI \
                 omit `ir`) — only the JOIN input (`analyze_trees`' cross-layer output) is narrowed.",
                dropped.provides,
                dropped.consumes,
            ));
        }
        source_ios.push(SourceIo {
            source: config.source_id.clone(),
            io: join_io,
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
    // Cross-tree package-import disclosure — rationale in `cross_tree_imports`' module doc.
    cross_tree_imports::disclose(&mut outputs);
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
    // Per-tree attribute stores, keyed by source id — the provider-side lookup channel for
    // `cross-layer/retrying-write-no-idempotency`'s `idempotency-guarded` veto (an edge's `to.source`
    // picks the PROVIDER tree's store; native producer judgments and Mode B overlay injections both
    // already live in `AnalyzeOutput::attributes`, so the veto covers every provider language via
    // injection even where no native recognizer exists).
    let attribute_stores: BTreeMap<String, &zzop_core::AttributeStore> = outputs
        .iter()
        .map(|(_, source, output)| (source.clone(), &output.attributes))
        .collect();
    let cross_layer_findings = compute_cross_layer_findings(
        &source_ios,
        &cross_layer,
        trees,
        &package_imports,
        &trpc_participating_sources,
        &attribute_stores,
    );
    drop(attribute_stores); // end the immutable borrow of `outputs` before the mutable pushes below

    // Severity overrides for cross-layer findings are applied INSIDE `compute_cross_layer_findings`'s
    // final merge (union of every tree's overrides, first-declaring tree wins — see its doc): applying
    // them out here, after the merge's sort, would leave a remapped finding in its pre-override
    // position and break the documented (severity, file, line, ruleId) order (opus review, 2026-07-17).

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

    let mut warnings = Vec::new();
    if let Some(w) = parallel_impl::maybe_warn(&cross_layer, &cross_layer_findings) {
        warnings.push(w);
    }

    MultiAnalyzeOutput {
        trees: outputs,
        cross_layer,
        cross_layer_findings,
        warnings,
    }
}
