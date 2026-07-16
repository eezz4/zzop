//! Cross-layer IO — joins what one tree CONSUMES to what another tree PROVIDES, on a normalized contract key.
//! Not AST matching: a `(kind, key)` exact join, with three integrity gates layered on top of the raw join
//! (each verified against real false-positive vectors, not speculative):
//! - **Ambiguity**: a consume whose key is provided by 2+ DISTINCT source trees is not auto-linked — it goes
//!   to [`CrossLayerResult::ambiguous_consumes`] with every candidate listed, rather than emitting a many-to-many
//!   edge fan-out that silently picks a "winner". Providers all within ONE tree are unaffected (legal
//!   multi-provider case, e.g. one tree exposing a topic twice) and still join exactly like before.
//! - **External egress**: a consume whose key carries a host (`"://"` present — an absolute URL an adapter
//!   preserved) never cross-tree joins; it goes to [`CrossLayerResult::external_consumes`] instead of
//!   `unprovidedConsumes`, since it is third-party egress, not drift. An absolute-URL consume whose
//!   authority matches a declared [`LinkOptions::internal_hosts`] entry is re-keyed to its path and
//!   exempted from this gate FIRST — deployment topology (a tree calling its own gateway host by its
//!   public name) is a same-deployment call, not egress.
//! - **Low confidence**: an edge whose key matches an injected pattern (generic paths like `/health` that many
//!   unrelated services legitimately share) is still emitted, but tagged with
//!   [`CrossLayerEdge::low_confidence_reason`] so a consumer can discount it. The pattern table itself is
//!   injected via [`LinkOptions`] — core carries no default vocabulary (see `zzop_metrics::
//!   default_generic_interface_key_patterns` for the shipped table).
//!
//! Each adapter projects its IO to a normalized key with full local context; the linker is then a dumb exact
//! join (no AST-level heuristics). Even a crude parser (e.g. JSP) joins as a first-class citizen as long as it
//! emits accurate IoFacts.
//!
//! Module layout (every public item stays importable at `crate::io::X` — the paths below are internal):
//! - [`facts`](self) — the serde wire types ([`IoFacts`], provides/consumes, cross-layer result buckets).
//! - [`key`](self) — pinned HTTP interface-key normalization ([`http_interface_key`] and friends).
//! - [`link`](self) — [`link_cross_layer_io`] and its [`LinkOptions`].

mod facts;
mod key;
mod link;

pub use facts::{
    AmbiguousConsume, ConsumeBodyShape, CrossLayerEdge, CrossLayerResult, EdgeFrom, EdgeTo,
    IoConsume, IoFacts, IoKind, IoProvide, ProvideBodyField, ProvideBodyShape, SourceIo,
    TaggedConsume, TaggedProvide,
};
pub use key::{
    http_consume_interface_key, http_interface_key, normalize_http_path, HTTP_KEY_VERBS,
};
pub use link::{link_cross_layer_io, LinkOptions};
