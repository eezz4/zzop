//! Fused per-file pass: for each file, parse -> project this file's slice of Common IR
//! (symbols/loc) -> run every applicable DSL pack against that slice -> return plain data. The
//! parser's AST never leaves the function that calls the parser — only `zzop_core` types
//! (`SourceSymbol`, `ImportMap`, `Finding`, `u32` loc) cross back into this module.
//!
//! Files are processed via `rayon::par_iter` over a single-threaded, pre-sorted walk
//! (`walk_files`), and `run_file_pass` re-sorts the results by path afterward — belt-and-suspenders
//! so output order does not depend on `rayon`'s collect-order guarantee holding across versions.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ignore::gitignore::Gitignore;
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::Regex;

use zzop_cache::{AnalysisCache, CacheKey, FileIrSlice};
use zzop_core::{
    dsl::{eval_pack, eval_pack_profiled, RuleContext, RuleTiming, SourceFile},
    ir::SourceSymbol,
    pack_loader, registry, ImportMap, IoFacts, RulePackDef,
};

use crate::cache::CacheCounters;
use crate::dispatch::{self, DispatchConfig, Language};
use crate::EngineConfig;

/// One file's contribution to the tree-wide assembly (`analyze::assemble`) — plain data only.
/// `imports` is `Some` for files this engine can place in the TS dep graph (TypeScript-dispatched,
/// including degraded ones — an empty `ImportMap` still gives the file a graph node); `None` for
/// Prisma / lexical-only files, which never participate in `resolve::build_dep`. Below, several
/// fields share that "`None`/empty for non-TypeScript or degraded files" convention; noted once here
/// rather than repeated per field.
pub(crate) struct FileArtifact {
    pub rel: String,
    pub symbols: Vec<SourceSymbol>,
    pub imports: Option<ImportMap>,
    pub loc: u32,
    pub findings: Vec<zzop_core::Finding>,
    pub degraded: bool,
    /// Minified/generated classification — distinct from `degraded`: a degraded file still runs
    /// line-scan DSL rules against raw text, but this flag skips ALL DSL rule-pack evaluation.
    /// Structural extraction below is unaffected; this only gates `findings`.
    pub minified_or_generated: bool,
    /// Projected HTTP-egress/route `IoFacts` (see `crate::io`'s module doc for the fusion tradeoff).
    pub io: Option<IoFacts>,
    /// Per-rule DSL timing; empty when profiling is off or on a full cache hit. `analyze::assemble`
    /// sums these into `AnalyzeOutput::rule_timings`.
    pub rule_timings: Vec<RuleTiming>,
    /// Identifiers referenced anywhere in this file, sorted — feeds `dead-exports`' per-file "used
    /// names" (in-file-only liveness, never cross-file).
    pub used_names: Vec<String>,
    /// Constant-map fragment (same parse, no second pass) — `analyze::assemble` merges every file's
    /// fragment into one project-wide map to re-resolve consumes left unresolved.
    pub const_map_fragment: std::collections::HashMap<String, String>,
    /// tRPC router shape fragment — `analyze::compose_trpc_provides`'s substrate.
    pub trpc_router_fragments: Vec<zzop_core::TrpcRouterFragment>,
    /// Code-registered router-mount fragment (Hono chained builders / cross-file sub-router mounts) —
    /// provide-side sibling of `trpc_router_fragments`.
    pub router_mount_fragments: Vec<zzop_core::RouterMountFragment>,
    /// Wrapper-DEFINITION fragment — substrate for `analyze`'s assemble-time wrapper-consume join.
    pub wrapper_def_fragments: Vec<zzop_core::WrapperDefFragment>,
    /// Wrapper-CALL fragment — each call is resolved via its import specifier back to a def.
    pub wrapper_call_fragments: Vec<zzop_core::WrapperCallFragment>,
}

/// Runs the fused per-file pass over every file under `root` (skipping `config.dispatch.skip_dirs`) and
/// returns one `FileArtifact` per file, sorted by `rel`. `cache`/`counters` are `analyze_tree`'s
/// already-opened cache handle and shared hit/miss counters — both `None` when caching is off.
pub(crate) fn run_file_pass(
    root: &Path,
    config: &EngineConfig,
    cache: Option<&AnalysisCache>,
    counters: Option<&CacheCounters>,
) -> Vec<FileArtifact> {
    let files = walk_files(root, &config.dispatch);
    // Pack-level and per-rule `disabled_rules` gating happen once here, outside the per-file loop
    // (`pack_loader::applies_to` below is the remaining per-file pre-filter). A bare pack id drops the
    // whole pack; a `"{pack}/{rule}"` id drops just that rule.
    let gated_packs: Vec<RulePackDef> = config
        .packs
        .iter()
        .filter(|p| registry::is_enabled(&config.rule_config, &p.id))
        .map(|p| gate_pack_rules(p, &config.rule_config))
        .collect();
    let enabled_packs: Vec<&RulePackDef> = gated_packs.iter().collect();
    // Computed once per call (constant across every file in this pass), not per file. `None` when the
    // cache is off.
    let ruleset_fp = cache.map(|_| crate::cache::ruleset_fingerprint(&enabled_packs, config));

    let mut artifacts: Vec<FileArtifact> = files
        .par_iter()
        .map(|(rel, abs)| {
            process_file(
                rel,
                abs,
                config,
                &enabled_packs,
                cache,
                ruleset_fp.as_deref(),
                counters,
            )
        })
        .collect();
    artifacts.sort_by(|a, b| a.rel.cmp(&b.rel));
    artifacts
}

/// Per-rule `disabled_rules` gating: returns a clone of `pack` with every rule whose full
/// `"{pack.id}/{rule.id}"` id is disabled removed from `rules`. Called once per call (not per file),
/// shared by both `analyze_tree` and `analyze_envelope`. A pack left with zero rules behaves like an
/// empty pack downstream (`pack_loader::applies_to` returns `false`).
pub(crate) fn gate_pack_rules(pack: &RulePackDef, config: &zzop_core::RuleConfig) -> RulePackDef {
    let mut gated = pack.clone();
    gated
        .rules
        .retain(|rule| registry::is_enabled(config, &format!("{}/{}", pack.id, rule.id)));
    gated
}

