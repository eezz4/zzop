//! Envelope ingestion — the engine-side receiver for the external-parser Normalized AST protocol
//! (`docs/NORMALIZED_AST.md`). Projects a `zzop_core::NormalizedEnvelope`'s `FileProjection`s into the
//! same per-file shape `analyze::assemble` consumes, then runs the same whole-graph analyses
//! (dep-graph resolution, `circular`/`unreachable`/`dead-candidates`, `merge_findings`). An external
//! parser (Java/Python/JSP/anything this engine cannot parse natively) is therefore a first-class
//! citizen of every language-neutral analysis — the engine never sees the external parser's own AST,
//! only this projection.
//!
//! ## Deviations from the native per-file pass (documented, not bugs)
//!
//! - **No source text -> line-scan/method-scan DSL rules never run.** Those matchers scan source text
//!   directly; evaluating them against an empty string would silently look like "ran, found nothing"
//!   instead of "did not run". `SymbolScan`/`IoScan` only read `symbols`/`io`, which a `FileProjection`
//!   does supply, so `envelope_rule_pack` filters every pack down to just those two matcher kinds.
//!   Per-file lexical rules belong on the external parser's own side of the boundary.
//! - **No filesystem root -> no `dead-exports`/call-graph-BFS rules, no git-history analyses.** Those
//!   need a second disk read or a repository root, which an envelope has neither of; the affected
//!   `AnalyzeOutput` fields stay at their "git inactive" empty value, and a configured `git` option
//!   produces one `warnings` entry rather than a panic.
//! - **Dep-graph resolution treats import specifiers as repo-relative.** Edge resolution is a plain
//!   exact match against the envelope's own path set, not the TS parser's relative/extension-guessing
//!   resolver — an arbitrary external parser's `imports` map has no reason to follow TS conventions. An
//!   unmatched specifier is external, never an error; a `deferred` binding gets no edge (lazy import).
//!   [`resolve_envelope_specifier`] is a separate, narrower resolver used only for fragment
//!   `Ref`/`Mount` specifiers, which additionally understands `./`/`../` joins.
//! - **Fragment composition** (tRPC PROVIDEs, router-mount PROVIDEs) and late const-map CONSUME
//!   re-resolution run in envelope mode too, via the same composer functions the native path uses —
//!   only the resolver differs, since an envelope carries no tsconfig or workspace manifests to alias
//!   against.
//! - **No caching, no rule-timing profiling.** Both are ignored — envelope mode has no per-file disk
//!   content to hash and no per-rule timing loop wired for this smaller rule surface.

use std::collections::{HashMap, HashSet};

use zzop_core::{
    circular_from_dep, eval_pack, is_enabled, merge_findings, pack_loader, registry, CommonIr,
    DepGraph, Finding, GitStats, IoConsume, IoFacts, IoProvide, Matcher, MinimalIr,
    NormalizedEnvelope, RuleContext, RulePackDef, SourceFile, DEFAULT_WEIGHTS,
};

use crate::analyze::{
    circular_findings, dead_candidate_findings, dep_stats_from_dep, unreachable_findings,
};
use crate::{AnalyzeOutput, EngineConfig};

