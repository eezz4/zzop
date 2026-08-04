//! `FileIrSlice` — the cacheable half of the engine's fused per-file pass output.
//!
//! Shape-equivalent to `zzop_engine::pipeline::FileArtifact` minus `rel` (the lookup key's business)
//! and `findings` (a separate cache entry — see `CacheKey`). Defined here, not in `zzop-engine`, so
//! this crate stays a leaf dependent of `zzop-core` only.
//!
//! `procedure_router_fragments` / `router_mount_fragments` / `wrapper_def_fragments` /
//! `wrapper_call_fragments` round-trip the matching `zzop_core` types verbatim; those types live in
//! `zzop-core` (not the TypeScript parser crate that produces them) so this crate never needs
//! `zzop-parser-typescript` as a dependency.
//!
//! # Structural-fact projection contract
//!
//! Every field below is a *structural fact* the per-file projection emits. They are NOT
//! interchangeable: each belongs to one of five categories that fixes its consumer, lifetime, and
//! obligations. A new field MUST declare which category it joins — that decision, not ad-hoc taste,
//! determines where it is threaded and who reads it. The categories:
//!
//! 1. **Cross-tree IR** — `symbols`, `io`, `loc` (the `zzop_core::MinimalIr` channels; `dep` is
//!    assembled from `imports`). Serialized and joined *across* trees by the cross-layer linker.
//! 2. **DSL-facing per-file facts** — `symbols` (body spans = method-scan spans), `io` (`IoScan`),
//!    `loop_spans` (`MethodScan::trigger_in_loop`), `function_spans`
//!    (`MethodScan::after_in_same_function`), `test_spans` (the subtractive test-region gate every
//!    matcher passes), `call_sites` (`Matcher::CallScan`), `string_literals`
//!    (`Matcher::LiteralScan`). Reach `zzop_core::dsl::RuleContext` and are
//!    read directly by DSL matchers. This set is a *deliberate contract*, not "whatever got threaded":
//!    adding to it requires a real consuming rule (no speculative generality — an unused fact waits).
//! 3. **Assemble-time composition fragments** — `procedure_router_fragments`,
//!    `router_mount_fragments`, `wrapper_def_fragments`, `wrapper_call_fragments`,
//!    `controller_prefix_route_fragments`, `class_shape_fragments`, `const_map_fragment`,
//!    `query_call_sites`. Composed whole-tree at assemble into cross-layer / schema facts; never read
//!    by a DSL matcher.
//! 4. **Dep-graph augmentation** — `imports`, `re_exports`, `dynamic_imports`, `asset_refs`,
//!    `used_names`, `exported_signature_names`. Feed the dependency graph and dead-code fan-in.
//! 5. **Native-scanner facts + file flags** — `SourceSymbol::write_sites` (embedded, native-only),
//!    `loc`, `degraded`, `minified_or_generated`.
//!
//! Contract shared by every category: OPTIONAL / graceful-degrade (an absent fact silently skips the
//! rules that would read it — never a hard error; a parser that does not produce it is a matrix blank,
//! not a failure), deterministic serialization, and — because this is the *cached* slice — a
//! `CACHE_SCHEMA_VERSION` bump (or `#[serde(default)]` back-compat) on every add, so a warm run never
//! serves a slice that silently lost the field. Language coverage is per-fact and uneven (e.g.
//! `loop_spans` covers every structural statement-loop language — TS/Go/Python/Java/C#/Rust, since
//! 2026-08-02 — while `function_spans` is TypeScript only, `test_spans` Rust only, `write_sites` and
//! `io`'s `IoConsume::retry_configured` tag are
//! TypeScript-only, while `symbols` / `io` (structure) span every parser); that unevenness is a
//! deliberate, documented state, not an oversight. WHICH languages produce `call_sites` is deliberately
//! not restated here at all: that set grows one dispatch arm at a time
//! (`crates/engine/src/pipeline/fresh/call_sites.rs`), and a language list copied into this doc would
//! go stale on the very next arm. The per-environment SSOT is
//! `crates/engine/tests/rule_contracts/capability_matrix.rs`'s declared table.

