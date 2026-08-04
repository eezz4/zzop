//! The cross-layer linker: an exact `(kind, key)` join of trees' IO with the ambiguity /
//! external-egress / route-identity / low-confidence gates documented in the [`crate::io`] module doc.
//! The structural gate sequence in front of the join — no-key, deployment-topology host re-key, and the
//! external-egress gate that must follow it — is [`consume_join::classify_consume_join`], shared verbatim
//! with the single-tree `http/unprovided-consume` rule; the route-identity predicate (asked only of a
//! provider MISS, so it is not part of that sequence) is [`super::key::key_carries_route_identity`].

use std::collections::BTreeMap;

use super::facts::{
    AmbiguousConsume, CrossLayerEdge, CrossLayerResult, EdgeFrom, EdgeTo, IoConsume, SourceIo,
    TaggedConsume, TaggedProvide,
};
use super::key::key_carries_route_identity;

mod consume_join;
mod host_rekey;
pub use consume_join::{classify_consume_join, ConsumeJoin};

/// Injectable options for [`link_cross_layer_io`]. Mirrors `zzop_git::CollectOptions::commit_type_patterns`'s
/// mechanism/vocabulary split: this crate owns the injectable mechanism (matching a compiled pattern against
/// an edge's key), never the default pattern table itself — that vocabulary (which paths count as "generic")
/// lives in `zzop_metrics::default_generic_interface_key_patterns`, injected by the engine call site.
#[derive(Debug, Clone, Default)]
pub struct LinkOptions {
    /// `(pattern, reason)` pairs, checked in order; the first pattern whose regex matches an edge's key
    /// sets that edge's `low_confidence_reason` to the paired reason string. Empty by default — no edge is
    /// ever marked low-confidence unless a caller injects a table.
    pub low_confidence_key_patterns: Vec<(regex::Regex, String)>,
    /// Hosts owned by an analyzed tree (config-declared deployment topology, `EngineConfig::hosts` at the
    /// engine layer). A consume whose key carries `scheme://host` with a matching host is re-keyed to its
    /// path (internal) BEFORE the `` `://` `` external-egress gate, so it can join that tree's provides —
    /// see [`link_cross_layer_io`]'s doc for exactly where this sits relative to that gate.
    ///
    /// Matching: ascii-case-insensitive host; port is ignored on the CONSUME side unless the declared
    /// host string itself carries a port, in which case the match requires an exact `host:port`. Only
    /// `http`/`https` consume-key schemes are eligible — `ws`/`wss` (and anything else) stay external in
    /// v1, since a websocket URL is not an HTTP route key `http_consume_interface_key` can normalize.
    /// Empty by default — no consume is ever re-keyed unless a caller injects hosts.
    pub internal_hosts: Vec<String>,
}

