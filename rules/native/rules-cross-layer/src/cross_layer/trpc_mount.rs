//! tRPC mount-route detection and its suppression-disclosure notes — shared by
//! `unconsumed_endpoint`/`unconsumed_mutation_endpoint` (the suppression side) and
//! `zzop_engine::analyze_trees` (the disclosure side). Moved verbatim from the module root
//! (`cross_layer/mod.rs`) purely for file-size layout; re-exported there so every existing
//! `super::`/crate-root path still resolves.
//!
//! ## Why suppression happens at TWO sites (intentional — do not "unify")
//! A tRPC mount surfaces as TWO distinct provide SHAPES, hitting TWO different findings, so its
//! transport-noise suppression is necessarily applied at each finding's own site:
//!
//! - an **explicit-verb** mount (`GET /api/trpc/{}`, app-router `route.ts` `export const GET`) is a real
//!   `http` provide that would fire `cross-layer/unconsumed-endpoint` — suppressed there via
//!   [`is_trpc_mount_route_key`].
//! - a **serve-all** mount (`? /api/trpc/{}`, pages-router `[trpc].ts`) is a `zzop_core::UNKNOWN_VERB`
//!   sentinel that would fire `cross-layer/unknown-verb-route` — suppressed in the engine's verb-unknown
//!   partition via [`is_trpc_mount_route_path`].
//!
//! The DETECTION (`is_trpc_mount_route_path`) and the user-facing DISCLOSURE
//! ([`trpc_mount_route_suppression_notes`], which reads the RAW unconsumed list so a single note covers
//! BOTH shapes) are already shared — there is no duplication to remove. Suppressing at the SOURCE (the
//! `file_routes` convention scan that mints the mount provide) is impossible: that scan is content-blind
//! and cannot know the tree participates in `trpc` edges — a whole-tree fact only available at the
//! cross-layer stage. So the two-site shape is inherent, not incidental.

use super::{path_segments, split_key};

/// An `http` provide whose path carries a literal `trpc` segment (`/api/trpc/{}`, `/trpc/{}`, ...) —
/// the shape `file_routes`'s Next.js `pages/api/**`/app-router conventions produce for a tRPC adapter
/// mount file (`createNextApiHandler`/`fetchRequestHandler`, ...).
///
/// **Why string-based, not structural**: `compose_trpc_provides` (the engine's assembly pass) composes
/// `trpc`-kind PROVIDEs from each file's own `ProcedureRouterFragment` — the router-definition file(s). The
/// mount file's `http`-kind PROVIDE comes from an entirely separate, content-blind pass
/// (`zzop_engine::file_routes`'s pure filesystem-path convention scan): it never reads what the file's
/// default export actually calls, so there is no shared file/symbol/import-edge between a `trpc` PROVIDE
/// and the `http` PROVIDE naming its mount route to key off of. The literal `trpc` path segment is the
/// narrowest real signal this analysis has. It is deliberately gated by callers on "THIS TREE participates
/// in at least one `trpc`-kind edge, on either side" (see [`super::unconsumed_endpoint::unconsumed_endpoint_findings`]/
/// [`super::unconsumed_mutation_endpoint::unconsumed_mutation_endpoint_findings`]'s `trpc_participating_sources`
/// parameter) before a match is treated as a real mount — the segment alone cannot rule out a
/// coincidentally-named route in a codebase with no tRPC at all. Per-tree, not run-global: a run-global
/// `trpc_edge_count >= 1` gate would suppress a literal `/trpc/`-segment route in tree B purely because
/// SOME OTHER tree in the run has tRPC edges, even though tree B has none of its own — the mount-IS-transport
/// justification only holds for the tree whose own edges actually flow through that route.
pub(crate) fn is_trpc_mount_route_key(key: &str) -> bool {
    split_key(key).is_some_and(|(_, path)| is_trpc_mount_route_path(path))
}

/// The path half of [`is_trpc_mount_route_key`]: true when a route PATH carries a literal `trpc` segment.
/// Exposed for the engine's verb-unknown disclosure partition — a serve-all tRPC mount route is now a
/// `zzop_core::UNKNOWN_VERB` sentinel rather than a fabricated GET/POST provide, so its transport nature is
/// suppressed from `cross-layer/unknown-verb-route` the same per-tree way `unconsumed-endpoint` suppresses
/// an explicit-verb mount (gated by the caller on the tree participating in a `trpc` edge).
pub fn is_trpc_mount_route_path(path: &str) -> bool {
    path_segments(path)
        .iter()
        .any(|seg| seg.eq_ignore_ascii_case("trpc"))
}

