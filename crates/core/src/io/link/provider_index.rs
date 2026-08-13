//! The PROVIDE side of the join, built once before any consume is looked at — see [`build`].
//!
//! The seam is provide-side vs consume-side, not a line count. Everything here is a property of the
//! provider SET alone and is computed before the consume loop precisely because it does not depend on
//! which consume is asking: which keys are indexed, which of them span two trees (ambiguity), and which
//! routes are patterns rather than keys (the wildcard partition). The consume loop in the parent then
//! only ever READS these three, which is what keeps "is this ambiguous?" from being re-derived per
//! consume and drifting.

use std::collections::{BTreeMap, HashSet};

use super::super::facts::{SourceIo, TaggedProvide, WildcardRoutePartition};
use super::super::wildcard::wildcard_route_path;

/// Everything the consume loop needs to know about the provide side.
pub(super) struct ProviderIndex {
    /// Providers by `id_key(kind, key)`. Multiple providers for one key is legal (e.g. two services
    /// expose one topic) — the ambiguity gate below decides what that means, not this map.
    pub(super) by_key: BTreeMap<String, Vec<TaggedProvide>>,
    /// Keys whose providers span 2+ DISTINCT source trees — ambiguous, never auto-linked. Computed once
    /// here (source-tree spread is a property of the key's provider set, independent of which consume is
    /// looking it up), then consulted per-consume. NOTE: this set alone must NOT drive the
    /// `unconsumed_provides` exclusion — a multi-tree key nobody consumes is still dead; only keys an
    /// actual consume referenced ambiguously are exempt (the parent tracks those separately in
    /// `ambiguously_consumed_keys`).
    pub(super) ambiguous_keys: HashSet<String>,
    /// `http` routes whose path is an ANT PATTERN (`GET /api/files/**`), lifted out of [`by_key`] rather
    /// than indexed: such a route can never be an exact-key provider, so indexing it would make it a dead
    /// route the moment nobody spells `**` back (see [`crate::io::wildcard`]'s module doc for the three
    /// false findings that produced). Sorted by `(source, key, file, line)` — the order the disclosure
    /// downstream renders in, and the order the consume loop's "charge the FIRST matching route" rule is
    /// defined against, so the charge cannot depend on tree input order.
    ///
    /// [`by_key`]: ProviderIndex::by_key
    pub(super) wildcard_routes: Vec<WildcardRoutePartition>,
}

/// Builds the provide-side index over every tree's provides. `id_key` is the parent's `(kind, key)`
/// composer, passed in rather than duplicated so both sides of the join key on the same bytes.
pub(super) fn build(trees: &[SourceIo], id_key: fn(&str, &str) -> String) -> ProviderIndex {
    let mut by_key: BTreeMap<String, Vec<TaggedProvide>> = BTreeMap::new();
    let mut wildcard_routes: Vec<WildcardRoutePartition> = Vec::new();
    for SourceIo { source, io } in trees {
        for p in &io.provides {
            if p.kind == "http" && wildcard_route_path(&p.key).is_some() {
                wildcard_routes.push(WildcardRoutePartition {
                    source: source.clone(),
                    key: p.key.clone(),
                    file: p.file.clone(),
                    line: p.line,
                    covered_consumes: 0,
                });
                continue;
            }
            by_key
                .entry(id_key(&p.kind, &p.key))
                .or_default()
                .push(TaggedProvide {
                    source: source.clone(),
                    provide: p.clone(),
                });
        }
    }
    wildcard_routes.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.key.cmp(&b.key))
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });

    let ambiguous_keys: HashSet<String> = by_key
        .iter()
        .filter(|(_, providers)| {
            providers
                .iter()
                .map(|p| p.source.as_str())
                .collect::<HashSet<_>>()
                .len()
                >= 2
        })
        .map(|(k, _)| k.clone())
        .collect();

    ProviderIndex {
        by_key,
        ambiguous_keys,
        wildcard_routes,
    }
}
