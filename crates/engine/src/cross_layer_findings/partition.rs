//! Verb-unknown route partition: lifts `zzop_core::UNKNOWN_VERB` sentinel provides (`"? <path>"`,
//! minted by a `pages/api` serve-all handler / pathname-dispatch / Go `HandleFunc` block that names
//! no method literal) OUT of the exact-key cross-layer join into a path-level "served, verb-unknown"
//! signal. Two consumers here:
//!
//! 1. [`http_provide_sites`] drops sentinels so `cross-layer/unconsumed-endpoint` never reports one
//!    as a dead route (a sentinel carries no witnessed method — it is not a real exact-key provide).
//! 2. [`without_verb_unknown`] drops a consume whose PATH is served by such a route, so the
//!    near-miss / unprovided family does not false-fire on a method we simply cannot see (the endpoint
//!    IS served, only its verb is unknown — reporting it "unprovided" would trade the old fabrication
//!    FP for a new one).
//!
//! The routes themselves surface via `cross-layer/unknown-verb-route` (an honest disclosure + inject
//! pointer, [`super::partition::verb_unknown_sites`] -> `zzop_rules_cross_layer::unknown_verb_route_findings`),
//! replacing the former per-adapter `[GET, POST]` fabrication.

use std::collections::{BTreeSet, HashSet};

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::SourceIo;
use zzop_rules_cross_layer::{HttpProvideSite, UnknownVerbRouteSite};

/// Every `UNKNOWN_VERB` sentinel provide across all trees, as a rule-crate `UnknownVerbRouteSite`
/// (source/normalized-path/file/line — the anchor the disclosure finding needs).
pub(super) fn verb_unknown_sites(source_ios: &[SourceIo]) -> Vec<UnknownVerbRouteSite> {
    source_ios
        .iter()
        .flat_map(|s| {
            s.io.provides
                .iter()
                .filter(|p| p.kind == "http")
                .filter_map(move |p| {
                    zzop_core::unknown_verb_route_path(&p.key).map(|path| UnknownVerbRouteSite {
                        source: s.source.clone(),
                        path: path.to_string(),
                        file: p.file.clone(),
                        line: p.line,
                    })
                })
        })
        .collect()
}

/// The normalized paths served by a verb-unknown route — the consume-suppression set.
pub(super) fn served_path_set(sites: &[UnknownVerbRouteSite]) -> HashSet<String> {
    sites.iter().map(|s| s.path.clone()).collect()
}

/// Verb-unknown sites to DISCLOSE (`cross-layer/unknown-verb-route`): all EXCEPT a tRPC mount route
/// (`/api/trpc/{}`) in a tree that participates in a `trpc` edge — that is transport plumbing already
/// modeled by the trpc procedure channel, suppressed here the same per-tree way `unconsumed-endpoint`
/// suppresses an explicit-verb mount (and disclosed via the tree's tRPC-mount note, since the sentinel is
/// also an unconsumed provide). A coincidental `trpc`-segment route in a no-edge tree stays disclosed.
pub(super) fn disclosure_sites(
    sites: &[UnknownVerbRouteSite],
    trpc_participating: &BTreeSet<String>,
) -> Vec<UnknownVerbRouteSite> {
    sites
        .iter()
        .filter(|s| {
            !(zzop_rules_cross_layer::is_trpc_mount_route_path(&s.path)
                && trpc_participating.contains(&s.source))
        })
        .cloned()
        .collect()
}

/// `http` provide sites across all trees, EXCLUDING `UNKNOWN_VERB` sentinels (which hold no witnessed
/// method, so they are not exact-key provides and must never be counted as unconsumed dead routes).
pub(super) fn http_provide_sites(source_ios: &[SourceIo]) -> Vec<HttpProvideSite> {
    source_ios
        .iter()
        .flat_map(|s| {
            s.io.provides
                .iter()
                .filter(|p| {
                    p.kind == "http" && zzop_core::unknown_verb_route_path(&p.key).is_none()
                })
                .map(move |p| HttpProvideSite {
                    source: s.source.clone(),
                    key: p.key.clone(),
                    file: p.file.clone(),
                    line: p.line,
                })
        })
        .collect()
}

/// `unconsumed_provides` with `UNKNOWN_VERB` sentinels removed — a sentinel is a verb-unknown route
/// disclosed via `cross-layer/unknown-verb-route`, never a "dead route" for the unconsumed rules to
/// report (its `"? <path>"` key must never surface in a finding message). The engine keeps the RAW list
/// for the tRPC mount-suppression note, which still counts a sentinel tRPC mount as transport.
pub(super) fn without_verb_unknown_provides(provides: &[TaggedProvide]) -> Vec<TaggedProvide> {
    provides
        .iter()
        .filter(|p| zzop_core::unknown_verb_route_path(&p.provide.key).is_none())
        .cloned()
        .collect()
}

/// The subset of `unprovided_consumes` whose PATH is NOT served by any verb-unknown route. A consume
/// on such a path is not really unprovided — the route serves it, we just cannot see which method — so
/// dropping it here is what prevents the fabrication FP from reappearing as a near-miss / unprovided FP.
pub(super) fn without_verb_unknown(
    unprovided: &[TaggedConsume],
    served: &HashSet<String>,
) -> Vec<TaggedConsume> {
    unprovided
        .iter()
        .filter(|c| {
            c.consume
                .key
                .as_deref()
                .and_then(|k| k.split_once(' '))
                .map(|(_, path)| !served.contains(path))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}
