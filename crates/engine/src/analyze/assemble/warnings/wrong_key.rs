//! S12/S13/S14 — the WRONG-KEY tripwire family, split out of the parent registration list when it hit
//! the repo's 300-line source cap. The seam is the one the parent's own comments already drew: every
//! other tripwire there reports a SILENCE (a channel that came up empty), while these three report a
//! plausible-looking FINDING produced by a prefix this build never read — a route keyed without its
//! gateway rewrite, without its C# base-class route, or without its Python mount prefix, which the
//! cross-layer join then reports as an unprovided consume for a route that is actually served. All
//! three are `Option<String>` self-reports like their siblings and none sets a side alarm, which is
//! why they lift out without changing what the parent decides.

use std::collections::HashMap;

use zzop_core::IoProvide;

/// Every wrong-key self-report that fired, in the parent's own registration order.
pub(super) fn wrong_key_warnings(
    root: &std::path::Path,
    io_provides: &[IoProvide],
    csharp_rels: &[String],
    loc_by_path: &HashMap<String, u32>,
) -> Vec<String> {
    let mut out = Vec::new();
    // S12 — unread gateway-declaration self-report. The one gap here whose symptom is a PLAUSIBLE
    // FINDING rather than a silence: a rewrite zzop never read keys the provide side pre-rewrite while
    // the consume side calls the post-rewrite path, so the join reports an unprovided consume for a
    // route that is actually served. Content-gated and route-gated (see its module doc), and it touches
    // disk, so it is gated on the tree having http provides at all.
    let http_provides = io_provides.iter().filter(|p| p.kind == "http").count();
    if let Some(w) = crate::framework_silence::gateway_declaration_warning(root, http_provides) {
        out.push(w);
    }

    // S13 — inherited C# route prefix. Same WRONG-KEY family as S12 rather than the silence family:
    // a controller deriving its prefix from a project base class is keyed without it.
    if let Some(w) = crate::framework_silence::csharp_base_route_warning(root, csharp_rels) {
        out.push(w);
    }

    // S14 — unread Python router-mount prefix. Third language in the WRONG-KEY family, and the one
    // with a measured cost: on the 17-tree corpus join, 22 of 24 unprovided consumes were one skipped
    // `prefix=settings.API_V1_STR`. Reads `.py` paths off `loc_by_path` rather than taking a new
    // parameter — every walked file is already there, and a fourth per-language rel list threaded
    // through `collect` for one lexical scan is plumbing nobody would keep in sync.
    let mut py_rels: Vec<String> = loc_by_path
        .keys()
        .filter(|p| p.ends_with(".py"))
        .cloned()
        .collect();
    py_rels.sort(); // HashMap iteration order is not stable; the message names examples.
    if let Some(w) =
        crate::framework_silence::python_mount_prefix_warning(root, &py_rels, http_provides)
    {
        out.push(w);
    }
    out
}