use serde::{Deserialize, Serialize};
use zzop_core::{
    BoundStringLiteral, CallSite, ClassShapeFragment, ControllerPrefixRouteFragment, ImportMap,
    IoFacts, ProcedureRouterFragment, QueryCallSite, ReExport, RouterMountFragment, SourceSymbol,
    WrapperCallFragment, WrapperDefFragment,
};

/// One file's Common-IR slice, as produced by parse + per-file projection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileIrSlice {
    pub symbols: Vec<SourceSymbol>,
    /// `Some` (possibly empty) for files that participate in the TS dep graph; `None` for files that
    /// never do (e.g. Prisma / lexical-only) — mirrors `FileArtifact::imports`.
    pub imports: Option<ImportMap>,
    /// This file's re-exports (specifier + `type_only`) — mirrors `FileArtifact::re_exports`. Must
    /// round-trip through the cache: dropping it on a hit would silently undercount a re-exported-only
    /// barrel file's fan-in and re-introduce Defect A's `dead-candidates` false positive for that file on
    /// every subsequent cache-warm run.
    #[serde(default)]
    pub re_exports: Vec<ReExport>,
    /// This file's dynamic-`import()` specifiers — mirrors `FileArtifact::dynamic_imports`. Must
    /// round-trip through the cache: dropping it on a hit would silently undercount a code-split-only
    /// module's fan-in and re-introduce its `dead-candidates` false positive on every cache-warm run.
    #[serde(default)]
    pub dynamic_imports: Vec<String>,
    /// This file's runtime asset-URL references (raw path strings) — mirrors `FileArtifact::asset_refs`.
    /// Must round-trip through the cache: dropping it on a hit would silently undercount a `public/`-served
    /// worklet/worker's fan-in and re-introduce its `dead-candidates` false positive on every cache-warm
    /// run. Introduced with the `CACHE_SCHEMA_VERSION` v33->v34 bump, so no pre-v34 entry is served stale.
    #[serde(default)]
    pub asset_refs: Vec<String>,
    pub loc: u32,
    /// Whether this file's slice came from the lexical fallback path rather than a full structural
    /// parse — mirrors `FileArtifact::degraded`.
    pub degraded: bool,
    /// This file's projected HTTP-egress/route `IoFacts` — mirrors `FileArtifact::io`. Must
    /// round-trip through the cache: dropping it on a hit would silently hide IoFacts from
    /// `Matcher::IoScan` rules and the cross-layer linker for that file.
    #[serde(default)]
    pub io: Option<IoFacts>,
    /// Identifier names referenced anywhere in this file, sorted for deterministic serialization —
    /// mirrors `FileArtifact::used_names`. Feeds the unimported-export analysis's in-file-only-vs-unused
    /// distinction; empty for non-TypeScript/degraded files, same convention as `imports`.
    #[serde(default)]
    pub used_names: Vec<String>,
    /// Names appearing in the PUBLIC SIGNATURE (parameter/return/member type-annotation positions,
    /// never a body) of some exported declaration in this file, sorted for deterministic
    /// serialization — mirrors `FileArtifact::exported_signature_names`. Category 4 alongside
    /// `used_names`, and the position-aware companion that flat set cannot be: it lets
    /// `unimported-export` exempt a type belonging to an exported value's public API without exempting a
    /// type used only inside a body. TypeScript-only (a matrix blank elsewhere); empty means "no
    /// exemptions", which is the pre-existing behavior, so an absent value degrades gracefully.
    #[serde(default)]
    pub exported_signature_names: Vec<String>,
    /// Whether this file was classified minified/generated — mirrors `FileArtifact::minified_or_generated`.
    /// A stale entry defaulting to `false` would silently drop the DSL-skip warning, so a
    /// schema-version bump (not `#[serde(default)]`) forces re-parsing — see `CACHE_SCHEMA_VERSION`'s doc.
    #[serde(default)]
    pub minified_or_generated: bool,
    /// This file's constant-map fragment (dotted constant access -> value, from this file's own
    /// top-level `const` declarations) — mirrors `FileArtifact::const_map_fragment`. Can't be
    /// re-derived from `symbols`/`io` alone; same schema-version reasoning as `minified_or_generated`
    /// above applies.
    #[serde(default)]
    pub const_map_fragment: std::collections::HashMap<String, String>,
    /// This file's tRPC router-fragment shape — mirrors `FileArtifact::procedure_router_fragments`. Same
    /// round-trip-through-the-cache reasoning as `const_map_fragment` above.
    #[serde(default)]
    pub procedure_router_fragments: Vec<ProcedureRouterFragment>,
    /// The provide-side sibling of `procedure_router_fragments` — mirrors
    /// `FileArtifact::router_mount_fragments`. Same round-trip reasoning.
    #[serde(default)]
    pub router_mount_fragments: Vec<RouterMountFragment>,
    /// This file's wrapper-definition fragment shape — mirrors `FileArtifact::wrapper_def_fragments`,
    /// indexed by `(file, name)` for assemble-time wrapper-consume joins. Same round-trip reasoning.
    #[serde(default)]
    pub wrapper_def_fragments: Vec<WrapperDefFragment>,
    /// The consume-side sibling of `wrapper_def_fragments` (resolves via import specifier back to a
    /// def) — mirrors `FileArtifact::wrapper_call_fragments`. Same round-trip reasoning.
    #[serde(default)]
    pub wrapper_call_fragments: Vec<WrapperCallFragment>,
    /// This file's controller-prefix route fragment (`controller-prefix-ref-v1`) — mirrors
    /// `FileArtifact::controller_prefix_route_fragments`. Same round-trip reasoning: dropping it on a
    /// hit would silently drop a `@Controller(RouteKey.Asset)`-shaped controller's routes from every
    /// subsequent cache-warm run.
    #[serde(default)]
    pub controller_prefix_route_fragments: Vec<ControllerPrefixRouteFragment>,
    /// This file's class field-shape fragments (`body-shape-v1`) — mirrors
    /// `FileArtifact::class_shape_fragments`. Must round-trip through the cache: dropping it on a
    /// hit would silently starve `IoProvide::body.dto_ref` resolution of this file's DTO class
    /// declarations on every subsequent cache-warm run, same reasoning as
    /// `controller_prefix_route_fragments` above.
    #[serde(default)]
    pub class_shape_fragments: Vec<ClassShapeFragment>,
    /// This file's Prisma query-call-site facts (`<clientAccessor>().<model>.<method>(...)`) — mirrors
    /// `FileArtifact::query_call_sites`. Must round-trip through the cache: dropping it on a hit would
    /// silently starve the schema x usage JOIN rules (`soft-delete-bypass`/`orderby-unindexed`/
    /// `enum-string-drift`) of call-site evidence for this file, same reasoning as `io` above.
    #[serde(default)]
    pub query_call_sites: Vec<QueryCallSite>,
    /// This file's comment/string-stripped identifier tokens (`zzop_rules_schema::field_usage_tokens`) —
    /// mirrors `FileArtifact::field_usage_tokens`. Must round-trip through the cache: dropping it on a
    /// hit would silently starve the `schema-usage` native rule's `unreferenced-field-name` check of this file's
    /// usage evidence, same reasoning as `query_call_sites` above.
    #[serde(default)]
    pub field_usage_tokens: Vec<String>,
    /// This file's loop-body line spans (`zzop_parser_typescript::extract_loop_spans` /
    /// `zzop_parser_go::extract_loop_spans`) — mirrors
    /// `FileArtifact::loop_spans` / `zzop_core::dsl::SourceFile::loop_spans`. Must round-trip through the
    /// cache: dropping it on a hit would silently starve `Matcher::MethodScan::trigger_in_loop` of loop
    /// evidence for this file on every subsequent cache-warm run, same reasoning as `query_call_sites`
    /// above. `#[serde(default)]` so a pre-existing cache entry (written before this field existed)
    /// still deserializes, just with an empty vec rather than a hard cache-format break.
    #[serde(default)]
    pub loop_spans: Vec<(u32, u32)>,
    /// This file's function line spans with promise-continuation callbacks merged into their call site
    /// (`zzop_parser_typescript::extract_function_spans`) — mirrors `FileArtifact::function_spans` /
    /// `zzop_core::dsl::SourceFile::function_spans`. Category 2, alongside `loop_spans`. Must round-trip
    /// through the cache for the same reason: dropping it on a hit would silently starve
    /// `Matcher::MethodScan::after_in_same_function` of scope evidence, and because the absent-fact
    /// degrade for THAT gate is "no pairing removed", the loss would restore the exact false positives
    /// the fact exists to remove — a silent regression, not a silent skip. `#[serde(default)]` keeps a
    /// pre-existing entry deserializable; the `CACHE_SCHEMA_VERSION` bump is what actually prevents one
    /// from being served (see that constant's doc).
    #[serde(default)]
    pub function_spans: Vec<(u32, u32)>,
    /// This file's TEST-ONLY line spans (`zzop_parser_rust::extract_test_spans`) — mirrors
    /// `FileArtifact::test_spans` / `zzop_core::dsl::SourceFile::test_spans`. Category 2, and the only
    /// SUBTRACTIVE member of it: every other fact there lets a rule say MORE, this one stops every rule
    /// from speaking about a line the parser proved is compiled out of the shipping build. Must
    /// round-trip through the cache, and the consequence of dropping it is the LOUD direction rather than
    /// the quiet one — a warm run that lost it would resurrect every finding inside every `#[cfg(test)]
    /// mod tests` in the tree, which is 100% of what this repo's own `.rs` findings were before the fact
    /// existed. `#[serde(default)]` keeps a pre-existing entry deserializable; the `CACHE_SCHEMA_VERSION`
    /// bump is what actually prevents one from being served (see that constant's doc).
    #[serde(default)]
    pub test_spans: Vec<(u32, u32)>,
    /// This file's projected CALL SITES (`zzop_core::call_sites::CallSite`) — mirrors
    /// `FileArtifact::call_sites` / `zzop_core::dsl::SourceFile::call_sites`. **Category 2**, alongside
    /// `loop_spans`/`function_spans`/`test_spans`: it reaches `RuleContext` and is read directly by
    /// `Matcher::CallScan`, never by the cross-layer linker or an assemble-time composer.
    ///
    /// Must round-trip through the cache, and the degrade direction says how loudly: `CallScan`'s
    /// absent-fact behavior is SILENCE (the `loop_spans` family, not `function_spans`' no-op), so a warm
    /// run that dropped this field would report those rules as finding nothing — indistinguishable, in
    /// the output, from a clean tree. `#[serde(default)]` keeps a pre-existing entry deserializable; what
    /// actually prevents one from being SERVED is the derived `CACHE_SCHEMA_VERSION`, which hashes this
    /// crate's own sources (`crates/engine/build.rs`) and therefore moved the moment this field was
    /// added — no hand-held bump to forget.
    #[serde(default)]
    pub call_sites: Vec<CallSite>,
    /// This file's projected BOUND STRING LITERALS (`zzop_core::BoundStringLiteral`) — mirrors
    /// `FileArtifact::string_literals` / `zzop_core::dsl::SourceFile::string_literals`. **Category 2**,
    /// alongside `call_sites`, whose degrade direction (SILENCE) and round-trip reasoning apply
    /// verbatim: a warm run that dropped this field would report every `LiteralScan` rule as finding
    /// nothing, indistinguishable from a clean tree.
    ///
    /// What this field is allowed to contain is part of the channel's contract: name + line + value
    /// HASH + entropy, NEVER the literal's value — this is the cached, plain-text-JSON-on-disk slice
    /// the no-plaintext design in `zzop_core::string_literals`'s module doc exists to protect.
    /// `#[serde(default)]` keeps a pre-existing entry deserializable; what actually prevents one from
    /// being SERVED is the derived `CACHE_SCHEMA_VERSION` (hashes this crate's own sources), which
    /// moved the moment this field was added.
    #[serde(default)]
    pub string_literals: Vec<BoundStringLiteral>,
}
