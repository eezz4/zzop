//! Wire-contract request types: the serde `Deserialize` shapes every `*_json` entry point accepts.

use std::collections::BTreeMap;

use serde::Deserialize;

use zzop_core::{GlobalExclude, Severity, Suppression};

/// `packs_dir`'s accepted shapes: a single directory (unchanged, pre-existing wire form) or an array of
/// directories, all loaded and merged (see `base_engine_config`'s doc for the collision rule). `untagged`
/// tries `String` first, falling back to `Vec<String>` — either form deserializes unambiguously since JSON
/// strings and arrays never overlap.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PacksDir {
    One(String),
    Many(Vec<String>),
}

impl PacksDir {
    /// Normalizes either wire shape into an ordered list of directories to load, in the order given (a
    /// `One` is a single-element list — `base_engine_config` applies the exact same later-wins merge
    /// either way, so this is the only place the two shapes need to be told apart).
    pub(crate) fn as_dirs(&self) -> Vec<&str> {
        match self {
            PacksDir::One(s) => vec![s.as_str()],
            PacksDir::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

/// One tree's request shape (wire = camelCase via `rename_all`; full field list = this struct —
/// `root` plus 14 optional knobs, see `docs/modules/napi.md`'s table for the authoritative
/// per-field contract). `#[serde(deny_unknown_fields)]` is deliberately
/// NOT set — an older/newer Node host sending an extra field (e.g. a future `scores_config` knob) should
/// degrade to "ignored", not fail the whole call.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzeRequest {
    pub root: String,
    pub source_id: String,
    pub packs_dir: Option<PacksDir>,
    /// Inline rule-pack definitions injected as data — the self-contained-binary alternative to
    /// `packs_dir`: a host with no filesystem-resident pack directory (e.g. `zzop-mcp`'s bundled packs,
    /// embedded at compile time by `zzop-config`'s `build.rs`) hands its packs straight to the engine as
    /// `RulePackDef` values instead of pointing at a directory of pack JSON files on disk. Wire name
    /// `packDefs`; `#[serde(default)]` (inherited from this struct's `default` attribute) makes this
    /// genuinely additive — an older or newer JS host that never sends `packDefs` at all gets the
    /// pre-existing `packs_dir`-only behavior, byte-for-byte. Loaded BEFORE `packs_dir` directories in
    /// `base_engine_config`'s seed order, so a directory pack with the same id WINS the collision whole
    /// (a caller's own `packsDir` directory overrides an embedded bundled pack with the same id, mirroring
    /// the JS wrapper's bundled-first `index.js` ordering — see `base_engine_config`'s doc for the full
    /// three-layer collision rule). `EnvelopeAnalyzeRequest` carries the same field with the identical
    /// contract, so every analyze entry point (`analyze`/`analyzeTrees`/`analyzeEnvelope`) accepts
    /// inline packs.
    pub pack_defs: Vec<zzop_core::RulePackDef>,
    pub cache_dir: Option<String>,
    pub git: Option<GitOptionsRequest>,
    pub size_cap: Option<usize>,
    pub disabled_rules: Vec<String>,
    /// Per-rule severity remap (rule id -> `"critical"`/`"warning"`/`"info"`). Reuses `zzop_core::Severity`
    /// (lowercase serde) and `RuleConfig::severity_overrides` directly. Default: empty (no remaps).
    pub severity_overrides: BTreeMap<String, Severity>,
    /// Finding-level accept-list — `{rule, path?, glob?}` entries dropping matching findings (`glob`
    /// wins when both filters are set). Reuses
    /// `zzop_core::Suppression`/`RuleConfig::suppressions` directly. Default: empty (nothing suppressed).
    pub suppressions: Vec<Suppression>,
    /// Config-wide, rule-agnostic finding-level filter — the top-level `"exclude"` config key's wire
    /// exposure (camelCase `globalExcludes`). `{path?, glob?}` entries drop matching findings from EVERY
    /// rule at once (the file is still analyzed; only findings are filtered). Reuses
    /// `zzop_core::GlobalExclude`/`RuleConfig::global_excludes` directly. Default: empty (nothing globally
    /// excluded).
    pub global_excludes: Vec<GlobalExclude>,
    /// Mode-B adapter overlays: partial `NormalizedEnvelope`s (typically just `io` + fragment channels
    /// for a handful of files) merged ON TOP of native TypeScript analysis for this tree — the wire
    /// exposure of `EngineConfig::adapter_overlays`. Each overlay is re-validated and soft-skipped with a
    /// warning if invalid (see `envelope::apply_adapter_overlays`); a structurally-unparseable overlay
    /// fails request deserialization (producer's contract to emit well-formed envelopes). Overlays are
    /// re-applied every run AFTER the native cache, so they need no cache-key participation.
    pub adapter_overlays: Vec<zzop_core::NormalizedEnvelope>,
    /// Deployment-topology "whole-tree" mount point — the wire exposure of an implicit
    /// `zzop_engine::MountRule { dir: String::new(), at: mounted_at }` covering the entire tree (the
    /// engine's own longest-`dir`-wins rule makes this the lowest-specificity entry: any `mounts[]` entry
    /// with a non-empty `dir` beats it on a match). `None` (the default) adds no implicit whole-tree
    /// mount. See `build_engine_config`'s fold order for exactly how this combines with `mounts`. Shape
    /// (must start with `/`, no scheme/placeholder/whitespace) is NOT validated here — that is the
    /// mapper's fail-fast gate (`packages/cli/lib/mapper.js`); the engine's own
    /// `analyze::compose::apply_config_mounts` defensively warns and skips a malformed value as a
    /// last-resort backstop.
    pub mounted_at: Option<String>,
    /// Deployment-topology mounts, in array order — the wire exposure of
    /// `zzop_engine::EngineConfig::mounts` (see that field's doc for the longest-`dir`-wins matching rule
    /// `apply_config_mounts` applies at assemble time). Empty (the default) declares no mounts beyond
    /// `mounted_at`. Same "mapper validates, the facade passes through, engine defensively backstops" contract
    /// as `mounted_at`.
    pub mounts: Vec<MountEntryRequest>,
    /// Hosts this tree owns — the wire exposure of `zzop_engine::EngineConfig::hosts` (absolute-URL
    /// consumes to these hosts are re-keyed internal at cross-layer link time, see
    /// `zzop_core::LinkOptions::internal_hosts`). Empty (the default) declares no hosts.
    pub hosts: Vec<String>,
}

/// Deserializes `T | null` into `Some(Some(T)) | Some(None)` so a struct-level `#[serde(default)]`
/// (-> `None`) can tell "key absent" apart from "key explicitly `null`" — serde's standard
/// double-`Option` idiom, used by `EnvelopeAnalyzeRequest::packs_dir` (see its doc for why the
/// distinction is contract-bearing there).
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

/// One `AnalyzeRequest::mounts` entry: `{dir, at}` — the wire exposure of `zzop_engine::MountRule`,
/// field-for-field. `#[serde(rename_all = "camelCase")]` is a no-op today (`dir`/`at` are already single
/// lowercase words) but kept for consistency with every other request struct at this boundary. No shape
/// validation happens here (empty/leading-slash/scheme/backslash/etc.) — see `AnalyzeRequest::mounts`'s
/// doc for why that is deliberately the mapper's job, not this layer's.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MountEntryRequest {
    pub dir: String,
    pub at: String,
}

/// `AnalyzeRequest::git`'s payload — mirrors `zzop_engine::GitOptions` field-for-field, as JSON input.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GitOptionsRequest {
    pub since: Option<String>,
    pub recent_days: Option<u32>,
    /// Custom commit-type classifier table — the wire exposure of config `git.commitTypePatterns`.
    /// REPLACES `zzop_metrics::default_commit_type_patterns()` entirely when present and non-empty (match
    /// order = array order); absent or an empty array falls back to the default table. See
    /// `zzop_engine::GitOptions::commit_type_patterns`'s doc for the full contract, including how an
    /// invalid regex is handled (skipped, surfaced as a `warnings` entry, never a panic).
    pub commit_type_patterns: Option<Vec<CommitTypePatternRequest>>,
}

