//! Request -> `EngineConfig` assembly: pack loading/merging and the tree-rooted config knobs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use zzop_core::{load_dsl_packs, GlobalExclude, RulePackDef, Severity, Suppression};
use zzop_engine::{EngineConfig, GitOptions, MountRule, PackSource, DEFAULT_SIZE_CAP};

use crate::request::{AnalyzeRequest, MountEntryRequest, PacksDir};

/// The shared "seed `pack_defs`, load `packs_dir`, build the DSL-pack list + `RuleConfig`" step both
/// `build_engine_config` (tree-rooted requests) and `analyze_envelope_json` (envelope requests) need.
///
/// The pack list is built in two layers, in this order:
/// 1. `pack_defs` (inline, data-injected packs — see `AnalyzeRequest::pack_defs`) seed the list first, in
///    array order; a same-id collision AMONG `pack_defs` themselves follows the same later-wins-whole rule
///    as step 2 below (reusing the identical collision loop). Each def first faces the loader's DSL
///    schema-version gate (`zzop_core::check_dsl_schema_version`) — a too-new pack is skipped with a
///    by-name warning instead of running silently misinterpreted (the same verdict `packs_dir`/
///    `validate_rule_pack` give the identical bytes).
/// 2. `packs_dirs` is loaded in order, one `zzop_core::pack_loader::load_dsl_packs` call per directory, and
///    merged into the same list: if a loaded pack (from any directory, or from step 1's `pack_defs`) shares
///    a `RulePackDef::id` with a pack already in the list, the LATER one REPLACES the earlier one whole —
///    not a rule-level merge inside that pack id. Since directories are always folded in AFTER `pack_defs`,
///    a directory pack always wins a same-id collision against an inline def — this is the intentional
///    override path (see `docs/modules/napi.md`'s "Defaults" section) — the JS wrapper (`index.js`) puts
///    the bundled default pack dir first and any caller-supplied `packsDir` after it, so a caller's pack
///    always wins a collision against a shipped one with the same id, while packs with distinct ids from
///    every source all stay loaded together. Per-directory load errors (a malformed `rules/dsl/*.json`, an
///    unreadable directory) are pushed onto `warnings` rather than failing the whole call — same "surface,
///    don't crash" contract `load_dsl_packs` itself documents; the caller folds `warnings` into the
///    corresponding `AnalyzeOutput`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn base_engine_config(
    source_id: &str,
    pack_defs: &[RulePackDef],
    packs_dirs: &[&str],
    disabled_rules: &[String],
    severity_overrides: &BTreeMap<String, Severity>,
    suppressions: &[Suppression],
    global_excludes: &[GlobalExclude],
    warnings: &mut Vec<String>,
) -> EngineConfig {
    // Provenance for `AnalyzeOutput::packs_loaded` (pack id -> "inline" | "dir"), maintained alongside
    // the pack list: an insert AND a same-id replacement both stamp the map, so after a collision the
    // map reports the source of the pack that actually WON (e.g. a directory pack overriding an inline
    // def reports "dir"). Packs are only ever inserted/replaced, never removed, so the map's key set
    // always equals the final pack-id set.
    let mut pack_sources: BTreeMap<String, PackSource> = BTreeMap::new();
    let mut packs: Vec<RulePackDef> = Vec::new();
    for def in pack_defs {
        // Inline defs never pass through the loader's text path (`parse_dsl_pack`), so the DSL
        // schema-version gate is re-applied HERE — the one chokepoint every `packDefs` entry funnels
        // through (caller-supplied and bundled-seed alike; bundled packs are schema_version 1, so
        // this is a no-op for them). Same verdict, same wording as the loader
        // (`zzop_core::check_dsl_schema_version` — one wording, no fork), surfaced as a by-name
        // warning + skip rather than a failure, matching the `packs_dir` load-error contract below.
        if let Err(msg) = zzop_core::check_dsl_schema_version(def) {
            warnings.push(format!("packDefs: pack \"{}\" skipped: {msg}", def.id));
            continue;
        }
        pack_sources.insert(def.id.clone(), PackSource::Inline);
        match packs.iter_mut().find(|existing| existing.id == def.id) {
            Some(slot) => *slot = def.clone(), // later inline def wins whole on a same-id collision
            None => packs.push(def.clone()),
        }
    }
    for dir in packs_dirs {
        // A degenerate empty-string entry is a caller error answered BY NAME, not passed to the
        // loader (which would surface a confusing `failed to load : (os error ...)` for `Path::new("")`).
        // This loop is the one chokepoint every pack-dir shape funnels through (`analyze`/
        // `analyzeTrees` via `build_engine_config`, `analyzeEnvelope` directly), string and array forms alike.
        if dir.is_empty() {
            warnings.push(
                "packs_dir entry is an empty string — ignored (pass a directory path, or null to opt out)"
                    .to_string(),
            );
            continue;
        }
        let result = load_dsl_packs(Path::new(dir));
        for (path, pack) in result.packs {
            let _ = path; // load order already deterministic (sorted by file name) — path not needed here.
            pack_sources.insert(pack.id.clone(), PackSource::Dir);
            match packs.iter_mut().find(|existing| existing.id == pack.id) {
                Some(slot) => *slot = pack, // later directory wins whole-pack on a same-id collision
                None => packs.push(pack),
            }
        }
        for err in result.errors {
            warnings.push(format!(
                "packs_dir: failed to load {}: {}",
                err.path.display(),
                err.message
            ));
        }
    }

    EngineConfig {
        source_id: source_id.to_string(),
        packs,
        pack_sources,
        rule_config: zzop_core::RuleConfig {
            disabled_rules: disabled_rules.to_vec(),
            severity_overrides: severity_overrides.clone(),
            suppressions: suppressions.to_vec(),
            global_excludes: global_excludes.to_vec(),
        },
        ..EngineConfig::default()
    }
}

