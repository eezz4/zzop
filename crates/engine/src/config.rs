//! Engine configuration types for one `analyze_tree` call — `EngineConfig`, `MountRule`,
//! `GitOptions`, `PackSource`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use zzop_core::{RuleConfig, RulePackDef};
use zzop_metrics::ScoresConfig;

use crate::{DispatchConfig, IoOptions, DEFAULT_SIZE_CAP};

/// Provenance of one loaded DSL pack, as reported by `AnalyzeOutput::packs_loaded`. Only two classes
/// exist at this boundary: `Dir` = read off disk from a packs directory (`packsDir` — which is also how
/// the bundled default packs arrive, since the `zzop-config` mapper prepends the bundled
/// directory to `packsDir`); `Inline` = handed to the engine as an already-parsed, in-memory
/// `RulePackDef` (`packDefs` — which is also how the Rust hosts' build.rs-embedded bundled packs arrive,
/// via `zzop-config`'s `withDefaults` injection). There is deliberately NO `Bundled` variant: "bundled"
/// is a packaging fact of the host, not something the engine can observe — it sees a directory path or
/// an inline def either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackSource {
    /// Loaded from a `packsDir` directory (`zzop_core::pack_loader::load_dsl_packs`).
    Dir,
    /// Supplied as an in-memory `RulePackDef` (`packDefs`, or a direct embedder filling `packs`).
    Inline,
}

impl PackSource {
    /// The wire string `AnalyzeOutput::packs_loaded` serializes: `"dir"` | `"inline"`.
    pub fn as_str(self) -> &'static str {
        match self {
            PackSource::Dir => "dir",
            PackSource::Inline => "inline",
        }
    }
}

/// Engine configuration for one `analyze_tree` call. `packs` is already-loaded `RulePackDef`s (e.g. via
/// `zzop_core::pack_loader::load_dsl_packs`) — this crate does not read `rules/dsl/*.json` off disk itself,
/// keeping "where rule packs live" a caller concern (a CLI, a test, an N-API host).
pub struct EngineConfig {
    /// Tags the assembled `CommonIr`'s `source` field (zzop's multi-tree / cross-layer-join convention).
    pub source_id: String,
    pub dispatch: DispatchConfig,
    /// Files strictly larger than this (in bytes) skip structural parsing (see `DEFAULT_SIZE_CAP`).
    pub size_cap: usize,
    pub rule_config: RuleConfig,
    pub packs: Vec<RulePackDef>,
    /// Per-pack provenance for `AnalyzeOutput::packs_loaded`: pack id -> where that pack came from. A
    /// pack id absent from this map reports `PackSource::Inline` (an in-memory def is what the engine
    /// actually received — the honest default for a direct embedder that fills `packs` without touching
    /// this map). Purely descriptive output plumbing: never consulted by gating, evaluation, or the
    /// cache fingerprint.
    pub pack_sources: BTreeMap<String, PackSource>,
    /// Router-identifier-name config for the per-file Hono-route provide projection — see `crate::io`.
    pub io: IoOptions,
    /// When `Some`, `analyze_tree` runs `zzop_git::collect` over `root` and, if it succeeds, builds real
    /// `FileNode`s from the collected history and computes `scores`/`health`/`recommendations`/
    /// `critical`/`seams`. `None` (the default) leaves those fields empty/`None`; no git process is ever
    /// spawned. A `Some` on a non-git root does not panic — see `AnalyzeOutput::warnings`.
    pub git: Option<GitOptions>,
    /// Override for `zzop_metrics::compute_scores`'s threshold/vocabulary config. Only consulted when
    /// `git` is `Some` and collection succeeds.
    pub scores_config: ScoresConfig,
    /// When `Some`, `analyze_tree` opens (creating if absent) a `zzop_cache::AnalysisCache` at this path
    /// and drives the fused per-file pass through it: a file whose content hash + parser fingerprint +
    /// ruleset fingerprint already has a cached IR *and* findings entry skips parsing and rule
    /// evaluation entirely. `None` (the default) never touches a cache directory. A cache directory that
    /// fails to open degrades to "cache off" for that call plus a `warnings` entry — never a panic.
    ///
    /// **The `None` default is deliberate and must stay** — do not "fix" it by defaulting a path here.
    /// This is a library knob: an embedder that names no directory gets no directory created inside its
    /// repository. Shipped RUNS do get a default, supplied one layer up by the config front-end
    /// (`zzop-config` maps an omitted `cacheDir` to `zzop_cache::DEFAULT_CACHE_DIR`), which is also the
    /// only layer that knows what to resolve a relative path against. Defaulting here would additionally
    /// be inert: `zzop-facade` assigns this field from the request unconditionally, so a default set in
    /// `EngineConfig::default()` is overwritten before `analyze_tree` ever sees it.
    pub cache_dir: Option<PathBuf>,
    /// Rule profiling — the ESLint `TIMING=1` / oxlint rule-timing equivalent. `false` (the default)
    /// leaves `AnalyzeOutput::rule_timings` at `None` with zero added cost. `true` times each DSL rule
    /// and each whole-graph native analysis that actually runs. Profiling never changes
    /// `findings`/`ir` — only which optional field is populated.
    pub profile_rules: bool,
    /// Partial envelopes (`io` + fragment channels only, typically) merged onto the native per-file
    /// artifacts before whole-tree assembly — the external-adapter injection point for a framework
    /// adapter that wants to participate in a NATIVE `analyze_tree` run without reimplementing a parser
    /// (contrast with Mode A, `analyze_envelope`, a full envelope standing in for the entire tree).
    /// Empty (the default) runs no overlay processing. Each overlay is
    /// `zzop_core::validate_envelope`-checked; an invalid one is skipped with a `warnings` entry.
    pub adapter_overlays: Vec<zzop_core::NormalizedEnvelope>,
    /// Deployment-topology mounts: prepend `at` to the keys of http provides whose file falls under
    /// `dir` (tree-relative, forward slashes; empty dir matches the whole tree). Config-declared facts —
    /// the outermost gateway layer, stacked ON TOP of code-extracted prefixes (Nest setGlobalPrefix etc.),
    /// because a gateway lives outside the app. Applied by `analyze::compose::apply_config_mounts`, as the
    /// LAST provide transform in `analyze::assemble`. Empty (the default) applies no mounts.
    pub mounts: Vec<MountRule>,
    /// Hosts this tree owns: absolute-URL consumes to these hosts are re-keyed internal at cross-layer
    /// link time (see `zzop_core::LinkOptions::internal_hosts`) — plumbed in from every tree's own
    /// `hosts` by `analyze_trees`. Empty (the default) declares no hosts.
    pub hosts: Vec<String>,
    /// Declared convention vocabulary — the names this PROJECT picks for its own guards, API segments and
    /// source roots, rather than names a framework fixed. An unset field makes no judgment at all (see
    /// `VocabularyConfig`'s module doc for the per-key whole-replacement rule and why an empty declaration
    /// is never read as "use ours"), so [`EngineConfig::default`] seeds this with
    /// `VocabularyConfig::built_in()` — the visible, replaceable starting point a library embedder gets,
    /// exactly as `zzop init`'s starter file is the one a config author gets.
    ///
    /// The two are separate paths and do not interfere: `zzop-facade` assigns this field from the request
    /// unconditionally, so a request that declared nothing carries nothing, and this default never leaks
    /// into a product run.
    ///
    /// `vocabulary.skipDirs` is the one entry that does NOT live here: it lands in `dispatch.skip_dirs`,
    /// which already owned that list.
    pub vocabulary: crate::VocabularyConfig,
}