/// One line per source tree that has 1+ `http` unconsumed provide [`is_trpc_mount_route_key`] identifies
/// as a tRPC mount route, suppressed from `cross-layer/unconsumed-endpoint`/
/// `cross-layer/unconsumed-mutation-endpoint` reporting because THAT TREE participates in at least one
/// `trpc`-kind edge (on either side — `trpc_edge_counts_by_source`, keyed by source id) — the mount route
/// IS the transport those edges flow through, so reporting it unconsumed is tone noise, not signal
/// (dogfood round 9: a fully-joined tRPC starter's only "findings" were its own GET/POST mount routes).
/// A source with no entry in `trpc_edge_counts_by_source` contributes nothing here — no edges means no
/// evidence the segment match is a real mount FOR THAT TREE, so nothing is suppressed and this function has
/// nothing to disclose for it (see [`is_trpc_mount_route_key`]'s doc for the per-tree gating rationale;
/// this was a run-global `trpc_edge_count: usize` gate before — a tree with zero tRPC edges of its own
/// would still have its literal `/trpc/`-segment routes suppressed, and misattributed, purely because some
/// OTHER tree in the run had tRPC edges).
///
/// This is the disclosure half of the suppression (`output-philosophy.md` §0/§1: no silent suppression —
/// a finding a rule would otherwise emit must never simply vanish). Returned as `(source, note)` pairs,
/// sorted by source, for the caller (`zzop_engine::analyze_trees`) to push onto that source tree's own
/// `AnalyzeOutput::warnings` — the same per-tree self-report channel every other engine-level silent-
/// failure disclosure uses. Each note's edge count is THAT SOURCE's own participating-edge count
/// (`trpc_edge_counts_by_source[source]`), never the run-global total.
pub fn trpc_mount_route_suppression_notes(
    unconsumed_provides: &[zzop_core::io::TaggedProvide],
    trpc_edge_counts_by_source: &std::collections::BTreeMap<String, usize>,
) -> Vec<(String, String)> {
    let mut by_source: std::collections::BTreeMap<String, Vec<&str>> =
        std::collections::BTreeMap::new();
    for p in unconsumed_provides {
        // Same test-file filter the two unconsumed rules apply BEFORE suppression: a test-file provide
        // was never a candidate finding, so counting it here would disclose a suppression that never
        // happened (a phantom note is its own honesty defect). Gated per-tree on the provide's OWN source
        // having 1+ trpc edge — a provide in a tree with zero trpc edges of its own was never suppressed
        // (see this fn's doc), so it must not be counted here either.
        if p.provide.kind == "http"
            && trpc_edge_counts_by_source.contains_key(&p.source)
            && is_trpc_mount_route_key(&p.provide.key)
            && !zzop_core::is_test_file(&p.provide.file)
        {
            by_source
                .entry(p.source.clone())
                .or_default()
                .push(p.provide.key.as_str());
        }
    }
    by_source
        .into_iter()
        .map(|(source, mut keys)| {
            keys.sort_unstable();
            keys.dedup();
            let n = keys.len();
            let (route_word, pronoun) = if n == 1 {
                ("route", "its")
            } else {
                ("routes", "their")
            };
            // Present (non-empty by construction: `by_source` only gains an entry via the `contains_key`
            // filter above, so `source` is always a key of `trpc_edge_counts_by_source`).
            let trpc_edge_count = trpc_edge_counts_by_source[&source];
            let edge_word = if trpc_edge_count == 1 { "edge" } else { "edges" };
            // Display the PATH for a verb-unknown sentinel mount (`"? /api/trpc/{}"` -> `/api/trpc/{}`) so
            // the internal `UNKNOWN_VERB` method never surfaces in a user-facing warning; an explicit-verb
            // mount key (`GET /api/trpc/{}`) is shown verbatim.
            let display: Vec<&str> = keys
                .iter()
                .map(|&k| zzop_core::unknown_verb_route_path(k).unwrap_or(k))
                .collect();
            let note = format!(
                "{n} tRPC mount {route_word} ({}) treated as consumed by {trpc_edge_count} tRPC {edge_word} — {pronoun} HTTP surface is the tRPC transport",
                display.join(", ")
            );
            (source, note)
        })
        .collect()
}
