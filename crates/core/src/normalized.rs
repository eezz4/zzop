//! External-parser protocol receiver — the engine-side deserialization + validation of the
//! "Normalized AST" envelope an external/custom parser (Java, Python, JSP, anything the engine does
//! not parse natively) emits per source tree, specified by `docs/NORMALIZED_AST.md`. These types
//! mirror that document's Envelope/FileProjection JSON shapes field-for-field and reuse the SAME
//! `zzop_core` serde types (`SourceSymbol`/`ImportBinding`/`ReExport`/`IoFacts`) native parsers already
//! project into — an external parser is first-class regardless of how crude it is, as long as its
//! projection round-trips through these exact structs (see the doc's Validation section).

use serde::{Deserialize, Serialize};

use crate::io::IoFacts;
use crate::ir::{ImportMap, ReExport, SourceSymbol};

mod hints;

pub use hints::envelope_hints;

/// The exact `format` string every conforming envelope must carry (`docs/NORMALIZED_AST.md`'s Envelope
/// section).
pub const NORMALIZED_AST_FORMAT: &str = "zzop-normalized-ast";

/// The RELEASE in which the envelope shape last changed — the value a conforming producer declares as
/// `NormalizedEnvelope::version`, and the only contract-version constant this crate has.
///
/// ## Why this is a release number and not a counter (2026-07-31, user ruling)
/// It used to be an independent `u32` (v1, v2), so a reader had to hold two unrelated version systems
/// at once and could not tell from an envelope which zzop it belonged with. It now uses the workspace's
/// own semver, so "which zzop is this" and "which shape is this" are answered in the same units.
///
/// It is NOT simply the current release. It moves only when the SHAPE moves, which is what keeps a
/// producer's output working across releases: an adapter that emitted `"0.27.0"` keeps emitting it, and
/// keeps being accepted, through every later release that did not change the shape. Bumping it every
/// release would be the defect this replaces — a number that appears to describe the shape while
/// actually describing the calendar, leaving a reader unable to tell which bump mattered.
///
/// ⚠ That is exactly what happened during the v0.28.0 bump and it was caught in the pre-tag audit: this
/// constant AND `docs/contracts/example-envelope.json` were moved to `0.28.0` for a release whose only
/// envelope diff was a prose `description` rewrite, while the schema and `NORMALIZED_AST.md` still (and
/// correctly) said `0.27.0`. An adapter author copying the shipped example would have emitted an
/// envelope every 0.27.x engine rejects, for a shape identical to the one they already had. The example
/// file is now excluded from `scripts/check-release-version-propagation.sh` for this reason — a
/// release-propagation guard must not reach a constant whose contract is "do not track the release".
///
/// ## What acceptance means
/// A consumer accepts an envelope whose declared version is `<=` its own package version, and rejects
/// anything newer ("reject newer, never guess" — the same policy `pack_loader`'s DSL schema version
/// keeps). An engine built before a shape existed refuses the whole envelope rather than silently
/// ignoring the field it does not know, which is the silent-loss shape the displacement disclosure
/// exists to abolish, reappearing one level up in the contract. `FileProjection` has no
/// `deny_unknown_fields`, so without this comparison an unknown field would deserialize, be dropped,
/// and leave the producer believing it applied.
///
/// That comparison protects an HONEST producer, and only an honest one — it cannot see a mislabelled
/// envelope, which declares an old version while carrying a new field. Nothing about switching to a
/// release number changed that, so the per-feature floors ([`MIN_VERSION_FOR_OVERRIDES`]) stay: they
/// are the only thing that makes the mislabel fail loudly on the engine that DOES understand the field,
/// which is the one run where the producer can still be told.
///
/// PRE-1.0 CONSEQUENCE, accepted deliberately: an envelope declaring this version does not run on an
/// engine older than it, including engines that would have understood every field in it. `VERSIONING.md`
/// already states that `0.x` makes no backward-compatibility promise; the only known producers are this
/// repo's own `examples/adapters/`, which are migrated in the same commit.
pub const NORMALIZED_AST_CONTRACT_VERSION: &str = "0.27.0";