/// One deployment-topology mount (`EngineConfig::mounts`): `at` is prepended to the key of every `http`
/// provide whose `file` falls under `dir` — see that field's doc for the matching/precedence rules
/// (`analyze::compose::apply_config_mounts`).
#[derive(Debug, Clone)]
pub struct MountRule {
    pub dir: String,
    pub at: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            source_id: String::new(),
            dispatch: DispatchConfig::default(),
            size_cap: DEFAULT_SIZE_CAP,
            rule_config: RuleConfig::default(),
            packs: Vec::new(),
            pack_sources: BTreeMap::new(),
            io: IoOptions::default(),
            git: None,
            scores_config: ScoresConfig::default(),
            cache_dir: None,
            profile_rules: false,
            adapter_overlays: Vec::new(),
            mounts: Vec::new(),
            hosts: Vec::new(),
            vocabulary: crate::VocabularyConfig::built_in(),
        }
    }
}

/// Git-history collection options for `EngineConfig::git` — a thin mirror of `zzop_git::CollectOptions`.
/// `zzop_metrics::default_commit_type_patterns()` is used for the commit-type vocabulary UNLESS
/// `commit_type_patterns` supplies a custom table (see that field's doc).
#[derive(Debug, Clone)]
pub struct GitOptions {
    /// `git log --since=<since>`; `None` = full history.
    pub since: Option<String>,
    /// Window, in days, for each `FileNode`'s `recent_*` fields.
    pub recent_days: u32,
    /// Custom commit-type classifier table (regex source, TAG pairs, in match order) — the config-file
    /// wire path for `git.commitTypePatterns` (the wire `GitOptionsRequest::commit_type_patterns`). When
    /// `Some` and non-empty, this REPLACES `zzop_metrics::default_commit_type_patterns()` entirely (same
    /// "later table wins whole, not merged" semantics the default table's own REVERT-first ordering
    /// depends on) — match order is array order. `None`, or `Some(vec![])`, falls back to the default
    /// table. See `analyze::diagnostics::collect_git` for where this is applied, and
    /// `zzop_git::tags::CommitClassifiers::compile`'s doc for what happens to a pattern that fails to
    /// compile as a regex (skipped, never a panic; `collect_git` additionally surfaces a `warnings` entry
    /// naming any such pattern, since a silently-inert custom pattern is exactly the narrowed-scope
    /// degradation this codebase's self-report contract exists for).
    pub commit_type_patterns: Option<Vec<(String, String)>>,
    /// DECLARED subject-pattern table (regex source, label pairs, in declaration order) — the
    /// config-file wire path for `git.commitSubjectPatterns`. `None` or an empty vec means NO commit
    /// is labelled: unlike `commit_type_patterns` above there is NOTHING to fall back to, by design.
    /// A revert/ticket/hotfix subject convention is per-project, so a built-in table would be the
    /// engine guessing at a convention it cannot know and silently mislabelling every project that
    /// spells it differently — this axis reads declarations or stays silent. See
    /// `analyze::diagnostics::collect_git` for where it is applied (including the warning naming any
    /// pattern that fails to compile, and the one naming a declared table that matched nothing).
    pub commit_subject_patterns: Option<Vec<(String, String)>>,
}

impl Default for GitOptions {
    fn default() -> Self {
        GitOptions {
            since: None,
            recent_days: 30,
            commit_type_patterns: None,
            commit_subject_patterns: None,
        }
    }
}