/// One `git.commitTypePatterns` config-file entry: `{ pattern: <regex>, tag: <TAG> }`. A dedicated struct
/// (rather than accepting a raw 2-element JSON array over the wire) keeps the shape self-describing for a
/// config-file author; `build_engine_config` flattens the list into the `(String, String)` tuple pairs
/// `zzop_engine::GitOptions::commit_type_patterns` / `zzop_git::CollectOptions::commit_type_patterns` use
/// internally.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitTypePatternRequest {
    pub pattern: String,
    pub tag: String,
}

/// `analyzeTrees`'s request shape: `{trees: AnalyzeRequest[]}` — one `EngineConfig` per tree, joined by
/// `zzop_engine::analyze_trees` (multi-tree/cross-layer).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzeTreesRequest {
    pub trees: Vec<AnalyzeRequest>,
}

/// `analyzeEnvelope`'s request shape (`docs/NORMALIZED_AST.md`'s protocol receiver): unlike
/// `AnalyzeRequest` there is no `root`/`cacheDir`/`git`/`sizeCap` — an envelope carries no filesystem
/// location the engine can re-read (see `zzop_engine::analyze_envelope`'s own module doc for exactly
/// which config knobs envelope mode ignores and why).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EnvelopeAnalyzeRequest {
    pub source_id: String,
    /// Double-`Option`, unlike `AnalyzeRequest::packs_dir`: the envelope path is the one entry point
    /// where the FACADE injects the bundled-pack default (`analyze_envelope_json` — no host config
    /// front-end covers envelope requests, so the default lives at the shared chokepoint), and the
    /// injection must honor the JS wrapper's documented `packsDir: null` opt-out. A plain `Option`
    /// deserializes an explicit `null` and an ABSENT key identically, erasing
    /// the opt-out; here `None` = key absent (inject the bundled packs), `Some(None)` = explicit
    /// `null` (opt out of the bundled seed and all pack directories — caller `packDefs`, if any, are
    /// still honored, per the standing "packDefs always load" contract), `Some(Some(dirs))` = load these
    /// directories (bundled packs still injected as inline seeds; a same-id directory pack wins the
    /// collision whole, unchanged).
    #[serde(deserialize_with = "double_option")]
    pub packs_dir: Option<Option<PacksDir>>,
    /// Inline rule-pack definitions injected as data — the envelope-path twin of
    /// `AnalyzeRequest::pack_defs`, with the IDENTICAL serde shape and semantics: wire name `packDefs`,
    /// defaults to empty (absent = the pre-existing `packsDir`-only behavior, byte-for-byte), seeded
    /// BEFORE `packs_dir` directories in `base_engine_config`'s order so a directory pack with the same
    /// id WINS the collision whole. See `AnalyzeRequest::pack_defs` for the full contract.
    pub pack_defs: Vec<zzop_core::RulePackDef>,
    pub disabled_rules: Vec<String>,
    /// Per-rule severity remap (rule id -> `"critical"`/`"warning"`/`"info"`). See `AnalyzeRequest`.
    pub severity_overrides: BTreeMap<String, Severity>,
    /// Finding-level accept-list — `{rule, path?}` entries. See `AnalyzeRequest`.
    pub suppressions: Vec<Suppression>,
    /// Config-wide, rule-agnostic finding-level filter. See `AnalyzeRequest::global_excludes`.
    pub global_excludes: Vec<GlobalExclude>,
    /// Deployment-topology "whole-tree" mount point — the envelope-path twin of
    /// `AnalyzeRequest::mounted_at`, with the IDENTICAL serde shape and fold semantics (see that
    /// field's doc; `config::fold_mounts` is the one shared fold for both paths). The engine's mount
    /// apply already runs uniformly in envelope mode (`analyze_envelope`'s `apply_config_mounts`
    /// call — `docs/NORMALIZED_AST.md`'s "apply uniformly to Mode A envelopes and natively-parsed
    /// trees alike" promise); this field is the wire plumbing that lets a caller actually reach it.
    pub mounted_at: Option<String>,
    /// Deployment-topology mounts, in array order — the envelope-path twin of
    /// `AnalyzeRequest::mounts`, identical shape (`{dir, at}` via `MountEntryRequest`) and identical
    /// fold order (every `mounts[]` entry first, `mounted_at` as the implicit `dir: ""` entry LAST).
    pub mounts: Vec<MountEntryRequest>,
}