/// Exact join of trees' IO on (kind, key), with the ambiguity/external/route-identity/low-confidence
/// gates documented in this module's doc. Pure function (given `opts`).
pub fn link_cross_layer_io(trees: &[SourceIo], opts: &LinkOptions) -> CrossLayerResult {
    // Index providers by (kind, key). Multiple providers for one key is legal (e.g. two services expose one topic).
    let mut providers_by_key: BTreeMap<String, Vec<TaggedProvide>> = BTreeMap::new();
    for SourceIo { source, io } in trees {
        for p in &io.provides {
            providers_by_key
                .entry(id_key(&p.kind, &p.key))
                .or_default()
                .push(TaggedProvide {
                    source: source.clone(),
                    provide: p.clone(),
                });
        }
    }

    // Keys whose providers span 2+ DISTINCT source trees — ambiguous, never auto-linked. Computed once
    // over `providers_by_key` (source-tree spread is a property of the key's provider set, independent of
    // which consume is looking it up), then consulted per-consume below. NOTE: this set alone must NOT
    // drive the `unconsumed_provides` exclusion — a multi-tree key nobody consumes is still dead; only keys
    // an actual consume referenced ambiguously are exempt (tracked separately in `ambiguously_consumed_keys`).
    let ambiguous_keys: std::collections::HashSet<String> = providers_by_key
        .iter()
        .filter(|(_, providers)| {
            providers
                .iter()
                .map(|p| p.source.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len()
                >= 2
        })
        .map(|(k, _)| k.clone())
        .collect();

    let mut edges = Vec::new();
    let mut unprovided_consumes = Vec::new();
    let mut unresolved_consumes = Vec::new();
    let mut external_consumes = Vec::new();
    let mut ambiguous_consumes = Vec::new();
    let mut consumed_keys = std::collections::HashSet::new();
    let mut ambiguously_consumed_keys = std::collections::HashSet::new();
    // One entry per DISTINCT declared host, in `opts.internal_hosts`' own order (deduped defensively here
    // too, even though the engine call site already dedups before injecting) — see
    // `CrossLayerResult::host_rekey_counts`'s doc.
    let mut host_rekey_counts: Vec<(String, usize)> = Vec::new();
    for h in &opts.internal_hosts {
        if !host_rekey_counts.iter().any(|(hh, _)| hh == h) {
            host_rekey_counts.push((h.clone(), 0));
        }
    }

    for SourceIo { source, io } in trees {
        for c in &io.consumes {
            // The structural gate sequence (no-key -> host re-key -> `"://"` egress), run by the one
            // shared function so its ORDER cannot drift between this join and the single-tree rule.
            // On a re-key, downstream buckets must carry the JOIN key, not the original absolute
            // URL — `unprovided_consumes`/`ambiguous_consumes` feed the near-miss family, whose
            // segment logic has never seen (and must not see) a scheme-carrying key. Provenance
            // lands in `raw`: the original absolute spelling, unless `raw` is already set (a
            // late-resolved consume keeps its richer const-expr provenance — the earlier stage
            // wins, same contract as late cross-file resolution filling `key` in).
            let mut rekeyed_consume: Option<IoConsume> = None;
            let key = match classify_consume_join(c.key.as_deref(), &opts.internal_hosts) {
                ConsumeJoin::Unresolved => {
                    unresolved_consumes.push(TaggedConsume {
                        source: source.clone(),
                        consume: c.clone(),
                    });
                    continue;
                }
                ConsumeJoin::External => {
                    // A host-carrying key is third-party egress — never cross-tree joined, never
                    // `unprovidedConsumes`. Unreachable after a re-key, so `c` is the whole record.
                    external_consumes.push(TaggedConsume {
                        source: source.clone(),
                        consume: c.clone(),
                    });
                    continue;
                }
                ConsumeJoin::Joinable { key, rekeyed_host } => {
                    if let Some(host) = rekeyed_host {
                        if let Some(entry) = host_rekey_counts.iter_mut().find(|(h, _)| *h == host)
                        {
                            entry.1 += 1;
                        }
                        let mut cc = c.clone();
                        cc.raw = cc.raw.take().or_else(|| cc.key.clone());
                        cc.key = Some(key.to_string());
                        rekeyed_consume = Some(cc);
                    }
                    key
                }
            };
            let key: &str = &key;
            // Machine-pinned bucket invariant (class sweep 2026-07-14): everything past the
            // external gate reports under a scheme-free key, and the bucket CLONE must agree with
            // the join key — the near-miss family consumes these buckets and must never see a
            // `://` key. Guards any future transform that leaks a pre-rekey record.
            let bucket_consume = || {
                let out = rekeyed_consume.clone().unwrap_or_else(|| c.clone());
                debug_assert!(
                    out.key.as_deref().is_none_or(|k| !k.contains("://")),
                    "bucket invariant violated: a scheme-carrying consume key reached a join \
                     bucket — a transform leaked a pre-rekey record (key {:?})",
                    out.key
                );
                out
            };
            let k = id_key(&c.kind, key);
            let Some(providers) = providers_by_key.get(&k) else {
                // Route-identity gate (module doc). A key whose every path segment is `{}` names no
                // route, so its MISS proves nothing about the contract space — only that the extractor
                // lost the target. Calling that `unprovidedConsumes` INVENTS a missing internal route;
                // `unresolvedConsumes` states the truth (blind here) and is what
                // `cross-layer/unresolved-consume-ratio` counts, so the added silence is disclosed
                // rather than hidden. A HIT above is untouched: joining a declared catch-all provide is
                // a join, not a guess.
                let bucket = if key_carries_route_identity(key) {
                    &mut unprovided_consumes
                } else {
                    &mut unresolved_consumes
                };
                bucket.push(TaggedConsume {
                    source: source.clone(),
                    consume: bucket_consume(),
                });
                continue;
            };
            if ambiguous_keys.contains(&k) {
                ambiguously_consumed_keys.insert(k.clone());
                let mut candidates = providers.clone();
                candidates.sort_by(|a, b| {
                    a.source
                        .cmp(&b.source)
                        .then(a.provide.file.cmp(&b.provide.file))
                        .then(a.provide.line.cmp(&b.provide.line))
                });
                ambiguous_consumes.push(AmbiguousConsume {
                    source: source.clone(),
                    consume: bucket_consume(),
                    candidates,
                });
                continue;
            }
            consumed_keys.insert(k.clone());
            let low_confidence_reason = opts
                .low_confidence_key_patterns
                .iter()
                .find(|(re, _)| re.is_match(key))
                .map(|(_, reason)| reason.clone());
            for p in providers {
                edges.push(CrossLayerEdge {
                    kind: c.kind.clone(),
                    key: key.to_string(),
                    from: EdgeFrom {
                        source: source.clone(),
                        file: c.file.clone(),
                        line: c.line,
                    },
                    to: EdgeTo {
                        source: p.source.clone(),
                        file: p.provide.file.clone(),
                        line: p.provide.line,
                        symbol: p.provide.symbol.clone(),
                    },
                    cross_source: *source != p.source,
                    low_confidence_reason: low_confidence_reason.clone(),
                });
            }
        }
    }

    // A provide that was referenced ambiguously (it IS a candidate some consume saw, just not
    // unambiguously linkable) is not dead — but a multi-tree-provided key NOBODY consumes is exactly as
    // dead as a single-tree one, so the exclusion is keyed on `ambiguously_consumed_keys` (keys that
    // actually produced an `ambiguous_consumes` entry), never on provider-set shape alone — see
    // `CrossLayerResult::unconsumed_provides`'s doc.
    let mut unconsumed_provides = Vec::new();
    for (k, providers) in providers_by_key {
        if !consumed_keys.contains(&k) && !ambiguously_consumed_keys.contains(&k) {
            unconsumed_provides.extend(providers);
        }
    }
    // `providers_by_key` is a BTreeMap, but its key is `id_key(kind, key)` — NOT the comparator below —
    // so this sort still has work to do: the serialized `unconsumedProvides` order must be stable
    // run-to-run (deterministic-output contract; every other bucket is already ordered).
    unconsumed_provides.sort_by(|a, b| {
        a.provide
            .key
            .cmp(&b.provide.key)
            .then(a.source.cmp(&b.source))
            .then(a.provide.file.cmp(&b.provide.file))
            .then(a.provide.line.cmp(&b.provide.line))
    });

    edges.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then(a.from.file.cmp(&b.from.file))
            .then(a.from.line.cmp(&b.from.line))
    });
    ambiguous_consumes.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.consume.file.cmp(&b.consume.file))
            .then(a.consume.line.cmp(&b.consume.line))
    });
    external_consumes.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.consume.file.cmp(&b.consume.file))
            .then(a.consume.line.cmp(&b.consume.line))
    });

    CrossLayerResult {
        edges,
        unconsumed_provides,
        unprovided_consumes,
        unresolved_consumes,
        external_consumes,
        ambiguous_consumes,
        host_rekey_counts,
    }
}

fn id_key(kind: &str, key: &str) -> String {
    format!("{kind} {key}")
}

#[cfg(test)]
mod tests;