/// Ingests one `NormalizedEnvelope` (already validated — see `zzop_core::validate_envelope`) and
/// produces the same `AnalyzeOutput` shape `analyze_tree` does, per this module's doc for which
/// analyses run and which are skipped in envelope mode. Files are processed in `path`-sorted order
/// (mirroring `pipeline::run_file_pass`) so output is deterministic regardless of the envelope's own
/// file order.
pub fn analyze_envelope(envelope: &NormalizedEnvelope, config: &EngineConfig) -> AnalyzeOutput {
    let mut files: Vec<&zzop_core::FileProjection> = envelope.files.iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let file_count = files.len();

    let all_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();

    // Pack-level AND per-rule `disabled_rules` gating, same split `pipeline::run_file_pass` uses:
    // pack-level drops a whole disabled pack via `is_enabled`, then `gate_pack_rules` (shared, not
    // duplicated) drops an individually-disabled `"{pack}/{rule}"` id. `envelope_rule_pack`'s
    // SymbolScan/IoScan-only filter runs last.
    let enabled_packs: Vec<RulePackDef> = config
        .packs
        .iter()
        .filter(|p| registry::is_enabled(&config.rule_config, &p.id))
        .map(|p| crate::pipeline::gate_pack_rules(p, &config.rule_config))
        .map(|p| envelope_rule_pack(&p))
        .filter(|p| !p.rules.is_empty())
        .collect();

    let mut loc_by_path: HashMap<String, u32> = HashMap::new();
    let mut degraded: Vec<String> = Vec::new();
    let mut all_symbols = Vec::new();
    let mut io_provides: Vec<IoProvide> = Vec::new();
    let mut io_consumes: Vec<IoConsume> = Vec::new();
    let mut dep: DepGraph = DepGraph::new();
    let mut per_file_findings: Vec<Finding> = Vec::new();
    // Fragment-composition substrate — the envelope-mode counterpart of `analyze::assemble`'s own
    // `trpc_fragment_pairs`/`router_mount_pairs`/`fragment_pairs`: collected during the per-file loop,
    // composed once after (path-paired so composition can sort for deterministic first-writer-wins).
    let mut trpc_fragment_pairs: Vec<(String, Vec<zzop_core::TrpcRouterFragment>)> = Vec::new();
    let mut router_mount_pairs: Vec<(String, Vec<zzop_core::RouterMountFragment>)> = Vec::new();
    let mut const_fragment_pairs: Vec<(String, HashMap<String, String>)> = Vec::new();
    // Same summary `analyze::assemble` builds natively — see `AnalyzeOutput::package_imports`.
    let mut package_import_files: std::collections::BTreeMap<
        String,
        std::collections::BTreeSet<String>,
    > = std::collections::BTreeMap::new();

    for file in &files {
        loc_by_path.insert(file.path.clone(), file.loc);
        if file.degraded {
            degraded.push(file.path.clone());
        }
        all_symbols.extend(file.symbols.iter().cloned());
        io_provides.extend(file.io.provides.iter().cloned());
        io_consumes.extend(file.io.consumes.iter().cloned());
        if !file.trpc_router_fragments.is_empty() {
            trpc_fragment_pairs.push((file.path.clone(), file.trpc_router_fragments.clone()));
        }
        if !file.router_mount_fragments.is_empty() {
            router_mount_pairs.push((file.path.clone(), file.router_mount_fragments.clone()));
        }
        if !file.const_map_fragment.is_empty() {
            const_fragment_pairs.push((file.path.clone(), file.const_map_fragment.clone()));
        }

        // Every file gets a `dep` entry (even an empty edge list) so `dep_stats_from_dep` below counts
        // it as a graph node, letting an isolated (import-free) file still get a `FileNode`.
        let mut seen = HashSet::new();
        let mut targets = Vec::new();
        for binding in file.imports.values() {
            // Non-relative specifier naming no projected file = a package import — summarized for
            // `cross-layer/sdk-import-no-visible-consume`.
            if !binding.specifier.starts_with('.')
                && !binding.specifier.starts_with('/')
                && !all_paths.contains(binding.specifier.as_str())
            {
                package_import_files
                    .entry(binding.specifier.clone())
                    .or_default()
                    .insert(file.path.clone());
            }
            if binding.deferred {
                continue; // lazy import: no module-load edge.
            }
            if binding.specifier != file.path
                && all_paths.contains(binding.specifier.as_str())
                && seen.insert(binding.specifier.clone())
            {
                targets.push(binding.specifier.clone());
            }
        }
        dep.insert(file.path.clone(), targets);

        // Per-file DSL pass — symbol-scan/io-scan only (see module doc). `text` is empty since an
        // envelope carries no source lines.
        let source_file = SourceFile {
            rel: file.path.clone(),
            text: String::new(),
            symbols: file.symbols.clone(),
            io: Some(file.io.clone()),
        };
        let ctx_files = std::slice::from_ref(&source_file);
        let ctx = RuleContext {
            files: ctx_files,
            ir: None,
        };
        for pack in &enabled_packs {
            if pack_loader::applies_to(pack, &file.path) {
                per_file_findings.extend(eval_pack(pack, &ctx));
            }
        }
    }

    // Fragment composition + late const-map consume re-resolution must run before `io_provides`/
    // `io_consumes` are sorted and frozen into `MinimalIr::io` below.
    if !trpc_fragment_pairs.is_empty() {
        let composed =
            crate::analyze::compose_trpc_provides(trpc_fragment_pairs, |specifier, from_file| {
                resolve_envelope_specifier(specifier, from_file, &all_paths)
            });
        io_provides.extend(composed);
    }
    if !router_mount_pairs.is_empty() {
        let composed = crate::analyze::compose_router_mount_provides(
            router_mount_pairs,
            |specifier, from_file| resolve_envelope_specifier(specifier, from_file, &all_paths),
        );
        io_provides.extend(composed);
    }
    crate::analyze::late_resolve_cross_file_consumes(const_fragment_pairs, &mut io_consumes);

    let cycles = circular_from_dep(&dep);
    let dep_stats = dep_stats_from_dep(&dep);
    // Every `FileProjection` is, by construction, a parsed-source file (an external parser only ever
    // projects source it understood) — so `is_source` is unconditionally true here, unlike
    // `analyze::assemble`'s dispatch-backed classifier.
    let nodes = zzop_core::build_file_nodes(
        &dep_stats,
        &GitStats::default(),
        &loc_by_path,
        &DEFAULT_WEIGHTS,
        |_| true,
    );

    // `AnalyzeOutput::folders` is not git-gated, so envelope mode gets a real rollup too.
    // `layer_co_churn` stays `None`: envelope mode never has real commit history.
    let folders = Some(zzop_metrics::build_folder_aggregates(
        &nodes,
        &dep,
        zzop_metrics::DEFAULT_FOLDER_DEPTH,
    ));

    let mut warnings = Vec::new();
    if let Some(w) = crate::analyze::zero_packs_warning(config) {
        warnings.push(w);
    }
    if config.git.is_some() {
        warnings.push(
            "git collection skipped: envelope mode has no filesystem root to collect history from"
                .to_string(),
        );
    }

    let mut global_findings = Vec::new();
    if is_enabled(&config.rule_config, "circular") {
        global_findings.extend(circular_findings(&cycles));
    }
    if is_enabled(&config.rule_config, "unreachable") {
        global_findings.extend(unreachable_findings(&nodes, &dep));
    }
    if is_enabled(&config.rule_config, "dead-candidates") {
        // No filesystem root (see module doc) -> no way to read package.json-referenced entries, so
        // this always passes an empty `extra_entries` set.
        global_findings.extend(dead_candidate_findings(&nodes, &dep, &HashSet::new()));
    }

    let findings = merge_findings(
        vec![per_file_findings, global_findings],
        &config.rule_config,
    );

    degraded.sort();
    io_provides.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    io_consumes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    let io = if io_provides.is_empty() && io_consumes.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides: io_provides,
            consumes: io_consumes,
        })
    };

    let ir = CommonIr {
        source: config.source_id.clone(),
        parser: envelope.parser.clone(),
        ir: MinimalIr {
            dep,
            symbols: all_symbols,
            loc: loc_by_path,
            io,
        },
    };

    let package_imports = package_import_files
        .into_iter()
        .map(|(specifier, files)| crate::PackageImportSummary {
            file_count: files.len(),
            example_file: files.into_iter().next().unwrap_or_default(),
            specifier,
        })
        .collect();

    AnalyzeOutput {
        ir,
        findings,
        degraded,
        file_count,
        package_imports,
        nodes,
        scores: None,
        health: None,
        recommendations: Vec::new(),
        critical: Vec::new(),
        seams: Vec::new(),
        folders,
        layer_co_churn: None,
        warnings,
        cache: None,
        rule_timings: None,
    }
}

