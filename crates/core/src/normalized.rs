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

mod version;
pub use version::{
    parse_contract_version, MIN_VERSION_FOR_CALLS, MIN_VERSION_FOR_OVERRIDES,
    MIN_VERSION_FOR_ROUTER_MOUNT_REF, NORMALIZED_AST_CONTRACT_VERSION, NORMALIZED_AST_FORMAT,
    SUPPORTED_NORMALIZED_AST_VERSION,
};

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
    /// Per-file TEST-ONLY line spans (1-based, inclusive) — external-parser counterpart of
    /// `zzop_core::dsl::SourceFile::test_spans` (see that field's doc for the exact contract): regions
    /// this producer PROVED are compiled out of the shipping build, the fact that lets a rule pack tell
    /// Rust's in-file `#[cfg(test)] mod tests` apart from shipped code. OPTIONAL (`#[serde(default)]`;
    /// absent = empty = nothing is subtracted, so an envelope that omits it keeps its full judgment
    /// rather than going quiet — the SAFE direction for a SUBTRACTIVE fact, and the reason this field
    /// needs no schema-version gate). Same snake_case-with-camelCase-input-alias convention as
    /// `loop_spans`/`function_spans` above.
    #[serde(default, alias = "testSpans")]
    pub test_spans: Vec<(u32, u32)>,
    /// Per-file CALL-GRAPH EDGES (`calls-v1`) — the external-parser counterpart of the `RawCall` sites
    /// native parsers project (`crate::callgraph::RawCall`, same serde type verbatim): each entry
    /// attributes one call site to its enclosing top-level symbol (`from_symbol`, which MUST be
    /// `"<this file's path>#<symbol>"` — [`validate_envelope`] rejects any other file's prefix) and
    /// names its target (`callee_name`, plus the optional typed-receiver/heritage refinements). This is
    /// the channel that lets a producer with NO native call-graph parser turn on the call-graph-BFS
    /// rule family (`mutating-route-no-auth`, `unsafe-read-endpoint`, `non-idempotent-write`) for its
    /// language in envelope mode: the engine resolves these against this SAME projection's `imports` +
    /// symbol set (cross-file specifiers under the exact/`./`-relative envelope contract above; an
    /// unresolvable callee's edge is dropped, never guessed — identical to the native resolver's
    /// contract). OPTIONAL (`#[serde(default)]`; absent = empty = those rules stay silent for this
    /// envelope, the recall-direction degrade, which Mode A disclosures name rather than letting it
    /// read as "no findings"). NOT a `Matcher::CallScan` fact — call-graph EDGES and per-file
    /// call-SITES are different fact categories (see `dsl::SourceFile::call_sites`). Requires
    /// `version >= `[`MIN_VERSION_FOR_CALLS`]: an older engine drops the field silently and its
    /// call-graph rules stay quiet, so a mislabelled envelope must fail loudly on the engine that CAN
    /// tell the producer. Mode B (adapter overlays) does not consume this channel today — the native
    /// call graph re-parses dispatched sources itself — and discloses that per overlay rather than
    /// silently ignoring it. Serialized snake_case (`calls`), no camelCase alias: the field is new, so
    /// there is no frozen-v1 camelCase emitter to tolerate.
    #[serde(default)]
    pub calls: Vec<crate::callgraph::RawCall>,
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
