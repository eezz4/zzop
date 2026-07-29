//! Which http provides are even candidates for `mutating-route-no-auth`.
//!
//! Split out of `super` on 2026-07-29 (line cap) along a seam the code already had: this is the whole
//! "do not guess" gate stack — test files, ecosystems the call-graph BFS has no evidence for, routes
//! already proven guarded by a decorator or an injected attribute, non-write verbs, and the
//! auth-acquisition surface. Reading them in one place is how a reader checks the rule's precision
//! claims; interleaved with the BFS they were five `.filter` calls in a chain that also did the work.

use zzop_core::is_test_file;

use super::vocab::AcquisitionSurface;
use super::{
    is_call_graph_covered, ScanMutatingRouteNoAuthInput, AUTH_GUARDED_ATTR, WRITE_HTTP_METHODS,
};

/// The mutating, unexempted http routes of this tree, in `io_provides` order.
pub(super) fn mutating_route_candidates<'a>(
    input: &'a ScanMutatingRouteNoAuthInput,
    acquisition: &AcquisitionSurface,
) -> Vec<&'a zzop_core::IoProvide> {
    input
        .io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter(|p| !is_test_file(&p.file))
        // The call-graph BFS below has zero evidence for a non-TS/JS ecosystem — module doc "Call-graph
        // language coverage". Exempt before resolving/BFS-ing, the same "do not guess" spirit as the
        // unresolved/ambiguous-handler skip.
        .filter(|p| is_call_graph_covered(&p.file))
        .filter(|p| !input.decorator_guarded.contains(&(p.file.clone(), p.line)))
        // Injected auth-guard evidence (route-level middleware the call-graph BFS can't see) — see
        // `AUTH_GUARDED_ATTR`. Exempt BEFORE the BFS, like `decorator_guarded`: this IS how the route is guarded.
        .filter(|p| {
            !input
                .route_attr_store
                .route_attr(&p.kind, &p.key, AUTH_GUARDED_ATTR)
                .is_some_and(zzop_core::attr_is_truthy)
        })
        .filter(|p| {
            let Some((method, path)) = p.key.split_once(' ') else {
                return false;
            };
            // The auth-acquisition surface itself is exempt — see module doc.
            WRITE_HTTP_METHODS.contains(&method) && !acquisition.exempts(path)
        })
        .collect()
}