/// Merges each of `overlays` onto `artifacts` in place — the Mode B counterpart of `analyze_envelope`
/// (Mode A): a partial envelope (typically just `io` + fragment channels for a handful of files) folded
/// onto the native per-file artifacts a real `analyze_tree` run already produced, rather than an
/// envelope standing in for the entire tree. This is how an external framework adapter participates in
/// a native run without reimplementing a full parser (`EngineConfig::adapter_overlays`; empty = the
/// pre-overlay path, byte-for-byte).
///
/// Overlays are processed in `parser`-sorted order (deterministic regardless of assembly order) and
/// each is re-validated via `zzop_core::validate_envelope` first — a malformed overlay degrades to one
/// `warnings` entry naming its `parser` id and first few issues, then is skipped entirely.
///
/// Per `FileProjection`: if `path` matches an existing artifact's `rel`, it's merged in place — `io`
/// entries appended minus exact-duplicate `(kind, key, file, line)` tuples (`file` normalized to
/// `projection.path` first), fragments appended with no dedup (composition dedups later), and
/// `const_map_fragment` native-first (existing key wins). If `path` names no existing artifact (e.g. a
/// `.py`/`.jsp` sibling the native dispatch table doesn't recognize), it's pushed as a synthetic
/// `FileArtifact` with every native-only field at its empty/default value.
///
/// `artifacts` is re-sorted by `rel` before returning — `analyze::assemble` relies on that order for
/// `ir.ir.symbols`'s determinism.
pub(crate) fn apply_adapter_overlays(
    artifacts: &mut Vec<crate::pipeline::FileArtifact>,
    overlays: &[NormalizedEnvelope],
    warnings: &mut Vec<String>,
) {
    let mut ordered: Vec<&NormalizedEnvelope> = overlays.iter().collect();
    ordered.sort_by(|a, b| a.parser.cmp(&b.parser));

    for overlay in ordered {
        let json = match serde_json::to_string(overlay) {
            Ok(j) => j,
            Err(e) => {
                warnings.push(format!(
                    "adapter overlay '{}' skipped: failed to serialize for validation: {e}",
                    overlay.parser
                ));
                continue;
            }
        };
        if let Err(issues) = zzop_core::validate_envelope(&json) {
            let detail = issues
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            warnings.push(format!(
                "adapter overlay '{}' skipped: {detail}",
                overlay.parser
            ));
            continue;
        }

        for projection in &overlay.files {
            if let Some(artifact) = artifacts.iter_mut().find(|a| a.rel == projection.path) {
                merge_projection_onto_artifact(artifact, projection);
            } else {
                artifacts.push(synthetic_artifact_from_projection(projection));
            }
        }
    }

    artifacts.sort_by(|a, b| a.rel.cmp(&b.rel));
}

