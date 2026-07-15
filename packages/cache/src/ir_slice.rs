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

use serde::{Deserialize, Serialize};
use zzop_core::{
    ClassShapeFragment, ControllerPrefixRouteFragment, ImportMap, IoFacts, ProcedureRouterFragment,
    QueryCallSite, ReExport, RouterMountFragment, SourceSymbol, WrapperCallFragment,
    WrapperDefFragment,
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
    /// mirrors `FileArtifact::used_names`. Feeds the dead-exports analysis's in-file-only-vs-unused
    /// distinction; empty for non-TypeScript/degraded files, same convention as `imports`.
    #[serde(default)]
    pub used_names: Vec<String>,
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
    /// hit would silently starve the `schema-usage` native rule's `dead-field` check of this file's
    /// usage evidence, same reasoning as `query_call_sites` above.
    #[serde(default)]
    pub field_usage_tokens: Vec<String>,
    /// This file's loop-body line spans (`zzop_parser_typescript::extract_loop_spans`) — mirrors
    /// `FileArtifact::loop_spans` / `zzop_core::dsl::SourceFile::loop_spans`. Must round-trip through the
    /// cache: dropping it on a hit would silently starve `Matcher::MethodScan::trigger_in_loop` of loop
    /// evidence for this file on every subsequent cache-warm run, same reasoning as `query_call_sites`
    /// above. `#[serde(default)]` so a pre-existing cache entry (written before this field existed)
    /// still deserializes, just with an empty vec rather than a hard cache-format break.
    #[serde(default)]
    pub loop_spans: Vec<(u32, u32)>,
}