/// The release that introduced `overrides` — the floor an envelope must DECLARE to use it.
///
/// A per-feature floor is not made redundant by the `<=` acceptance comparison above, because the two
/// catch opposite mistakes. Acceptance catches an envelope that is NEWER than the engine. This catches
/// one that claims to be OLDER than the field it carries: declared `"0.20.0"` plus a populated
/// `overrides` deserializes cleanly on an engine that predates the field, drops it, and produces a run
/// where the adapter believes it displaced a native binding and the engine quietly did not. The engine
/// that understands the field is the only one positioned to notice, so it rejects — the producer learns
/// at authoring time instead of shipping bytes that mean different things to different engines.
///
/// A new gated field adds a constant here and moves [`NORMALIZED_AST_CONTRACT_VERSION`] to the same
/// release. Fields that are safe to silently ignore need no floor and get none.
pub const MIN_VERSION_FOR_OVERRIDES: &str = "0.27.0";

/// This build's own version, as the acceptance ceiling — see [`NORMALIZED_AST_CONTRACT_VERSION`].
/// Every crate inherits the workspace version (`version.workspace = true`), so this is the number
/// `zzop version` prints.
pub const SUPPORTED_NORMALIZED_AST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `"0.27.0"` -> `(0, 27, 0)`, or `None` when the string is not three dot-separated integers.
///
/// Hand-rolled rather than a `semver` dependency: the envelope contract needs ordering over
/// `MAJOR.MINOR.PATCH` and nothing else — no pre-release tags, no build metadata, no ranges — and a
/// dependency whose extra semantics nobody uses is a surface that can disagree with this crate's own
/// idea of what a version is. Tuple comparison gives the ordering directly.
pub fn parse_contract_version(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.');
    let mut next = || parts.next()?.parse::<u32>().ok();
    let parsed = (next()?, next()?, next()?);
    parts.next().is_none().then_some(parsed)
}

/// One external-parser invocation's output for one source tree (`docs/NORMALIZED_AST.md`'s Envelope
/// section). `format`/`version` are plain fields here (not enforced at the type level) so a
/// deserialization failure can never hide a "wrong format string"/"future version" mismatch behind a
/// generic serde error — [`validate_envelope`] is what turns those into structured, actionable errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEnvelope {
    pub format: String,
    /// The release whose envelope shape these bytes conform to — see [`NORMALIZED_AST_CONTRACT_VERSION`].
    pub version: String,
    /// `"<parser id>/<impl version>"` — the producer's self-identification. Quoted by every warning
    /// that attributes an overlay action to its adapter (displacement/overruled disclosures, source
    /// mismatch, zero-fact census) and used as the deterministic ordering key when several overlays
    /// merge, so a reader can tell WHICH adapter build did what. It is NOT a cache ingredient and
    /// bumping it invalidates nothing — measured 2026-07-31: Mode A creates no cache at all, Mode B
    /// overlays apply after the cached pass (`apply_adapter_overlays` runs post-`run_file_pass`), so
    /// a changed projection is always reflected with or without a bump. This doc used to instruct
    /// producers to bump it "so the cache notices" — an instruction with no reader, the same class as
    /// the manual native-fingerprint bump retired 2026-07-29.
    pub parser: String,
    /// Tree/source id — the cross-layer join's per-tree tag (see `crate::io`'s module doc).
    pub source: String,
    pub files: Vec<FileProjection>,
}