/// Overwrites every `IoProvide`/`IoConsume` in `io`'s `file` field to `path` — the defensive
/// normalization `apply_adapter_overlays` describes: an overlay is not trusted to already have set
/// `file` to match its own `FileProjection::path`.
fn normalize_io_file_field(io: &mut IoFacts, path: &str) {
    for provide in &mut io.provides {
        provide.file = path.to_string();
    }
    for consume in &mut io.consumes {
        consume.file = path.to_string();
    }
}

/// The "found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge (see that function's doc
/// for the dedup/native-first semantics per channel).
fn merge_projection_onto_artifact(
    artifact: &mut crate::pipeline::FileArtifact,
    projection: &zzop_core::FileProjection,
) {
    let mut incoming_io = projection.io.clone();
    normalize_io_file_field(&mut incoming_io, &projection.path);

    let existing = artifact.io.get_or_insert_with(IoFacts::default);
    for provide in incoming_io.provides {
        let dup = existing.provides.iter().any(|p| {
            p.kind == provide.kind
                && p.key == provide.key
                && p.file == provide.file
                && p.line == provide.line
        });
        if !dup {
            existing.provides.push(provide);
        }
    }
    for consume in incoming_io.consumes {
        let dup = existing.consumes.iter().any(|c| {
            c.kind == consume.kind
                && c.key == consume.key
                && c.file == consume.file
                && c.line == consume.line
        });
        if !dup {
            existing.consumes.push(consume);
        }
    }

    artifact
        .trpc_router_fragments
        .extend(projection.trpc_router_fragments.iter().cloned());
    artifact
        .router_mount_fragments
        .extend(projection.router_mount_fragments.iter().cloned());
    for (key, value) in &projection.const_map_fragment {
        artifact
            .const_map_fragment
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

/// The "not found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge — builds a brand-new
/// `FileArtifact` for a `path` the native pass never dispatched at all.
fn synthetic_artifact_from_projection(
    projection: &zzop_core::FileProjection,
) -> crate::pipeline::FileArtifact {
    let mut io = projection.io.clone();
    normalize_io_file_field(&mut io, &projection.path);
    let io = if io.provides.is_empty() && io.consumes.is_empty() {
        None
    } else {
        Some(io)
    };

    crate::pipeline::FileArtifact {
        rel: projection.path.clone(),
        symbols: Vec::new(),
        imports: None,
        loc: projection.loc,
        findings: Vec::new(),
        degraded: false,
        minified_or_generated: false,
        io,
        rule_timings: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: projection.const_map_fragment.clone(),
        trpc_router_fragments: projection.trpc_router_fragments.clone(),
        router_mount_fragments: projection.router_mount_fragments.clone(),
        // Wrapper resolution is a native-TS-source concern; an external adapter emits final
        // io/router fragments instead, so a synthetic overlay artifact never carries these.
        wrapper_def_fragments: Vec::new(),
        wrapper_call_fragments: Vec::new(),
    }
}

/// Resolves one fragment `Ref`/`Mount` specifier for envelope-mode composition — no tsconfig/
/// workspace-alias machinery, since an envelope's `FileProjection::path` set is the entire addressable
/// universe. Contract: (a) an exact match of `specifier` against known file paths wins outright; (b)
/// else, if `specifier` starts with `./` or `../`, join it against `from_file`'s own directory
/// (normalizing `.`/`..` segments as pure string ops, no filesystem APIs), try that joined path as-is,
/// then try appending each of `.ts`/`.tsx`/`.js` in turn; (c) anything else resolves to `None` —
/// external/unresolved, never guessed.
fn resolve_envelope_specifier(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<&str>,
) -> Option<String> {
    if all_paths.contains(specifier) {
        return Some(specifier.to_string());
    }
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }

    // `from_file`'s own directory, as path segments (envelope paths are contractually forward-slash,
    // so plain `/`-splitting avoids `std::path::Path`'s Windows-backslash normalization surprises).
    let mut segments: Vec<&str> = from_file.split('/').collect();
    segments.pop(); // drop the file's own basename, keeping just its directory

    for part in specifier.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                segments.pop();
            }
            seg => segments.push(seg),
        }
    }
    let joined = segments.join("/");

    if all_paths.contains(joined.as_str()) {
        return Some(joined);
    }
    for ext in [".ts", ".tsx", ".js"] {
        let candidate = format!("{joined}{ext}");
        if all_paths.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// `pack`, with every rule whose matcher is not `SymbolScan`/`IoScan` dropped — see module doc for why.
fn envelope_rule_pack(pack: &RulePackDef) -> RulePackDef {
    let mut p = pack.clone();
    p.rules
        .retain(|r| matches!(r.matcher, Matcher::SymbolScan(_) | Matcher::IoScan(_)));
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::{
        FileProjection, ImportBinding, ImportMap, SourceSymbol, SourceSymbolKind,
        NORMALIZED_AST_FORMAT,
    };

    fn projection(path: &str, loc: u32) -> FileProjection {
        FileProjection {
            path: path.to_string(),
            loc,
            symbols: Vec::new(),
            imports: ImportMap::new(),
            re_exports: Vec::new(),
            used_names: Vec::new(),
            const_map_fragment: HashMap::new(),
            trpc_router_fragments: Vec::new(),
            router_mount_fragments: Vec::new(),
            io: IoFacts::default(),
            degraded: false,
        }
    }

    fn envelope(files: Vec<FileProjection>) -> NormalizedEnvelope {
        NormalizedEnvelope {
            format: NORMALIZED_AST_FORMAT.to_string(),
            version: 1,
            parser: "test-parser/1".to_string(),
            source: "test".to_string(),
            files,
        }
    }

    fn config() -> EngineConfig {
        EngineConfig {
            source_id: "test".to_string(),
            ..EngineConfig::default()
        }
    }

    #[test]
    fn projects_loc_and_symbols_into_the_common_ir() {
        let mut a = projection("a.jsp", 10);
        a.symbols.push(SourceSymbol {
            id: "a.jsp#Foo".to_string(),
            file: "a.jsp".to_string(),
            name: "Foo".to_string(),
            kind: SourceSymbolKind::Class,
            line: 1,
            exported: true,
            is_default: false,
            body_start: None,
            body_end: None,
        });
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        assert_eq!(out.file_count, 1);
        assert_eq!(out.ir.ir.loc.get("a.jsp"), Some(&10));
        assert_eq!(out.ir.ir.symbols.len(), 1);
        assert_eq!(out.ir.parser, "test-parser/1");
        assert_eq!(out.ir.source, "test");
    }

    #[test]
    fn resolves_dep_edge_when_specifier_matches_a_projected_path() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "b".to_string(),
            ImportBinding {
                specifier: "b.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let b = projection("b.jsp", 5);
        let env = envelope(vec![a, b]);
        let out = analyze_envelope(&env, &config());
        assert_eq!(
            out.ir.ir.dep.get("a.jsp").cloned().unwrap_or_default(),
            vec!["b.jsp".to_string()]
        );
        assert_eq!(
            out.ir.ir.dep.get("b.jsp").cloned().unwrap_or_default(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unresolvable_specifier_is_external_not_an_error() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "ext".to_string(),
            ImportBinding {
                specifier: "some/external/package".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        assert!(out
            .ir
            .ir
            .dep
            .get("a.jsp")
            .cloned()
            .unwrap_or_default()
            .is_empty());
    }

    #[test]
    fn degraded_file_is_reported_but_loc_still_counted() {
        let mut a = projection("a.jsp", 3);
        a.degraded = true;
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        assert_eq!(out.degraded, vec!["a.jsp".to_string()]);
        assert_eq!(out.ir.ir.loc.get("a.jsp"), Some(&3));
    }

    #[test]
    fn circular_import_pair_produces_a_circular_finding() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "b".to_string(),
            ImportBinding {
                specifier: "b.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let mut b = projection("b.jsp", 5);
        b.imports.insert(
            "a".to_string(),
            ImportBinding {
                specifier: "a.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let env = envelope(vec![a, b]);
        let out = analyze_envelope(&env, &config());
        assert!(out.findings.iter().any(|f| f.rule_id == "circular"));
    }

    #[test]
    fn io_facts_are_collected_and_surfaced_on_the_common_ir() {
        let mut a = projection("Ctrl.jsp", 20);
        a.io.provides.push(IoProvide {
            kind: "http".to_string(),
            key: "GET /legacy/user.jsp".to_string(),
            file: "Ctrl.jsp".to_string(),
            line: 3,
            symbol: None,
        });
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        let io = out.ir.ir.io.expect("expected io facts");
        assert_eq!(io.provides.len(), 1);
        assert_eq!(io.provides[0].key, "GET /legacy/user.jsp");
    }

    #[test]
    fn git_config_is_ignored_with_a_warning_and_never_panics() {
        let mut cfg = config();
        cfg.git = Some(crate::GitOptions::default());
        let env = envelope(vec![projection("a.jsp", 1)]);
        let out = analyze_envelope(&env, &cfg);
        assert!(out.scores.is_none());
        assert!(out.health.is_none());
        assert!(out
            .warnings
            .iter()
            .any(|w| w.contains("git collection skipped")));
    }

    #[test]
    fn symbol_scan_dsl_rule_fires_against_envelope_symbols() {
        let pack: RulePackDef = serde_json::from_str(
            r#"{"id":"t","framework":"any","rules":[{"id":"r","severity":"info","message":"m","matcher":{"type":"symbol-scan","file_pattern":"\\.jsp$","name_pattern":"^Bad"}}]}"#,
        )
        .unwrap();
        let mut a = projection("a.jsp", 5);
        a.symbols.push(SourceSymbol {
            id: "a.jsp#BadName".to_string(),
            file: "a.jsp".to_string(),
            name: "BadName".to_string(),
            kind: SourceSymbolKind::Function,
            line: 4,
            exported: true,
            is_default: false,
            body_start: None,
            body_end: None,
        });
        let env = envelope(vec![a]);
        let mut cfg = config();
        cfg.packs = vec![pack];
        let out = analyze_envelope(&env, &cfg);
        assert!(out.findings.iter().any(|f| f.rule_id == "t/r"));
    }

    #[test]
    fn line_scan_dsl_rule_never_fires_in_envelope_mode() {
        // A LineScan rule that would match "TODO" if it ever saw source text — envelope mode carries no
        // text, so the rule is filtered out rather than silently "running clean".
        let pack: RulePackDef = serde_json::from_str(
            r#"{"id":"t","framework":"any","rules":[{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.jsp$","line_pattern":"TODO"}}]}"#,
        )
        .unwrap();
        let env = envelope(vec![projection("a.jsp", 1)]);
        let mut cfg = config();
        cfg.packs = vec![pack];
        let out = analyze_envelope(&env, &cfg);
        assert!(!out.findings.iter().any(|f| f.rule_id == "t/r"));
    }

    #[test]
    fn two_runs_over_the_same_envelope_are_byte_for_byte_identical() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "b".to_string(),
            ImportBinding {
                specifier: "b.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let env = envelope(vec![a, projection("b.jsp", 5)]);
        let out1 = analyze_envelope(&env, &config());
        let out2 = analyze_envelope(&env, &config());
        assert_eq!(
            serde_json::to_value(&out1.ir).unwrap(),
            serde_json::to_value(&out2.ir).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&out1.findings).unwrap(),
            serde_json::to_value(&out2.findings).unwrap()
        );
    }

    #[test]
    fn router_mount_fragments_split_across_two_files_compose_into_one_http_provide() {
        use zzop_core::{RouterMountEntry, RouterMountFragment};

        // Mount file: an "app" router mounting "sub" at "/api", by exact-path specifier.
        let mut mount_file = projection("app.jsp", 4);
        mount_file.router_mount_fragments.push(RouterMountFragment {
            name: "app".to_string(),
            entries: vec![RouterMountEntry::Mount {
                prefix: "/api".to_string(),
                ident: "sub".to_string(),
                specifier: Some("sub.jsp".to_string()),
            }],
        });

        // Sub-router file: registers one verb, `POST /widgets`.
        let mut sub_file = projection("sub.jsp", 3);
        sub_file.router_mount_fragments.push(RouterMountFragment {
            name: "sub".to_string(),
            entries: vec![RouterMountEntry::Verb {
                method: "POST".to_string(),
                path: "/widgets".to_string(),
                handler: Some("createWidget".to_string()),
                line: 2,
            }],
        });

        let env = envelope(vec![mount_file, sub_file]);
        let out = analyze_envelope(&env, &config());
        let provides = out.ir.ir.io.expect("expected io facts").provides;
        assert!(
            provides
                .iter()
                .any(|p| p.kind == "http" && p.key == "POST /api/widgets" && p.file == "sub.jsp"),
            "{:?}",
            provides
        );
    }

    mod resolve_envelope_specifier_tests {
        use super::super::resolve_envelope_specifier;
        use std::collections::HashSet;

        #[test]
        fn relative_dot_slash_resolves_against_the_emitting_files_own_directory() {
            let all: HashSet<&str> = ["a/b.ts", "a/sibling.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./sibling", "a/b.ts", &all),
                Some("a/sibling.ts".to_string())
            );
        }

        #[test]
        fn parent_relative_dot_dot_slash_walks_up_one_directory() {
            let all: HashSet<&str> = ["a/b/c.ts", "a/x.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("../x", "a/b/c.ts", &all),
                Some("a/x.ts".to_string())
            );
        }

        #[test]
        fn exact_match_wins_over_relative_join() {
            // "./x" from "a/b.ts" would join to "a/x" — but an exact path literally named "./x" must win
            // outright per the documented precedence.
            let all: HashSet<&str> = ["./x", "a/x.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./x", "a/b.ts", &all),
                Some("./x".to_string())
            );
        }

        #[test]
        fn extension_guessing_finds_a_real_source_file_behind_an_extensionless_join() {
            let all: HashSet<&str> = ["a/sibling.tsx"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./sibling", "a/b.ts", &all),
                Some("a/sibling.tsx".to_string())
            );
        }

        #[test]
        fn unresolvable_specifier_is_none() {
            let all: HashSet<&str> = ["a/b.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("some-package", "a/b.ts", &all),
                None
            );
            assert_eq!(
                resolve_envelope_specifier("./missing", "a/b.ts", &all),
                None
            );
        }
    }
}