/// Walks `root` collecting every file not under a `config.skip_dirs` directory and not excluded by a
/// committed `.gitignore` (nested ones, plus ancestor ones up to the git toplevel), as `(normalized rel
/// path, absolute path)` pairs sorted by the rel path. A read error on a subdirectory is swallowed —
/// the walk continues, never panics.
///
/// **Ancestor `.gitignore`s**: when `root` is below the git toplevel (e.g. a monorepo subdir), a
/// `.gitignore` above `root` is just as "committed" as one under it, and real `git` honors it.
/// `WalkBuilder`'s own `parents(true)` is unsuitable — it climbs unboundedly past the repo — so this
/// function does its own bounded walk (`ancestor_gitignores`): from `root` upward, stopping at the
/// first `.git` found, loading each ancestor `.gitignore` anchored to its own directory, OR'd with the
/// crate's built-in handling for files at-or-below `root`. Known gap: an at-or-below-`root` `!pattern`
/// re-inclusion of something an ancestor ignores would win under real `git` but not here.
///
/// **Determinism contract**: output must be byte-identical across machines/clones of the same commit,
/// so only `.gitignore` files on disk are honored — every machine-local ignore source (`core
/// .excludesFile`, `.git/info/exclude`, `WalkBuilder`'s own unbounded `parents`, ripgrep's `.ignore`)
/// is explicitly turned off, while `require_git`/`git_ignore` stay on so a non-git tree is still
/// scanned. Dotfiles are walked like any other file; symlinks are never followed (avoids loops/escaping
/// `root`). `config.skip_dirs` is enforced unconditionally via `filter_entry`, independent of
/// `.gitignore`; the walk root itself is exempt.
fn walk_files(root: &Path, config: &DispatchConfig) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let skip_config = config.clone();
    let ancestor_ignores = ancestor_gitignores(root);
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .ignore(false)
        .require_git(false)
        .git_ignore(true)
        .follow_links(false)
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            if let Some(ft) = entry.file_type() {
                if ft.is_dir() {
                    let name = entry.file_name().to_string_lossy();
                    if dispatch::is_skip_dir(&name, &skip_config) {
                        return false;
                    }
                }
            }
            !ancestor_ignored(entry, &ancestor_ignores)
        });
    for entry in builder.build().filter_map(Result::ok) {
        let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
        if is_file {
            out.push((to_rel(root, entry.path()), entry.path().to_path_buf()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The directory containing `.git` at or above `root` (a `.git` entry may be a dir or, for a worktree, a
/// file pointing elsewhere — presence alone marks the boundary, same as `git` itself checks). `None` if
/// the filesystem root is reached with no `.git` found (a non-git tree).
fn find_git_toplevel(root: &Path) -> Option<PathBuf> {
    let mut dir = root;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Every `.gitignore` between the git toplevel (inclusive) and `root` (exclusive — `root`'s own, and
/// everything below it, is already handled by `WalkBuilder`'s built-in nested traversal), ordered
/// farthest-from-`root` first. Empty when `root` is the toplevel, or no toplevel is found.
fn ancestor_gitignores(root: &Path) -> Vec<Gitignore> {
    let Some(toplevel) = find_git_toplevel(root) else {
        return Vec::new();
    };
    if toplevel == root {
        return Vec::new();
    }
    let mut dirs = Vec::new();
    let mut cur = root.parent();
    while let Some(dir) = cur {
        dirs.push(dir.to_path_buf());
        if dir == toplevel {
            break;
        }
        cur = dir.parent();
    }
    dirs.reverse(); // farthest (toplevel) first, nearest-to-root last.
    dirs.into_iter()
        .filter_map(|dir| {
            let gi_path = dir.join(".gitignore");
            if !gi_path.is_file() {
                return None;
            }
            // Errors (a malformed glob line) are swallowed: `Gitignore::new` still returns a matcher
            // built from whichever lines did parse.
            let (gitignore, _err) = Gitignore::new(&gi_path);
            Some(gitignore)
        })
        .collect()
}

/// Whether any ancestor `.gitignore` ignores `entry`. `ancestors` is ordered farthest-from-`root`
/// first, so a nearer matcher's verdict overrides a farther one — "closer `.gitignore` wins", same as
/// real `git`. A matcher with no opinion (`Match::None`) never changes the running verdict.
fn ancestor_ignored(entry: &ignore::DirEntry, ancestors: &[Gitignore]) -> bool {
    if ancestors.is_empty() {
        return false;
    }
    let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
    let path = entry.path();
    let mut ignored = false;
    for gi in ancestors {
        match gi.matched(path, is_dir) {
            ignore::Match::Ignore(_) => ignored = true,
            ignore::Match::Whitelist(_) => ignored = false,
            ignore::Match::None => {}
        }
    }
    ignored
}

/// `path` relative to `root`, joined with forward slashes regardless of host OS separator — every
/// downstream consumer expects POSIX-style rel paths.
fn to_rel(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Processes one file end to end: read -> cache lookup -> (size-cap / dispatch / parse) -> per-file DSL
/// rules -> artifact. Never panics outward: an unreadable file, oversized file, or parser panic all
/// degrade the artifact instead of propagating.
///
/// Cache flow: content-hash `bytes` -> `get_ir`. IR miss -> full parse via `compute_fresh_artifact`,
/// then `put_ir` + `put_findings`. IR hit + findings hit -> full skip, no reparse. IR hit but findings
/// miss (ruleset-only change) -> reuse the cached `FileIrSlice`, re-run `eval_packs`, `put_findings`.
fn process_file(
    rel: &str,
    abs: &Path,
    config: &EngineConfig,
    packs: &[&RulePackDef],
    cache: Option<&AnalysisCache>,
    ruleset_fingerprint: Option<&str>,
    counters: Option<&CacheCounters>,
) -> FileArtifact {
    let bytes = match fs::read(abs) {
        Ok(b) => b,
        Err(_) => {
            // Unreadable (permission error, or a race with a concurrent delete) — never a panic, just a
            // degraded empty artifact. No cache lookup: there's no content to hash.
            return FileArtifact {
                rel: rel.to_string(),
                symbols: Vec::new(),
                imports: None,
                loc: 0,
                findings: Vec::new(),
                degraded: true,
                minified_or_generated: false,
                io: None,
                rule_timings: Vec::new(),
                used_names: Vec::new(),
                const_map_fragment: std::collections::HashMap::new(),
                trpc_router_fragments: Vec::new(),
                router_mount_fragments: Vec::new(),
                wrapper_def_fragments: Vec::new(),
                wrapper_call_fragments: Vec::new(),
            };
        }
    };

    let language = dispatch::dispatch(rel, &config.dispatch);

    let cache_key = match (cache, ruleset_fingerprint) {
        (Some(_), Some(rsfp)) => Some(CacheKey {
            content_hash: AnalysisCache::content_hash(&bytes),
            parser_fingerprint: crate::cache::parser_fingerprint(language, config),
            // Without `scope`, two different files with byte-identical content could alias each
            // other's cached IR/findings (which embed their own `file` path).
            scope: crate::cache::cache_scope(config, rel),
            ruleset_fingerprint: rsfp.to_string(),
        }),
        _ => None,
    };

    if let (Some(cache), Some(key)) = (cache, cache_key.as_ref()) {
        if let Some(ir) = cache.get_ir(key) {
            if let Some(findings) = cache.get_findings(key) {
                if let Some(c) = counters {
                    c.record_hit();
                }
                // Full cache hit: no rule evaluation ran this call, so nothing to time.
                return artifact_from_ir(rel, ir, findings, Vec::new());
            }
            // IR hit, findings miss: reuse the parsed IR, re-run rules only.
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let (mut findings, rule_timings, _minified) = eval_packs(
                packs,
                rel,
                &text,
                &ir.symbols,
                ir.io.clone(),
                config.profile_rules,
            );
            if schema_findings_eligible(language, ir.degraded) {
                findings.extend(schema_findings(&config.rule_config, rel, &text));
            }
            let _ = cache.put_findings(key, &findings);
            if let Some(c) = counters {
                c.record_miss();
            }
            return artifact_from_ir(rel, ir, findings, rule_timings);
        }
    }
    if cache_key.is_some() {
        if let Some(c) = counters {
            c.record_miss();
        }
    }

    let text = String::from_utf8_lossy(&bytes).into_owned();
    let artifact = compute_fresh_artifact(rel, &bytes, &text, language, config, packs);

    if let (Some(cache), Some(key)) = (cache, cache_key.as_ref()) {
        let ir_slice = FileIrSlice {
            symbols: artifact.symbols.clone(),
            imports: artifact.imports.clone(),
            loc: artifact.loc,
            degraded: artifact.degraded,
            io: artifact.io.clone(),
            used_names: artifact.used_names.clone(),
            minified_or_generated: artifact.minified_or_generated,
            const_map_fragment: artifact.const_map_fragment.clone(),
            trpc_router_fragments: artifact.trpc_router_fragments.clone(),
            router_mount_fragments: artifact.router_mount_fragments.clone(),
            wrapper_def_fragments: artifact.wrapper_def_fragments.clone(),
            wrapper_call_fragments: artifact.wrapper_call_fragments.clone(),
        };
        let _ = cache.put_ir(key, &ir_slice);
        let _ = cache.put_findings(key, &artifact.findings);
    }

    artifact
}

/// Rebuilds a `FileArtifact` from a cached `FileIrSlice` + its (possibly just-recomputed) findings —
/// `rel` is the only piece `FileIrSlice` doesn't carry (not part of the cached payload; the lookup path
/// already knows it). `rule_timings` is empty on a full cache hit.
fn artifact_from_ir(
    rel: &str,
    ir: FileIrSlice,
    findings: Vec<zzop_core::Finding>,
    rule_timings: Vec<RuleTiming>,
) -> FileArtifact {
    FileArtifact {
        rel: rel.to_string(),
        symbols: ir.symbols,
        imports: ir.imports,
        loc: ir.loc,
        findings,
        degraded: ir.degraded,
        minified_or_generated: ir.minified_or_generated,
        io: ir.io,
        rule_timings,
        used_names: ir.used_names,
        const_map_fragment: ir.const_map_fragment,
        trpc_router_fragments: ir.trpc_router_fragments,
        router_mount_fragments: ir.router_mount_fragments,
        wrapper_def_fragments: ir.wrapper_def_fragments,
        wrapper_call_fragments: ir.wrapper_call_fragments,
    }
}

/// The "no cache entry available" path: size-cap / dispatch / parse / IO projection / per-file DSL
/// rules — shared by the cache-miss path and (via `cache: None`) the cache-off path.
fn compute_fresh_artifact(
    rel: &str,
    bytes: &[u8],
    text: &str,
    language: Option<Language>,
    config: &EngineConfig,
    packs: &[&RulePackDef],
) -> FileArtifact {
    if bytes.len() > config.size_cap {
        // Oversized: loc counted lexically, no symbols/imports/io, but the text is still scanned by
        // line-scan DSL rules (lexical-only files are excluded from structural projection, not rule
        // evaluation).
        let loc = lexical_loc(text);
        let (findings, rule_timings, minified_or_generated) =
            eval_packs(packs, rel, text, &[], None, config.profile_rules);
        return FileArtifact {
            rel: rel.to_string(),
            symbols: Vec::new(),
            imports: ts_slot(language),
            loc,
            findings,
            degraded: true,
            minified_or_generated,
            io: None,
            rule_timings,
            used_names: Vec::new(),
            const_map_fragment: std::collections::HashMap::new(),
            trpc_router_fragments: Vec::new(),
            router_mount_fragments: Vec::new(),
            wrapper_def_fragments: Vec::new(),
            wrapper_call_fragments: Vec::new(),
        };
    }

    let (symbols, imports, loc, degraded, used_names) = match language {
        Some(Language::TypeScript) => parse_typescript(rel, text),
        Some(Language::Prisma) => {
            let (symbols, imports, loc, degraded) = parse_prisma(&config.source_id, rel, text);
            (symbols, imports, loc, degraded, Vec::new())
        }
        Some(Language::JavaLexical) => {
            let (symbols, imports, loc, degraded) = parse_java_lexical(rel, text);
            (symbols, imports, loc, degraded, Vec::new())
        }
        None => (Vec::new(), None, lexical_loc(text), false, Vec::new()),
    };
    // IO projection: TypeScript (HTTP egress consumes + Hono route provides) for a well-formed,
    // in-size-cap file; Java (Spring MVC route provides only, never `degraded`) for any `.java` file. A
    // degraded/oversized/dispatch-`None` file has no adapter to run.
    let io = match language {
        Some(Language::TypeScript) if !degraded => {
            crate::io::extract_file_io(rel, text, &config.io)
        }
        Some(Language::JavaLexical) => crate::io::extract_java_file_io(rel, text),
        _ => None,
    };
    // The next four projections are all TypeScript-only, reusing `text` already in hand (an extra parse
    // of already-read text, not a second file read): const-map fragment (feeds `analyze::assemble`'s
    // merge + late consume re-resolution), tRPC router fragment (`analyze::compose_trpc_provides`),
    // router-mount fragment (Hono chained builders / cross-file mounts, for
    // `analyze::compose_router_mount_provides`), and wrapper def/call fragments (`analyze`'s
    // assemble-time wrapper-consume join, defs indexed by `(file, name)`).
    let const_map_fragment = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::const_map_fragment(rel, text)
        }
        _ => std::collections::HashMap::new(),
    };
    let trpc_router_fragments = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_trpc_router_fragments(rel, text)
        }
        _ => Vec::new(),
    };
    let router_mount_fragments = match language {
        Some(Language::TypeScript) if !degraded => {
            let router_names: Vec<&str> =
                config.io.router_names.iter().map(String::as_str).collect();
            zzop_parser_typescript::extract_router_mount_fragments(rel, text, &router_names)
        }
        _ => Vec::new(),
    };
    let (wrapper_def_fragments, wrapper_call_fragments) = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_wrapper_fragments(rel, text)
        }
        _ => (Vec::new(), Vec::new()),
    };
    let (mut findings, rule_timings, minified_or_generated) =
        eval_packs(packs, rel, text, &symbols, io.clone(), config.profile_rules);
    if schema_findings_eligible(language, degraded) {
        findings.extend(schema_findings(&config.rule_config, rel, text));
    }
    FileArtifact {
        rel: rel.to_string(),
        symbols,
        imports,
        loc,
        findings,
        degraded,
        minified_or_generated,
        io,
        rule_timings,
        used_names,
        const_map_fragment,
        trpc_router_fragments,
        router_mount_fragments,
        wrapper_def_fragments,
        wrapper_call_fragments,
    }
}

/// `Some(empty map)` for a TypeScript-dispatched file (gives it a dep-graph node even when parsing was
/// skipped/degraded), `None` otherwise.
fn ts_slot(language: Option<Language>) -> Option<ImportMap> {
    matches!(language, Some(Language::TypeScript)).then(ImportMap::new)
}

/// Non-blank, non-comment line count computed from raw text alone (no parse) — used for oversized
/// files, lexical-only files, and the fallback when a parse panics. Approximate for Prisma text
/// (also uses `//` comments), acceptable for a fallback-only path.
fn lexical_loc(text: &str) -> u32 {
    zzop_parser_typescript::count_loc(text)
}

/// TypeScript parse: symbols + imports + loc, or a degraded lexical fallback.
///
/// `parse_symbols`/`parse_imports` fold "swc couldn't parse this" and "legitimately empty file" into
/// the same empty result, so the broken/empty distinction instead comes from
/// `zzop_parser_typescript::parse_ok`: `false` means swc produced no `Module` at all — route straight to
/// the lexical fallback; `true` proceeds to `parse_symbols`/`parse_imports`, still `catch_unwind`-wrapped
/// as defense in depth.
///
/// Also computes `used_names` (`parse_local_identifier_refs`) for `dead-exports`. Known cost: each of
/// the three extraction calls parses independently, so a well-formed file is parsed by swc three times
/// per pass (four counting `parse_ok`'s probe) — `zzop_cache::FileIrSlice::used_names` caches the result
/// so a warm run pays this only once per distinct file content.
fn parse_typescript(
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    if !zzop_parser_typescript::parse_ok(rel, text) {
        return (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        );
    }
    let result = std::panic::catch_unwind(|| {
        let symbols = zzop_parser_typescript::parse_symbols(rel, text);
        let imports = zzop_parser_typescript::parse_imports(rel, text);
        let loc = zzop_parser_typescript::count_loc(text);
        let used_names: Vec<String> =
            zzop_parser_typescript::parse_local_identifier_refs(rel, text)
                .into_iter()
                .collect();
        (symbols, imports, loc, used_names)
    });
    match result {
        Ok((symbols, imports, loc, used_names)) => (symbols, Some(imports), loc, false, used_names),
        Err(_) => (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        ),
    }
}

/// Prisma parse: reuses `zzop_parser_prisma::build_common_ir` with a single-file slice. Its parser is a
/// line-based regex scanner with no AST step, so a malformed schema degrades to "zero models found"
/// rather than panicking; `catch_unwind` is still applied as defense in depth. Prisma files never
/// participate in the TS dep graph (`imports: None`, always).
fn parse_prisma(
    source_id: &str,
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool) {
    let owned = (rel.to_string(), text.to_string());
    let result = std::panic::catch_unwind(|| {
        zzop_parser_prisma::build_common_ir(source_id, std::slice::from_ref(&owned))
    });
    match result {
        Ok(ir) => {
            let loc = ir
                .ir
                .loc
                .get(rel)
                .copied()
                .unwrap_or_else(|| lexical_loc(text));
            (ir.ir.symbols, None, loc, false)
        }
        Err(_) => (Vec::new(), None, lexical_loc(text), true),
    }
}

/// Java parse: the lexical brace-matcher `zzop_parser_java::parse_method_spans`, `catch_unwind`-wrapped
/// as defense in depth. Normally never `degraded` — a malformed `.java` file just yields fewer/odd
/// spans, not a parse failure (the `Err` arm only fires on an actual panic). Never participates in the
/// TS dep graph (`imports: None`, always).
fn parse_java_lexical(rel: &str, text: &str) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool) {
    let owned = (rel.to_string(), text.to_string());
    let result =
        std::panic::catch_unwind(|| zzop_parser_java::parse_method_spans(&owned.0, &owned.1));
    match result {
        Ok(symbols) => (symbols, None, lexical_loc(text), false),
        Err(_) => (Vec::new(), None, lexical_loc(text), true),
    }
}

/// Runs every applicable DSL pack against this one file's slice. `packs` is already
/// `is_enabled`-filtered by `run_file_pass`; `pack_loader::applies_to` is the remaining per-file,
/// per-pack pre-filter. Short-circuits before iterating `packs` when the text is minified/generated
/// (skips all matcher types, not only line-scan); the returned bool lets callers set
/// `FileArtifact::minified_or_generated` without recomputing the check.
///
/// `profile` mirrors `EngineConfig::profile_rules`: `false` calls `eval_pack` (no timing overhead);
/// `true` calls `eval_pack_profiled` and concatenates every pack's `RuleTiming`s, summed later across
/// every artifact by `analyze::assemble`.
fn eval_packs(
    packs: &[&RulePackDef],
    rel: &str,
    text: &str,
    symbols: &[SourceSymbol],
    io: Option<IoFacts>,
    profile: bool,
) -> (Vec<zzop_core::Finding>, Vec<RuleTiming>, bool) {
    if zzop_core::dsl::is_minified_or_generated(text) {
        return (Vec::new(), Vec::new(), true);
    }
    let file = SourceFile {
        rel: rel.to_string(),
        text: text.to_string(),
        symbols: symbols.to_vec(),
        io,
    };
    let files = std::slice::from_ref(&file);
    let ctx = RuleContext { files, ir: None };
    let mut out = Vec::new();
    let mut timings = Vec::new();
    for pack in packs {
        if pack_loader::applies_to(pack, rel) {
            if profile {
                let (findings, t) = eval_pack_profiled(pack, &ctx);
                out.extend(findings);
                timings.extend(t);
            } else {
                out.extend(eval_pack(pack, &ctx));
            }
        }
    }
    (out, timings, false)
}

/// Whether a Prisma file's schema-structural rules (`schema_findings`) should run — shared by
/// `compute_fresh_artifact` and `process_file`'s cache-reuse branch so re-enabling `schema-structural`
/// on a warm run doesn't silently drop findings for already-cached files. Only Prisma, non-degraded.
fn schema_findings_eligible(language: Option<Language>, degraded: bool) -> bool {
    matches!(language, Some(Language::Prisma)) && !degraded
}

/// Wires `zzop_rules_schema::apply_schema_rules` into the fused per-file pass for Prisma files:
/// re-parses this file's `SchemaModel`s (cheap — same scan `parse_prisma` already ran) and converts
/// each `SchemaIssue` into a `zzop_core::Finding`, gated behind native id `"schema-structural"`.
/// `rule_id` is `"schema/{issue.rule}"`, a fresh namespace since this is native logic, not a DSL pack.
fn schema_findings(
    rule_config: &zzop_core::RuleConfig,
    rel: &str,
    text: &str,
) -> Vec<zzop_core::Finding> {
    if !registry::is_enabled(rule_config, "schema-structural") {
        return Vec::new();
    }
    let models = zzop_parser_prisma::parse_schema(text, Some(rel), None);
    zzop_rules_schema::apply_schema_rules(&models)
        .iter()
        .map(|issue| schema_issue_to_finding(rel, text, issue))
        .collect()
}

/// The usage counterpart of `schema_findings`: wires the usage cross-check (dead-model / dead-field /
/// schema-churn) via `zzop_rules_schema::cross_check_schema`/`apply_churn_rule`. Unlike
/// `schema_findings` this is a whole-tree pass — usage evidence (store bindings, identifier counts,
/// migration churn) spans every source file, so it runs from `analyze::assemble`'s global stage and is
/// recomputed each run, never entering the per-file findings cache. `analyze_schema_with_usage` is
/// deliberately not used here since it re-runs the structural rules the per-file pass already emitted.
///
/// `scan_store_map` needs store-factory/client-getter vocabulary; this engine has no general vocabulary
/// config, so `zzop_parser_prisma`'s defaults apply. Degraded `.prisma` files are excluded by the
/// caller; unreadable files are skipped.
///
/// Scope asymmetry (intentional): `prisma_rels` honors the engine's dispatch/skip config, but the
/// three usage collectors walk `root/src` themselves with only their own skips — an engine-excluded
/// source file still contributes identifier counts and store bindings.
pub(crate) fn schema_usage_findings(
    root: &Path,
    prisma_rels: &[String],
) -> Vec<zzop_core::Finding> {
    if prisma_rels.is_empty() {
        return Vec::new();
    }
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut models: Vec<zzop_core::SchemaModel> = Vec::new();
    for rel in prisma_rels {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        models.extend(zzop_parser_prisma::parse_schema(&text, Some(rel), None));
        texts.push((rel.clone(), text));
    }
    if models.is_empty() {
        return Vec::new();
    }
    let churn = zzop_rules_schema::scan_migration_churn(root, &models);
    let usage = zzop_core::SchemaUsage {
        bound_models: zzop_rules_schema::scan_store_map(
            root,
            zzop_parser_prisma::DEFAULT_STORE_FACTORY_FN,
            zzop_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN,
        )
        .into_values()
        .collect(),
        identifier_counts: zzop_rules_schema::scan_field_usage(root),
        model_churn: None, // `apply_churn_rule` is called explicitly below instead
    };
    let mut issues = zzop_rules_schema::cross_check_schema(&models, &usage);
    issues.extend(zzop_rules_schema::apply_churn_rule(&models, &churn));
    issues
        .iter()
        .map(|issue| {
            // A usage issue names its model; `source_path` (stamped by `parse_schema` above) picks the
            // file whose text anchors the finding line. Known limit: if two .prisma files declare the
            // same model name, both issues anchor on the first declaration.
            let rel = models
                .iter()
                .find(|m| m.name == issue.model)
                .and_then(|m| m.source_path.as_deref())
                .unwrap_or_else(|| texts[0].0.as_str());
            let text = texts
                .iter()
                .find(|(r, _)| r == rel)
                .map(|(_, t)| t.as_str())
                .unwrap_or_default();
            schema_issue_to_finding(rel, text, issue)
        })
        .collect()
}

/// One `SchemaIssue` -> one `Finding`. `line` uses `zzop_parser_prisma::model_decl_line` since
/// `SchemaIssue` carries no line number of its own (only `model`/`field` names). `data` embeds the
/// full `SchemaIssue` so a structured consumer can recover `field`/`params` without re-parsing
/// `message`.
///
/// This glue stays in this engine rather than `zzop-rules-schema`: it needs
/// `zzop_parser_prisma::model_decl_line`, and `zzop-rules-schema` deliberately does not depend on
/// `zzop-parser-prisma` (the dependency runs the other way) — this engine depends on both.
fn schema_issue_to_finding(
    rel: &str,
    text: &str,
    issue: &zzop_rules_schema::SchemaIssue,
) -> zzop_core::Finding {
    zzop_core::Finding {
        rule_id: format!("schema/{}", issue.rule),
        severity: issue.severity,
        file: rel.to_string(),
        line: zzop_parser_prisma::model_decl_line(text, &issue.model),
        message: zzop_rules_schema::schema_issue_message(issue),
        data: serde_json::to_value(issue).ok(),
    }
}

// The schema x usage JOIN native rules (`soft-delete-bypass` / `orderby-unindexed`) are wired in
// `analyze::run_schema_join_rules`, beside `schema-usage`/`duplicate-route` — the canonical whole-tree
// native-rule call site.

/// Filename pattern matching a `package.json` at any depth — a monorepo has one per package (see
/// `package_json_entries`'s own doc).
fn is_package_json_path(rel: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(^|/)package\.json$").unwrap())
        .is_match(rel)
}

/// True for a whitespace-delimited token that looks like a relative source-file path (matched against
/// the whole token, never a mid-token substring) — deliberately conservative, preferring to miss an
/// obscure script invocation over treating an unrelated flag/argument as a path.
fn looks_like_script_path_token(tok: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\S+\.(?:ts|js|mjs|cjs)$").unwrap())
        .is_match(tok)
}

/// POSIX dirname of a rel path, `package_json_entries`-flavored: `""` (not `resolve::dirname`'s `"."`) for
/// a root-level `package.json`, so it can feed `join_and_normalize` below as the join-identity element
/// without an accidental `"./"` hop.
fn package_json_dir(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    }
}

