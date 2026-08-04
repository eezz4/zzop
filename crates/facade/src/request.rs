//! Wire-contract request types: the serde `Deserialize` shapes every `*_json` entry point accepts.

use std::collections::BTreeMap;

mod git;
mod parsers;

pub use git::{CommitSubjectPatternRequest, CommitTypePatternRequest, GitOptionsRequest};
pub use parsers::ParsersRequest;

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
/// `root` plus 15 optional knobs, see `docs/modules/facade.md`'s AnalyzeRequest table for the authoritative
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
    /// the bundled-first ordering — see `base_engine_config`'s doc for the full
    /// three-layer collision rule). `EnvelopeAnalyzeRequest` carries the same field with the identical
    /// contract, so every analyze entry point (`analyze`/`analyzeTrees`/`analyzeEnvelope`) accepts
    /// inline packs.
    pub pack_defs: Vec<zzop_core::RulePackDef>,
    pub cache_dir: Option<String>,
    pub git: Option<GitOptionsRequest>,
    pub size_cap: Option<usize>,
    pub disabled_rules: Vec<String>,
    /// DSL pack ALLOWLIST (`packsOnly`) — the opt-in twin of `disabled_rules`, which can only say
    /// "everything except". Semantics both dialects share (empty = no allowlist, packs only, composes
    /// with `disabled_rules`): `zzop_core::RuleConfig::only_packs`. Config-file dialect: `packs.only`.
    pub packs_only: Vec<String>,
    /// Per-rule severity remap (rule id -> `"critical"`/`"warning"`/`"info"`). Reuses `zzop_core::Severity`
    /// (lowercase serde) and `RuleConfig::severity_overrides` directly. Default: empty (no remaps).
    pub severity_overrides: BTreeMap<String, Severity>,
    /// Finding-level accept-list — `{rule, path?, glob?}` entries dropping matching findings (`glob`
    /// wins when both filters are set). Reuses
    /// `zzop_core::Suppression`/`RuleConfig::suppressions` directly. Default: empty (nothing suppressed).
    pub suppressions: Vec<Suppression>,
    /// Config-wide, rule-agnostic REPORT-level filter — the top-level `"exclude"` config key's wire
    /// exposure (camelCase `globalExcludes`). `{path?, glob?}` entries drop matching paths from EVERY
    /// rule at once AND from the `recommendations` score channel (the file is still analyzed and still
    /// appears in `nodes`/the dep graph; only what is reported about it is filtered). Reuses
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
    /// mapper's fail-fast gate (`crates/config/src/mapper.rs`, ported from the removed JS CLI's
    /// `mapper.js`, 2026-07-20); the engine's own
    /// `analyze::compose::apply_config_mounts` defensively warns and skips a malformed value as a
    /// last-resort backstop.
    pub mounted_at: Option<String>,
    /// Deployment-topology mounts, in array order — the wire exposure of
    /// `zzop_engine::EngineConfig::mounts` (see that field's doc for the longest-`dir`-wins matching rule
    /// `apply_config_mounts` applies at assemble time). Empty (the default) declares no mounts beyond
    /// `mounted_at`. Same "mapper validates, the facade passes through, engine defensively backstops" contract
    /// as `mounted_at`.
    pub mounts: Vec<MountEntryRequest>,
    /// The base path this tree's own OUTBOUND calls carry — the wire exposure of
    /// `zzop_engine::EngineConfig::client_base`, and the CALLING-side mirror of `mounted_at`. `None` (the
    /// default) prefixes nothing. Same "mapper validates, the facade passes through, engine defensively
    /// backstops" contract as `mounted_at`, and the same shape rules (leading `/`, no scheme, no `{}`).
    pub client_base: Option<String>,
    /// Hosts this tree owns — the wire exposure of `zzop_engine::EngineConfig::hosts` (absolute-URL
    /// consumes to these hosts are re-keyed internal at cross-layer link time, see
    /// `zzop_core::LinkOptions::internal_hosts`). Empty (the default) declares no hosts.
    pub hosts: Vec<String>,
    /// Lightweight route-fact injection — the ergonomic counterpart of `adapter_overlays` for the common
    /// "inject one route zzop could not resolve from source" case (a non-literal path, a dynamic verb, a
    /// computed URL). `build_engine_config` expands the whole array into ONE synthetic adapter overlay of
    /// `http` provides/consumes (see `RouteInjectionRequest`), so it composes through the same join path as
    /// a hand-authored overlay. Empty (the default) injects no routes.
    pub routes: Vec<RouteInjectionRequest>,
    /// Declared convention vocabulary — the wire exposure of the config file's `vocabulary` object and of
    /// `zzop_engine::EngineConfig::vocabulary`. Reuses the engine type directly (rather than mirroring it
    /// the way `GitOptionsRequest` mirrors `GitOptions`) precisely because the two must never drift: the
    /// same struct that decides what a vocabulary key MEANS is the one deserialized off the wire, so a
    /// field added on one side cannot go missing on the other. Default (every field unset) keeps every
    /// built-in vocabulary — see that type's module doc for the per-key whole-replacement rule.
    pub vocabulary: zzop_engine::VocabularyConfig,
    /// Parser routing overrides — `parsers.globOverrides` in the config dialect. Each entry force-routes
    /// paths matching `glob` to a named language, ahead of the extension map.
    ///
    /// A SEPARATE roof from `vocabulary` on purpose: every key under that one names something the project
    /// CALLS its own (a guard, a segment, a directory), and this names a path→parser MAPPING. Folding a
    /// mapping in among names would repeat the mistake `git.commitTypePatterns` is explicitly kept out of
    /// `vocabulary` to avoid — same "user-declared table" feel, different subject matter.
    #[serde(default)]
    pub parsers: ParsersRequest,
    /// Rule TIMING instrumentation — the wire exposure of `zzop_engine::EngineConfig::profile_rules`
    /// (the ESLint `TIMING=1` / oxlint rule-timing equivalent). `false` (the default) leaves
    /// `AnalyzeOutput::rule_timings` at `None` with zero added cost; `true` times each DSL rule and each
    /// whole-graph native analysis that actually runs.
    ///
    /// Deliberately NOT a `zzop.config.jsonc` key, and the one request field here that is not: every
    /// other knob on this struct is a DECLARATION ABOUT THE PROJECT (what it calls its guards, where it
    /// is mounted, which rules it disables) and belongs in a file the project commits. A timing report
    /// is a QUESTION ABOUT THIS RUN — wall-clock, machine-specific, and jittery run to run — so it rides
    /// a per-invocation switch (`zzop analyze --profile-rules`) instead. See `crates/config/
    /// config-surface.json`'s `cliFlags`, which vouches for that flag and has no `configKeys` twin.
    ///
    /// Profiling NEVER changes `findings`/`ir` — only which optional output field is populated.
    pub profile_rules: bool,
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