/// One file's projection (`docs/NORMALIZED_AST.md`'s FileProjection section) — every field mirrors a
/// reused `zzop_core` serde type; see that document for the authoritative semantics of each
/// (loc/symbols/imports/re_exports/used_names/io/degraded). Every optional-in-practice field defaults
/// to its empty value when a producer omits it (a minimal/degraded parser may legitimately have nothing
/// to say about, say, `re_exports`), matching the doc's "graceful degrade, never an error" convention.
///
/// IMPORTANT — cross-file specifier resolution for the fragment-channel fields below
/// (`const_map_fragment`/`procedure_router_fragments`/`router_mount_fragments`): a `ProcedureRouterEntry::Ref`'s
/// `specifier` or a `RouterMountEntry::Mount`'s `specifier` MUST resolve to either (a) another file's
/// `path` exactly as that file emits it in this SAME envelope's `files[]` (an exact repo-relative
/// string match), or (b) a `./`- or `../`-relative path resolved from the EMITTING file's own
/// directory. An external adapter controls both sides of this reference — it emits both the fragment
/// and every file's `path` — so a full-envelope analysis (Mode A, `analyze_envelope`) never applies
/// tsconfig/workspace-alias resolution to fragments. Adapter OVERLAYS (Mode B) compose alongside the
/// native tree and inherit its alias-aware resolver — a superset; producers should rely only on the
/// exact/relative contract above so the same envelope behaves identically in both modes.
///
/// `Default` is derived purely so a producer/test can build a PARTIAL projection with `..Default::default()`
/// instead of restating all eighteen fields (a partial envelope carrying only `attributes`, or only `io`, is
/// the normal Mode-B shape). It changes nothing on the wire: serde consults `Default` only where a
/// `#[serde(default)]` says to, and `path`/`loc` carry none — both stay mandatory in JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileProjection {
    /// Relative, forward-slash path.
    pub path: String,
    /// Raw physical line count (`text.split('\n').length` semantics).
    pub loc: u32,
    #[serde(default)]
    pub symbols: Vec<SourceSymbol>,
    #[serde(default)]
    pub imports: ImportMap,
    #[serde(default)]
    pub re_exports: Vec<ReExport>,
    /// This file's dynamic-`import()` specifiers (external-parser contract; optional). Mirrors the native
    /// `FileArtifact::dynamic_imports` — folded into the envelope dep graph as real (circular-excluded)
    /// edges so a code-split-only module keeps its fan-in on the envelope path too.
    #[serde(default)]
    pub dynamic_imports: Vec<String>,
    #[serde(default)]
    pub used_names: Vec<String>,
    /// Producer FRAGMENT CHANNELS for cross-file composition — the envelope equivalent of what
    /// `zzop_engine::analyze`'s `compose_trpc_provides` / `compose_router_mount_provides` already
    /// compose from native in-process adapters' per-file fragments. An external adapter that only
    /// knows plain io facts may omit all three entirely (default = empty, and that is a fully valid,
    /// non-degraded projection); one that additionally understands a router framework (tRPC,
    /// Hono-style code-registered mounting, etc.) can emit fragments here and have them fold into the
    /// SAME whole-tree composition pass native parsers' fragments go through — the engine does not
    /// care which side (native or external) produced a given fragment. `const_map_fragment` is a
    /// simpler same-shaped channel: `identifier -> literal string value` for this file's top-level
    /// `const` string bindings, used to resolve identifier-valued route/table arguments elsewhere in
    /// composition without a producer having to do that substitution itself.
    #[serde(default)]
    pub const_map_fragment: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub procedure_router_fragments: Vec<crate::ProcedureRouterFragment>,
    #[serde(default)]
    pub router_mount_fragments: Vec<crate::RouterMountFragment>,
    /// Class field-shape fragments (`body-shape-v1`) — the DTO-resolution substrate for
    /// `IoProvide::body.dto_ref` (see `zzop_core::ClassShapeFragment`). Same optional/graceful
    /// posture as the other fragment channels: absent = this producer doesn't extract class shapes.
    #[serde(default)]
    pub class_shape_fragments: Vec<crate::ClassShapeFragment>,
    #[serde(default)]
    pub io: IoFacts,
    /// Generic entity-attribute annotations (`attributes-v1`) — the open-vocab injection channel for
    /// cross-cutting facts a producer attaches to entities (routes/symbols/files/path-scopes) that
    /// per-file extraction can't see, e.g. `{ "target": { "pathScope": { "prefix": "/admin" } },
    /// "key": "auth-guarded", "value": true }` to mark a middleware-guarded router. Consumed BY KEY by
    /// rules (see `zzop_core::AttributeStore`); the contract/kernel is agnostic to `key`. OPTIONAL
    /// (`#[serde(default)]`; absent = no annotations = every attribute-aware rule keeps its native
    /// behavior). Collected tree-wide at assemble regardless of which file emits them.
    #[serde(default)]
    pub attributes: Vec<crate::Attribute>,
    /// Per-file loop-body line spans (1-based, inclusive) — external-parser counterpart of
    /// `zzop_core::dsl::SourceFile::loop_spans` (see that field's doc for the exact span contract: each
    /// loop statement's full span, plus array-iteration callback ARGUMENT spans only, never the whole
    /// call). OPTIONAL (`#[serde(default)]`; absent = empty = `MethodScan::trigger_in_loop` silently
    /// skips this file, same graceful-degrade policy as `symbols`/`io`). Serialized snake_case
    /// (`loop_spans`), consistent with `FileProjection`'s other fields — this struct has no
    /// `rename_all`, so `SourceSymbol`'s dual-casing convention (a `rename_all = "camelCase"` type)
    /// does not apply here; `loopSpans` is still tolerated on INPUT for camelCase emitters.
    #[serde(default, alias = "loopSpans")]
    pub loop_spans: Vec<(u32, u32)>,
    /// Per-file function line spans (1-based, inclusive) with promise-continuation callbacks merged into
    /// their call site — external-parser counterpart of `zzop_core::dsl::SourceFile::function_spans` (see
    /// that field's doc for the exact span contract and the merge rule). OPTIONAL (`#[serde(default)]`;
    /// absent = empty = `MethodScan::after_in_same_function` degrades to a NO-OP for this file, so a rule
    /// using it keeps its coarser pre-gate behavior rather than going silent — the opposite degrade
    /// direction from `loop_spans`/`trigger_in_loop`). Same snake_case-with-camelCase-input-alias
    /// convention as `loop_spans` above.
    #[serde(default, alias = "functionSpans")]
    pub function_spans: Vec<(u32, u32)>,
    /// The parser could not fully process this file (size cap, syntax failure) — `loc` must still be
    /// present regardless.
    #[serde(default)]
    pub degraded: bool,
    /// Adapter-declared framework ENTRY / reachable-root: a file loaded by the framework/runtime by
    /// convention (e.g. SvelteKit `hooks.*`/`+page`, a `.vue` route) rather than imported, so its
    /// `fan_in == 0` is expected — exempts it from `dead-candidates`/`unreachable`, the overlay
    /// counterpart of package.json manifest entries (`pipeline::package_json_entries`'s
    /// `extra_entries`). Meaningful in Mode B (`apply_adapter_overlays` unions every `is_entry` path
    /// across `EngineConfig::adapter_overlays` into `dead_candidate_findings`'s `extra_entries`); Mode A
    /// (`analyze_envelope`) does not read this field at all today (no filesystem-manifest concept to
    /// union it against). Default `false`.
    #[serde(default)]
    pub is_entry: bool,
    /// Native facts this projection DISPLACES rather than adds to. Empty
    /// by default, and empty is the whole contract for every adapter that only fills gaps — see
    /// [`ProjectionOverrides`] for the shape and [`structural_issues`] for the three rules that make a
    /// declaration valid.
    ///
    /// Why a declaration exists at all, rather than "the overlay's value wins on a key collision":
    /// measured on `examples/adapters/override-required/`, an adapter correcting a wrong native import
    /// did not COLLIDE with it — the two sides spelled the local-name key differently, so the correction
    /// arrived as a sibling entry and both the right and the wrong edge survived. Priority-on-collision
    /// therefore cannot express overriding; the displacing fact has to NAME what it displaces.
    #[serde(default)]
    pub overrides: ProjectionOverrides,
}