/// Builds one `EngineConfig` from one `AnalyzeRequest` — `base_engine_config` plus the tree-rooted knobs
/// (`size_cap`/`cache_dir`/`git`) an envelope request has no equivalent for.
pub(crate) fn build_engine_config(
    req: &AnalyzeRequest,
    warnings: &mut Vec<String>,
) -> EngineConfig {
    let packs_dirs = req
        .packs_dir
        .as_ref()
        .map(PacksDir::as_dirs)
        .unwrap_or_default();
    let mut config = base_engine_config(
        &req.source_id,
        &req.pack_defs,
        &packs_dirs,
        &req.disabled_rules,
        &req.severity_overrides,
        &req.suppressions,
        &req.global_excludes,
        warnings,
    );

    config.size_cap = req.size_cap.unwrap_or(DEFAULT_SIZE_CAP);
    config.cache_dir = req.cache_dir.as_ref().map(PathBuf::from);
    config.git = req.git.as_ref().map(|g| GitOptions {
        since: g.since.clone(),
        recent_days: g
            .recent_days
            .unwrap_or_else(|| GitOptions::default().recent_days),
        commit_type_patterns: g.commit_type_patterns.as_ref().map(|patterns| {
            patterns
                .iter()
                .map(|p| (p.pattern.clone(), p.tag.clone()))
                .collect()
        }),
    });
    // Overlays flow to `analyze_tree`'s unconditional `apply_adapter_overlays` merge; no cache-key
    // impact (applied post-cache, re-applied every run regardless of hit/miss).
    config.adapter_overlays = req.adapter_overlays.clone();

    config.mounts = fold_mounts(&req.mounts, req.mounted_at.as_deref());
    config.hosts = req.hosts.clone();

    config
}

/// Deployment-topology mount fold, shared by BOTH request paths (`build_engine_config` for
/// tree-rooted requests, `analyze_envelope_json` for envelope requests — one fold, so the two wire
/// paths cannot drift): every `mounts[]` entry folds in FIRST, in array order, followed by
/// `mounted_at` as the implicit whole-tree entry (`dir: ""`) LAST. The engine's own
/// `apply_config_mounts` picks the longest matching `dir` on a match and resolves equal-length ties
/// to the first entry — appending `mounted_at` last so an explicit dir entry of equal length wins
/// ties (an explicit `{dir:"", at:"..."}` mount, the one shape that can tie with `mounted_at`'s
/// empty `dir`, is more specific intent than the shorthand and should win). No shape validation
/// happens here (see `AnalyzeRequest::mounted_at`/`mounts`'s docs) — this is a plain, unchecked
/// pass-through.
pub(crate) fn fold_mounts(
    mounts: &[MountEntryRequest],
    mounted_at: Option<&str>,
) -> Vec<MountRule> {
    let mut folded: Vec<MountRule> = mounts
        .iter()
        .map(|m| MountRule {
            dir: m.dir.clone(),
            at: m.at.clone(),
        })
        .collect();
    if let Some(at) = mounted_at {
        folded.push(MountRule {
            dir: String::new(),
            at: at.to_string(),
        });
    }
    folded
}