/// One `AnalyzeRequest::routes` entry — a single route fact a caller injects when zzop could not resolve it
/// from source (a non-literal route path, a dynamic HTTP verb, a computed client URL). The LIGHTWEIGHT
/// counterpart of hand-authoring an `adapterOverlays` envelope for the common "inject one route" case:
/// `build_engine_config` expands the whole `routes` array into ONE synthetic adapter overlay carrying
/// `http` io entries, so this reuses the exact same, already-proven overlay-`io` cross-layer join path
/// (`zzop_engine::analyze`'s `apply_adapter_overlays` -> assemble -> `link_cross_layer_io`) with no new
/// engine wiring — the ergonomic sibling of `mounts`/`hosts` on the route-FACT axis.
///
/// `key` is the `"METHOD PATH"` interface key (`"GET /api/users"`), normalized through the SAME transform
/// the native extractors use for that side — `http_interface_key` for a provide, the query/fragment-dropping
/// `http_consume_interface_key` for a consume — so `"get /api/users"` / `"GET /api/users/"` key canonically
/// and join a native route exactly. A `key` that is not a `METHOD` + `PATH` pair is skipped
/// with a `warnings` entry (an injected fact that can never join is surfaced, never silently dropped). The
/// injected route is attributed to a synthetic file marker (it is a caller-declared fact, not extracted
/// source), which the overlay's own synthetic-entry disclosure surfaces.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RouteInjectionRequest {
    pub key: String,
    /// Whether this route is SERVED here (a provide — the default, the "zzop dropped my route" case) or
    /// CALLED from here (a consume — a dynamic client URL zzop could not key).
    #[serde(default)]
    pub role: RouteRole,
}

/// `RouteInjectionRequest::role` — which side of the cross-layer join an injected route participates on.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RouteRole {
    /// The route is SERVED here — injected as an `http` PROVIDE (the default).
    #[default]
    Provide,
    /// The route is CALLED from here — injected as an `http` CONSUME.
    Consume,
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
    /// injection must honor the documented `packsDir: null` opt-out. A plain `Option`
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
    /// DSL pack allowlist (`packsOnly`) — same contract as `AnalyzeRequest::packs_only`; Mode A gates
    /// packs through the same seam, so the knob means the same thing on this lane.
    pub packs_only: Vec<String>,
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
    /// The base this envelope's own outbound calls carry — the envelope-path twin of
    /// `AnalyzeRequest::client_base`, identical shape and semantics. Present for the same reason
    /// `mounted_at` is: a declaration about where a tree sits is origin-agnostic, so a tree analyzed
    /// through Mode A must not freeze un-prefixed consume keys while the native path prefixes the same
    /// config. Mode A runs no code-extracted base pass, so this is the only base an envelope can carry.
    pub client_base: Option<String>,
    /// Rule TIMING instrumentation — the envelope-path twin of `AnalyzeRequest::profile_rules`,
    /// identical semantics (per-invocation switch, never a `zzop.config.jsonc` key, never changes
    /// `findings`/`ir`). This field deliberately did NOT exist until Mode A's pack evaluation was
    /// wired through the engine's timing accumulator (`envelope::file_pass`/`ingest`) — accepting it
    /// while `analyze_envelope` set `rule_timings: None` unconditionally would have been a knob
    /// nothing reads, the wire-level unwired-capability defect. Added in the same change that made
    /// Mode A timeable, per that standing note.
    pub profile_rules: bool,
    /// Declared convention vocabulary — the envelope-path twin of `AnalyzeRequest::vocabulary`,
    /// reusing the same engine type for the same no-drift reason. `Option`, unlike the tree twin,
    /// because the two lanes' undeclared defaults differ and this lane must tell them apart: a tree
    /// request always comes through a config front-end (a config file is mandatory there, and
    /// `zzop init` writes the built-in vocabulary into it), so its field arrives populated with
    /// whatever the author declared; the envelope lane has NO config front-end, so `None` (key
    /// absent) means `analyze_envelope_json` assigns the PRODUCT default
    /// (`VocabularyConfig::built_in()`) explicitly at the same facade chokepoint that seeds the
    /// bundled packs — never by accidentally inheriting an engine-side default. `Some(declared)` is
    /// applied WHOLE, per key, exactly like the tree lane (`config::declared::apply_declared`'s
    /// contract): a declared key replaces, an undeclared key inside the object makes no judgment.
    /// This field did not exist until Mode A's call-graph pass gave the lane a vocabulary consumer —
    /// before that, a declared `vocabulary` was silently discarded on this wire.
    pub vocabulary: Option<zzop_engine::VocabularyConfig>,
}