/// POSIX join + `.`/`..`-segment normalize — a small local reimplementation of
/// `zzop_parser_typescript::resolve`'s private `normalize`/dirname-join logic, sized to exactly what
/// `package_json_entries` needs (that module's helpers are private, not importable from here).
fn join_and_normalize(dir: &str, candidate: &str) -> String {
    let joined = if dir.is_empty() {
        candidate.to_string()
    } else {
        format!("{dir}/{candidate}")
    };
    let mut stack: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                if matches!(stack.last(), Some(&s) if s != "..") {
                    stack.pop();
                } else {
                    stack.push("..");
                }
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

/// Recursively collects every string leaf of `v` that looks like a relative path (`./`/`../`-prefixed)
/// — the `exports` field walker: handles a single string, a conditional map, a subpath map, and
/// arbitrary nesting of the two. Only string values are collected, never object keys (subpath/condition
/// names), and the prefix filter excludes non-path values like a bare package specifier.
fn collect_export_path_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => {
            if s.starts_with("./") || s.starts_with("../") {
                out.push(s.clone());
            }
        }
        serde_json::Value::Object(map) => {
            for val in map.values() {
                collect_export_path_strings(val, out);
            }
        }
        _ => {}
    }
}

/// `package_json_entries`' return: `extra_entries` plus `workspace_pkgs`, a `name -> WorkspacePkg` map
/// from the same manifest walk. The workspace-alias import resolver needs a directory to resolve
/// `<name>/subpath` specifiers and a resolved entry file to resolve a bare `<name>` specifier.
pub(crate) struct PackageJsonScan {
    pub extra_entries: std::collections::HashSet<String>,
    pub workspace_pkgs: std::collections::HashMap<String, zzop_parser_typescript::WorkspacePkg>,
}

