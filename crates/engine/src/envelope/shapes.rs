//! Mode A's `body`/`response` DTO-shape resolution — the envelope-lane call site of the SAME
//! assemble-time passes the native path runs (`analyze::compose`'s `ShapeMerge` +
//! `resolve_provide_body_refs`/`resolve_provide_response_refs`, re-exported through
//! `crate::analyze`, never copied). Before this seam existed, a Mode A envelope's
//! `provide.body.dtoRef`/`provide.response.dtoRef` was never resolved against its own
//! `class_shape_fragments`, and the parser's no-return-type sentinel (`dtoRef: None` + empty
//! `fields`) leaked into `MinimalIr::io` — while the contract docs promised "resolved, stripped and
//! disclosed at assembly" to exactly this lane's producers.

use zzop_core::IoProvide;

/// Resolves every provide's `body.dto_ref`/`response.dto_ref` against the envelope's own merged
/// class shapes, strips + discloses the no-return-type sentinel, and discloses unresolved/poisoned
/// refs — one call, both passes, identical wording to the native lane (same functions). Runs at the
/// seam `ingest` documents: after every provide-composition pass, before the whole-tree `IoScan`
/// rules and the topology freeze read `io_provides`.
pub(super) fn resolve_shape_refs(
    io_provides: &mut [IoProvide],
    class_shape_pairs: &[(String, Vec<zzop_core::ClassShapeFragment>)],
    warnings: &mut Vec<String>,
) {
    let merge = crate::analyze::ShapeMerge::build(class_shape_pairs);
    crate::analyze::resolve_provide_body_refs(io_provides, &merge, warnings);
    crate::analyze::resolve_provide_response_refs(io_provides, &merge, warnings);
}
