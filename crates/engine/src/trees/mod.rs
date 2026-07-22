//! Cross-layer multi-tree API — `analyze_trees` and its `MultiAnalyzeOutput`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use zzop_core::{Finding, IoFacts, RuleConfig, SourceIo};

use crate::cross_layer_findings::compute_cross_layer_findings;
use crate::{analyze_tree, AnalyzeOutput, EngineConfig};

mod parallel_impl;

pub use parallel_impl::MIN_PARALLEL_IMPL_SIGNALS;

/// Per-tree drop counts + a capped file-path sample from [`filter_join_io`] — substrate for that
/// function's caller's own per-tree warning (see the call site's doc for the disclosure rationale).
/// `examples` combines BOTH dropped provides and dropped consumes (provides first, in their original
/// order, then consumes), capped at 3 total — the same "up to 3 example paths" convention
/// `unparsed_extension_warning` already uses for its own per-extension sample. DISTINCT file paths
/// only: one test file usually carries several dropped facts, and "a.go, a.go, a.go" tells the
/// reader nothing the count didn't (observed in the first live run of this warning).
#[derive(Default)]
struct JoinIoDrop {
    provides: usize,
    consumes: usize,
    examples: Vec<String>,
}

/// Cross-layer JOIN input filter: drops every provide/consume whose `file` is test-classified
/// (`zzop_core::is_test_file`) before it ever reaches `link_cross_layer_io`/`compute_cross_layer_findings`.
/// The published disclosure (`disclosure.rs`'s "classified-skip" class) claims test-classified io is
/// excluded from the cross-layer join — before this filter existed that claim was false: the join input
/// was built straight from each tree's raw `output.ir.ir.io`, so e.g. a Go `unit_test.go` route
/// registration became an ordinary production "provide" and could join a real cross-tree edge (observed
/// live: 4 of 5 provides on a real repo were test-harness routes). Deliberately does NOT touch
/// `output.ir` — the per-file raw facts (test-classified included) must stay visible in that tree's own
/// single-tree output; only the JOIN input built here is narrowed.
fn filter_join_io(io: IoFacts) -> (IoFacts, JoinIoDrop) {
    let mut drop = JoinIoDrop::default();
    let provides = io
        .provides
        .into_iter()
        .filter(|p| {
            let is_test = zzop_core::is_test_file(&p.file);
            if is_test {
                drop.provides += 1;
                if drop.examples.len() < 3 && !drop.examples.contains(&p.file) {
                    drop.examples.push(p.file.clone());
                }
            }
            !is_test
        })
        .collect();
    let consumes = io
        .consumes
        .into_iter()
        .filter(|c| {
            let is_test = zzop_core::is_test_file(&c.file);
            if is_test {
                drop.consumes += 1;
                if drop.examples.len() < 3 && !drop.examples.contains(&c.file) {
                    drop.examples.push(c.file.clone());
                }
            }
            !is_test
        })
        .collect();
    (IoFacts { provides, consumes }, drop)
}

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

pub fn analyze_trees(trees: &[(PathBuf, EngineConfig)]) -> MultiAnalyzeOutput {
    let mut outputs = Vec::with_capacity(trees.len());
    let mut source_ios = Vec::with_capacity(trees.len());
    for (root, config) in trees {
        let mut output = analyze_tree(root, config);
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