/// The `exports` field's own `"."` (package-root) entry — unlike `collect_export_path_strings` (which
/// gathers every leaf including named sub-paths), a workspace bare-specifier import resolves only via
/// the `"."` condition (or `exports` being a bare string/condition-map, Node's shorthand for `{".":
/// ...}`). An `exports` map keyed entirely by sub-paths has no root entry; this conservatively falls
/// back to treating the whole object as a condition-map in that case.
fn collect_exports_dot_entry(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(_) => collect_export_path_strings(v, out),
        serde_json::Value::Object(map) => match map.get(".") {
            Some(dot) => collect_export_path_strings(dot, out),
            None => collect_export_path_strings(v, out),
        },
        _ => {}
    }
}

/// Collects file paths referenced by any `package.json` found during the walk that should be treated as
/// entry-like regardless of `fan_in` (`find_dead_candidates`'s `extra_entries`): manifest entry fields
/// (`main`/`module`/`bin`/`exports`) and lexically-scanned `scripts` path tokens. `all_paths` is the
/// TS-dispatched universe used to resolve an extensionless/compiled manifest value via
/// `zzop_parser_typescript::try_ext`.
///
/// Also collects each manifest's `name` into `PackageJsonScan::workspace_pkgs` (own directory, plus a
/// resolved bare-specifier entry tried in Node's own order: `main`, `module`, `exports["."]`, then a
/// conventional `index.*` file; `entry` stays `None` when nothing resolves) — same loop/read, not a
/// second walk.
///
/// Degrades gracefully on every failure mode — never panics. Each manifest's candidates resolve
/// relative to its own directory, not `root`; an unresolvable candidate is simply dropped.
pub(crate) fn package_json_entries(
    root: &Path,
    node_paths: impl Iterator<Item = String>,
    all_paths: &std::collections::HashSet<String>,
) -> PackageJsonScan {
    let mut result = std::collections::HashSet::new();
    let mut workspace_pkgs = std::collections::HashMap::new();
    for rel in node_paths.filter(|p| is_package_json_path(p)) {
        let Ok(text) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let dir = package_json_dir(&rel);
        let mut candidates: Vec<String> = Vec::new();
        for key in ["main", "module"] {
            if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
                candidates.push(s.to_string());
            }
        }
        match value.get("bin") {
            Some(serde_json::Value::String(s)) => candidates.push(s.clone()),
            Some(serde_json::Value::Object(map)) => {
                for v in map.values() {
                    if let Some(s) = v.as_str() {
                        candidates.push(s.to_string());
                    }
                }
            }
            _ => {}
        }
        if let Some(exports) = value.get("exports") {
            collect_export_path_strings(exports, &mut candidates);
        }
        if let Some(serde_json::Value::Object(scripts)) = value.get("scripts") {
            for cmd in scripts.values().filter_map(|v| v.as_str()) {
                for tok in cmd.split_whitespace() {
                    if looks_like_script_path_token(tok) {
                        candidates.push(tok.to_string());
                    }
                }
            }
        }
        for candidate in &candidates {
            let normalized = join_and_normalize(dir, candidate);
            if let Some(resolved) = zzop_parser_typescript::try_ext(&normalized, all_paths) {
                result.insert(resolved);
            }
        }

        if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
            let mut entry_candidates: Vec<String> = Vec::new();
            for key in ["main", "module"] {
                if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
                    entry_candidates.push(s.to_string());
                }
            }
            if let Some(exports) = value.get("exports") {
                collect_exports_dot_entry(exports, &mut entry_candidates);
            }
            for fallback in ["index.ts", "index.tsx", "src/index.ts", "src/index.tsx"] {
                entry_candidates.push(fallback.to_string());
            }
            let entry = entry_candidates.iter().find_map(|candidate| {
                let normalized = join_and_normalize(dir, candidate);
                zzop_parser_typescript::try_ext(&normalized, all_paths)
            });
            workspace_pkgs.insert(
                name.to_string(),
                zzop_parser_typescript::WorkspacePkg {
                    dir: dir.to_string(),
                    entry,
                },
            );
        }
    }
    PackageJsonScan {
        extra_entries: result,
        workspace_pkgs,
    }
}

