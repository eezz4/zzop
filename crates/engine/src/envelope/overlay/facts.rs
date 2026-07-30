//! The overlay fact-census predicate — "does this projection contribute anything the merge acts on?"
//!
//! Split out of `overlay.rs` for the 300-line ceiling, on the same seam as `reports`: a pure judgment
//! on a `FileProjection`, touching no artifact. Its verdict feeds two consumers that must not drift
//! apart (the zero-fact census and the covered-path set), which is exactly why it is one function.

use super::super::reserved::{is_reserved_consume_kind, is_reserved_provide_kind};
/// True iff `file` contributes at least one extraction FACT that an overlay merge actually acts on —
/// non-empty non-reserved `io.provides`/`io.consumes`, `imports`, `re_exports`, `dynamic_imports`, any
/// fragment channel (`const_map_fragment`, `procedure_router_fragments`, `router_mount_fragments`,
/// `class_shape_fragments`), a non-empty per-file `attributes` (the channel already lives on
/// `FileProjection` itself, one array per file — not envelope-level — so "this projection's own
/// `attributes` is non-empty" is already the precise per-file rule, no cross-referencing an `EntityRef`
/// target needed), or `is_entry == true`. `path`/`loc`/`degraded` are metadata, not facts.
///
/// The io checks skip reserved engine-internal sentinel kinds (the same set `drop_reserved_io` strips
/// before the merge), so the predicate judges a RAW projection and a cleaned one identically — the two
/// call sites can safely feed it different pre-processing stages without drifting.
///
/// `symbols` is deliberately EXCLUDED: Mode B's merge never consumes overlay symbols
/// (`merge_projection_onto_artifact` does not touch the field and `synthetic_artifact_from_projection`
/// sets it empty), so counting it would call a file "covered" for data the engine silently drops — a
/// symbols-only overlay must instead trip the zero-fact census and keep the "no native parser"
/// disclosure alive. `used_names`, `loop_spans`, and `function_spans` are excluded for the same reason:
/// none is read by either merge branch in a way that reaches an actual consumer today.
///
/// Called from exactly one place — [`apply_adapter_overlays`]'s per-projection loop — where its verdict
/// feeds BOTH the per-overlay zero-fact census (G8b) and the returned covered-path set `analyze::assemble`
/// uses to exclude a file from the "no native parser" per-extension disclosure (G8's unmasking half). One
/// rule, one evaluation, so the two can never drift apart.
pub(super) fn overlay_file_carries_facts(file: &zzop_core::FileProjection) -> bool {
    file.io
        .provides
        .iter()
        .any(|p| !is_reserved_provide_kind(&p.kind))
        || file
            .io
            .consumes
            .iter()
            .any(|c| !is_reserved_consume_kind(&c.kind))
        || !file.imports.is_empty()
        || !file.re_exports.is_empty()
        || !file.dynamic_imports.is_empty()
        || !file.const_map_fragment.is_empty()
        || !file.procedure_router_fragments.is_empty()
        || !file.router_mount_fragments.is_empty()
        || !file.class_shape_fragments.is_empty()
        || !file.attributes.is_empty()
        || file.is_entry
}