/// The per-channel displacement declaration carried by [`FileProjection::overrides`].
///
/// One channel today (`imports`), because one channel is what has a measured case
/// (`examples/adapters/override-required/`). Adding a second is additive here and additive on the wire
/// — a `#[serde(default)]` field beside this one — and deliberately waits for its own measured case,
/// since widening a surface ahead of evidence is the cost this repo has paid repeatedly.
///
/// `imports` lists LOCAL NAMES (the `FileProjection::imports` map's keys). Each listed name means: "the
/// native binding for this local name is wrong; mine replaces it, and the engine must say so." The
/// replacement is mandatory — a name listed here without a corresponding entry in `imports` is a
/// deletion request, which [`structural_issues`] rejects. Deletion has no honest output form (there is
/// no replacement fact to disclose, and an adapter that can delete can blind the engine silently), so it
/// is refused at the contract boundary rather than at the merge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionOverrides {
    #[serde(default)]
    pub imports: Vec<String>,
}

mod validate;
pub use validate::{validate_envelope, validate_envelope_verdict, EnvelopeVerdict};
#[cfg(test)]
mod hints_tests;
#[cfg(test)]
mod tests;

/// One patch past this build's own version — the smallest version `validate_envelope` must reject.
///
/// DERIVED, never a literal. The assertion it feeds used to spell the rejected version as a hardcoded
/// `2`, which stopped being a future version the moment the ceiling moved to 2 — the test then asserted
/// that a SUPPORTED version is rejected, and failed for the right reason at the wrong place. Deriving it
/// keeps "one past the ceiling" true at every ceiling, including every future release bump.
#[cfg(test)]
pub(crate) fn one_past_supported_version() -> String {
    let (major, minor, patch) = parse_contract_version(SUPPORTED_NORMALIZED_AST_VERSION)
        .expect("this build's own CARGO_PKG_VERSION must be MAJOR.MINOR.PATCH");
    format!("{major}.{minor}.{}", patch + 1)
}