// --- tsconfig `paths`/`baseUrl` alias collection ---
//
// `tsconfig_scan` is this engine's filesystem-touching collection pass; the pure resolver logic it
// feeds lives in `zzop_parser_typescript::resolve` instead (no I/O there).

/// Filename pattern matching a `tsconfig.json` at any depth — only this literal name is auto-discovered
/// (mirrors real `tsc` project discovery); an `extends` target is read only when referenced.
fn is_tsconfig_json_path(rel: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(^|/)tsconfig\.json$").unwrap())
        .is_match(rel)
}

/// One tsconfig file's own (unmerged, un-`extends`-resolved) `compilerOptions.baseUrl`/`paths` +
/// `extends` target, as written — `tsconfig_scan` resolves/merges `extends` and joins `baseUrl`
/// against the file's own directory.
struct RawTsconfig {
    base_url: Option<String>,
    paths: std::collections::BTreeMap<String, Vec<String>>,
    extends: Option<String>,
}

/// Strips `//` line comments and `/* ... */` block comments from `input`, respecting string literals
/// (a comment marker inside a JSON string is left alone). tsconfig.json commonly ships JSONC, which
/// `serde_json` rejects outright; this plus the trailing-comma strip in `parse_raw_tsconfig` is a small
/// tolerant preprocessor sized to real-world tsconfigs, not a general JSONC/JSON5 parser.
fn strip_jsonc_comments(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Parses one tsconfig file's text (JSONC-tolerant) into its own `compilerOptions.baseUrl`/`paths`/
/// `extends`, un-merged. `None` on any parse failure — `tsconfig_scan` degrades by skipping that file.
fn parse_raw_tsconfig(text: &str) -> Option<RawTsconfig> {
    static TRAILING_COMMA: OnceLock<Regex> = OnceLock::new();
    let stripped = strip_jsonc_comments(text);
    let cleaned = TRAILING_COMMA
        .get_or_init(|| Regex::new(r",(\s*[}\]])").unwrap())
        .replace_all(&stripped, "$1");
    let value: serde_json::Value = serde_json::from_str(&cleaned).ok()?;
    let extends = value
        .get("extends")
        .and_then(|v| v.as_str())
        .map(String::from);
    let compiler_options = value.get("compilerOptions");
    let base_url = compiler_options
        .and_then(|c| c.get("baseUrl"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let mut paths = std::collections::BTreeMap::new();
    if let Some(map) = compiler_options
        .and_then(|c| c.get("paths"))
        .and_then(|v| v.as_object())
    {
        for (pattern, targets) in map {
            if let Some(arr) = targets.as_array() {
                let targets: Vec<String> = arr
                    .iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect();
                if !targets.is_empty() {
                    paths.insert(pattern.clone(), targets);
                }
            }
        }
    }
    Some(RawTsconfig {
        base_url,
        paths,
        extends,
    })
}

/// Collects `compilerOptions.baseUrl`/`paths` from every `tsconfig.json` found during the same manifest
/// walk `package_json_entries` uses, keyed by the tsconfig's own directory (the directory a TypeScript
/// file's nearest ancestor tsconfig governs, per `zzop_parser_typescript::resolve::governing_tsconfig`).
///
/// `extends` handling is minimal: only a local relative target is followed, exactly one level, merged
/// parent-fills-gaps (child's `paths` keys win; `baseUrl` is the child's if set, else the parent's). A
/// second-level or non-local `extends` is left unresolved.
///
/// A directory whose merged tsconfig declares neither `baseUrl` nor `paths` is not registered, so
/// `governing_tsconfig`'s ancestor walk continues past it. Degrades gracefully on every failure mode —
/// never panics.
pub(crate) fn tsconfig_scan(
    root: &Path,
    node_paths: impl Iterator<Item = String>,
) -> std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths> {
    let mut result = std::collections::BTreeMap::new();
    for rel in node_paths.filter(|p| is_tsconfig_json_path(p)) {
        let Ok(text) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        let Some(raw) = parse_raw_tsconfig(&text) else {
            continue;
        };
        let dir = package_json_dir(&rel);

        let mut paths = raw.paths;
        let mut base_url_raw = raw.base_url;
        if let Some(extends) = &raw.extends {
            if extends.starts_with("./") || extends.starts_with("../") {
                let mut parent_rel = join_and_normalize(dir, extends);
                if !parent_rel.ends_with(".json") {
                    parent_rel.push_str(".json");
                }
                if let Ok(parent_text) = fs::read_to_string(root.join(&parent_rel)) {
                    if let Some(parent_raw) = parse_raw_tsconfig(&parent_text) {
                        // Child keys win; any key only the parent declares is kept (parent-fills-gaps).
                        // `parent_raw.extends` (a 2nd extends level) is intentionally not chased further.
                        let mut merged = parent_raw.paths;
                        merged.extend(paths);
                        paths = merged;
                        if base_url_raw.is_none() {
                            base_url_raw = parent_raw.base_url;
                        }
                    }
                }
            }
        }

        if paths.is_empty() && base_url_raw.is_none() {
            continue;
        }
        let base_url = match &base_url_raw {
            Some(b) => join_and_normalize(dir, b),
            None => dir.to_string(),
        };
        result.insert(
            dir.to_string(),
            zzop_parser_typescript::TsconfigPaths { base_url, paths },
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_typescript's real parse-failure signal (zzop_parser_typescript::parse_ok) ---

    #[test]
    fn parse_typescript_does_not_degrade_balanced_well_formed_file() {
        let (symbols, _imports, _loc, degraded, _used_names) = parse_typescript(
            "x.ts",
            "export function foo(a: { x: number }) {\n  return [a.x, (a.x + 1)];\n}\n",
        );
        assert!(!degraded);
        assert_eq!(symbols.len(), 1);
    }

    #[test]
    fn parse_typescript_degrades_unbalanced_brace() {
        let (symbols, imports, loc, degraded, used_names) =
            parse_typescript("b.ts", "function foo( {\n  return 1;\n");
        assert!(degraded);
        assert!(symbols.is_empty());
        assert_eq!(imports, Some(ImportMap::new()));
        assert!(loc > 0); // lexical fallback still counts loc
        assert!(used_names.is_empty());
    }

    #[test]
    fn parse_typescript_degrades_stray_closing_brace() {
        let (_symbols, _imports, _loc, degraded, _used_names) =
            parse_typescript("s.ts", "}\nfunction foo() {}\n");
        assert!(degraded);
    }

    #[test]
    fn parse_typescript_does_not_degrade_braces_inside_strings_and_comments() {
        let (_symbols, _imports, _loc, degraded, _used_names) = parse_typescript(
            "c.ts",
            "const s = \"{ unmatched\"; // } also unmatched\nfunction f() {}\n",
        );
        assert!(!degraded);
    }

    #[test]
    fn parse_typescript_degrades_a_balanced_but_syntactically_invalid_file() {
        // Braces/parens are balanced (there are none), but `const x: = 1;` is not valid TypeScript —
        // a brace-balance-only check would misclassify this as a legitimately empty file.
        let (symbols, imports, _loc, degraded, _used_names) =
            parse_typescript("t.ts", "const x: = 1;\n");
        assert!(degraded);
        assert!(symbols.is_empty());
        assert_eq!(imports, Some(ImportMap::new()));
    }

    #[test]
    fn parse_typescript_does_not_degrade_a_legitimately_empty_file() {
        let (symbols, imports, _loc, degraded, used_names) = parse_typescript("e.ts", "");
        assert!(!degraded);
        assert!(symbols.is_empty());
        assert!(imports.unwrap().is_empty());
        assert!(used_names.is_empty());
    }

    #[test]
    fn parse_typescript_collects_used_names_alongside_symbols() {
        let (_symbols, _imports, _loc, degraded, used_names) = parse_typescript(
            "x.ts",
            "const X = 1;\nfunction foo() { return X + Y; }\nexport { foo };\n",
        );
        assert!(!degraded);
        assert!(used_names.contains(&"X".to_string()));
        assert!(used_names.contains(&"Y".to_string()));
        // `foo`'s own declaration name is excluded — matches `parse_local_identifier_refs`'s contract.
        assert!(!used_names.contains(&"foo".to_string()));
    }

    // --- what the parser actually does on garbage input ---

    #[test]
    fn garbage_ts_input_does_not_panic_parse_symbols_or_parse_imports() {
        // Calls the parser functions directly (the real pipeline would already have degraded this file
        // via `parse_ok` before reaching these calls).
        let garbage = "@#$%^&*( ) => => => 123abc <<< >>> \u{0}\u{1}";
        let symbols = zzop_parser_typescript::parse_symbols("g.ts", garbage);
        let imports = zzop_parser_typescript::parse_imports("g.ts", garbage);
        assert!(symbols.is_empty());
        assert!(imports.is_empty());
    }

    #[test]
    fn parse_typescript_degrades_garbage_input() {
        let garbage = "@#$%^&*( ) => => => 123abc <<< >>> \u{0}\u{1}";
        let (symbols, imports, loc, degraded, used_names) = parse_typescript("g.ts", garbage);
        assert!(degraded);
        assert!(symbols.is_empty());
        assert_eq!(imports, Some(ImportMap::new()));
        assert!(loc > 0);
        assert!(used_names.is_empty());
    }

    #[test]
    fn parse_typescript_succeeds_on_well_formed_file() {
        let (symbols, imports, _loc, degraded, _used_names) =
            parse_typescript("ok.ts", "export function foo() { return 1; }\n");
        assert!(!degraded);
        assert_eq!(symbols.len(), 1);
        assert!(imports.unwrap().is_empty());
    }

    // --- package_json_entries ---

    use std::collections::HashSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A self-cleaning temp directory (std-only mkdtemp equivalent — no `tempfile` crate dependency in
    /// this workspace; mirrors `rules/native/rules-schema/src/usage.rs`'s own test-local `TempDir`).
    /// Created fresh per test and removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, rel: &str, content: &str) {
            let full = self.0.join(rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn package_json_entries_resolves_extensionless_or_js_main_via_try_ext() {
        let dir = TempDir::new("zzop-pkg-entries-main");
        dir.write("package.json", r#"{"main": "dist/index.js"}"#);
        let all_paths: HashSet<String> = ["dist/index.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_resolves_bin_object_with_multiple_entries() {
        let dir = TempDir::new("zzop-pkg-entries-bin");
        dir.write(
            "package.json",
            r#"{"bin": {"foo-cli": "./bin/foo.ts", "bar-cli": "./bin/bar.ts"}}"#,
        );
        let all_paths: HashSet<String> = ["bin/foo.ts".to_string(), "bin/bar.ts".to_string()]
            .into_iter()
            .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_resolves_nested_exports_and_ignores_condition_keys() {
        let dir = TempDir::new("zzop-pkg-entries-exports");
        dir.write(
            "package.json",
            r#"{
                "exports": {
                    ".": { "import": "./src/index.mts", "require": "./src/index.cts" },
                    "./sub": "./src/sub.ts"
                }
            }"#,
        );
        let all_paths: HashSet<String> = [
            "src/index.mts".to_string(),
            "src/index.cts".to_string(),
            "src/sub.ts".to_string(),
        ]
        .into_iter()
        .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_lexically_scans_scripts_for_path_tokens() {
        let dir = TempDir::new("zzop-pkg-entries-scripts");
        dir.write(
            "package.json",
            r#"{
                "scripts": {
                    "build": "tsc && node scripts/postbuild.js",
                    "test": "jest"
                }
            }"#,
        );
        let all_paths: HashSet<String> = ["scripts/postbuild.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        // "test": "jest" has no path-looking token — contributes nothing; "tsc" isn't a path either.
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_resolves_relative_to_own_directory_not_root() {
        let dir = TempDir::new("zzop-pkg-entries-nested");
        dir.write("packages/foo/package.json", r#"{"main": "./index.ts"}"#);
        let all_paths: HashSet<String> =
            ["packages/foo/index.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/foo/package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    // --- PackageJsonScan::workspace_pkgs ---

    #[test]
    fn package_json_entries_collects_workspace_pkg_name_to_main_entry() {
        let dir = TempDir::new("zzop-pkg-entries-ws-main");
        dir.write(
            "packages/prisma/package.json",
            r#"{"name": "@acme/prisma", "main": "index.ts"}"#,
        );
        let all_paths: HashSet<String> = ["packages/prisma/index.ts".to_string()]
            .into_iter()
            .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/prisma/package.json".to_string()),
            &all_paths,
        );
        let pkg = scan.workspace_pkgs.get("@acme/prisma").unwrap();
        assert_eq!(pkg.dir, "packages/prisma");
        assert_eq!(pkg.entry.as_deref(), Some("packages/prisma/index.ts"));
    }

    #[test]
    fn package_json_entries_falls_back_to_index_ts_when_no_main_module_exports() {
        let dir = TempDir::new("zzop-pkg-entries-ws-index-fallback");
        dir.write("packages/lib/package.json", r#"{"name": "@acme/lib"}"#);
        dir.write("packages/lib/index.ts", "export {};\n");
        let all_paths: HashSet<String> =
            ["packages/lib/index.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/lib/package.json".to_string()),
            &all_paths,
        );
        let pkg = scan.workspace_pkgs.get("@acme/lib").unwrap();
        assert_eq!(pkg.entry.as_deref(), Some("packages/lib/index.ts"));
    }

    #[test]
    fn package_json_entries_workspace_pkg_entry_none_when_nothing_resolves() {
        // A pure sub-path-only package with no entry point: no `main`/`module`/`exports`, no root
        // `index.ts` — every import of it names a sub-path. `entry` staying `None` (rather than some
        // guessed path) is the honest signal.
        let dir = TempDir::new("zzop-pkg-entries-ws-no-entry");
        dir.write("packages/lib/package.json", r#"{"name": "@acme/lib"}"#);
        dir.write("packages/lib/tracking.ts", "export {};\n");
        let all_paths: HashSet<String> = ["packages/lib/tracking.ts".to_string()]
            .into_iter()
            .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/lib/package.json".to_string()),
            &all_paths,
        );
        let pkg = scan.workspace_pkgs.get("@acme/lib").unwrap();
        assert_eq!(pkg.dir, "packages/lib");
        assert_eq!(pkg.entry, None);
    }

    // --- tsconfig_scan ---

    #[test]
    fn tsconfig_scan_collects_star_pattern_and_base_url() {
        let dir = TempDir::new("zzop-tsconfig-star");
        dir.write(
            "tsconfig.json",
            r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["./src/*"]}}}"#,
        );
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        let cfg = scan.get("").unwrap();
        assert_eq!(cfg.base_url, "");
        assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
    }

    #[test]
    fn tsconfig_scan_registers_under_own_directory_not_root() {
        let dir = TempDir::new("zzop-tsconfig-nested-dir");
        dir.write(
            "packages/app/tsconfig.json",
            r#"{"compilerOptions": {"baseUrl": "src"}}"#,
        );
        let scan = tsconfig_scan(
            dir.path(),
            std::iter::once("packages/app/tsconfig.json".to_string()),
        );
        assert!(scan.contains_key("packages/app"));
        assert_eq!(
            scan.get("packages/app").unwrap().base_url,
            "packages/app/src"
        );
    }

    #[test]
    fn tsconfig_scan_follows_one_level_of_local_extends_and_merges() {
        let dir = TempDir::new("zzop-tsconfig-extends");
        dir.write(
            "tsconfig.base.json",
            r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@shared/*": ["./shared/*"], "@app/*": ["./old-app/*"]}}}"#,
        );
        dir.write(
            "tsconfig.json",
            r#"{"extends": "./tsconfig.base.json", "compilerOptions": {"paths": {"@app/*": ["./src/*"]}}}"#,
        );
        let scan = tsconfig_scan(
            dir.path(),
            vec![
                "tsconfig.json".to_string(),
                "tsconfig.base.json".to_string(),
            ]
            .into_iter(),
        );
        let cfg = scan.get("").unwrap();
        // Child's `@app/*` overrides the parent's; parent-only `@shared/*` is kept (parent-fills-gaps); the
        // parent's `baseUrl` is inherited since the child doesn't declare its own.
        assert_eq!(
            cfg.paths.get("@app/*").unwrap(),
            &vec!["./src/*".to_string()]
        );
        assert_eq!(
            cfg.paths.get("@shared/*").unwrap(),
            &vec!["./shared/*".to_string()]
        );
        assert_eq!(cfg.base_url, "");
    }

    #[test]
    fn tsconfig_scan_ignores_non_local_extends() {
        let dir = TempDir::new("zzop-tsconfig-extends-pkg");
        dir.write(
            "tsconfig.json",
            r#"{"extends": "@tsconfig/node18/tsconfig.json", "compilerOptions": {"paths": {"@/*": ["./src/*"]}}}"#,
        );
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        // The non-local `extends` target is never read (no such file exists here); the tsconfig's own
        // `compilerOptions` still register normally.
        let cfg = scan.get("").unwrap();
        assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
    }

    #[test]
    fn tsconfig_scan_tolerates_jsonc_comments_and_trailing_commas() {
        let dir = TempDir::new("zzop-tsconfig-jsonc");
        dir.write(
            "tsconfig.json",
            r#"{
                // line comment
                "compilerOptions": {
                    /* block comment */
                    "baseUrl": ".",
                    "paths": {
                        "@/*": ["./src/*"],
                    },
                },
            }"#,
        );
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        let cfg = scan.get("").unwrap();
        assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
    }

    #[test]
    fn tsconfig_scan_skips_directory_with_neither_base_url_nor_paths() {
        let dir = TempDir::new("zzop-tsconfig-empty");
        dir.write("tsconfig.json", r#"{"compilerOptions": {"strict": true}}"#);
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        assert!(scan.is_empty());
    }

    #[test]
    fn tsconfig_scan_degrades_on_invalid_json() {
        let dir = TempDir::new("zzop-tsconfig-invalid");
        dir.write("tsconfig.json", "{ this is not json");
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        assert!(scan.is_empty());
    }
}
