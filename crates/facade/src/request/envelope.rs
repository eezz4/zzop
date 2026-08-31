//! `analyzeEnvelope`'s request shape — the Mode A lane's wire type, split from its tree-lane
//! siblings for the file-size guard. The envelope lane is the natural seam: it shares knob NAMES with
//! `AnalyzeRequest` but not its filesystem half, and each field below documents its own twin.

use std::collections::BTreeMap;

use serde::Deserialize;
use zzop_core::{GlobalExclude, Severity, Suppression};

use super::{double_option, MountEntryRequest, PacksDir};

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
