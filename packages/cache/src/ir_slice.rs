//! `FileIrSlice` — the cacheable half of the engine's fused per-file pass output.
//!
//! Shape-equivalent to `zpz_engine::pipeline::FileArtifact` minus `rel` (the lookup key's business)
//! and `findings` (a separate cache entry — see `CacheKey`). Defined here, not in `zpz-engine`, so
//! this crate stays a leaf dependent of `zpz-core` only.
//!
//! `trpc_router_fragments` / `router_mount_fragments` / `wrapper_def_fragments` /
//! `wrapper_call_fragments` round-trip the matching `zpz_core` types verbatim; those types live in
//! `zpz-core` (not the TypeScript parser crate that produces them) so this crate never needs
//! `zpz-parser-typescript` as a dependency.

use serde::{Deserialize, Serialize};
use zpz_core::{
    ImportMap, IoFacts, RouterMountFragment, SourceSymbol, TrpcRouterFragment, WrapperCallFragment,
    WrapperDefFragment,
};

/// One file's Common-IR slice, as produced by parse + per-file projection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileIrSlice {
    pub symbols: Vec<SourceSymbol>,
    /// `Some` (possibly empty) for files that participate in the TS dep graph; `None` for files that
    /// never do (e.g. Prisma / lexical-only) — mirrors `FileArtifact::imports`.
    pub imports: Option<ImportMap>,
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
    /// This file's tRPC router-fragment shape — mirrors `FileArtifact::trpc_router_fragments`. Same
    /// round-trip-through-the-cache reasoning as `const_map_fragment` above.
    #[serde(default)]
    pub trpc_router_fragments: Vec<TrpcRouterFragment>,
    /// The provide-side sibling of `trpc_router_fragments` — mirrors
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
}
