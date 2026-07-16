//! The cross-layer linker: an exact `(kind, key)` join of trees' IO with the ambiguity /
//! external-egress / low-confidence gates documented in the [`crate::io`] module doc.

use std::collections::HashMap;

use super::facts::{
    AmbiguousConsume, CrossLayerEdge, CrossLayerResult, EdgeFrom, EdgeTo, IoConsume, SourceIo,
    TaggedConsume, TaggedProvide,
};
use super::key::http_consume_interface_key;

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

/// Exact join of trees' IO on (kind, key), with the ambiguity/external/low-confidence gates documented in
/// this module's doc. Pure function (given `opts`).
pub fn link_cross_layer_io(trees: &[SourceIo], opts: &LinkOptions) -> CrossLayerResult {
    // Index providers by (kind, key). Multiple providers for one key is legal (e.g. two services expose one topic).
    let mut providers_by_key: HashMap<String, Vec<TaggedProvide>> = HashMap::new();
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
            let Some(key) = &c.key else {
                unresolved_consumes.push(TaggedConsume {
                    source: source.clone(),
                    consume: c.clone(),
                });
                continue;
            };
            // Deployment-topology host re-key — MUST run BEFORE the `"://"` external-egress gate right
            // below: an absolute-URL consume whose authority matches a declared internal host is a
            // same-deployment call that merely happens to spell its own gateway host out loud, not
            // third-party egress. A miss (no host declared, or none matches) falls through to the
            // ordinary external gate byte-for-byte — this never changes behavior for a tree that declares
            // no hosts.
            let rekeyed;
            // On a re-key, downstream buckets must carry the JOIN key, not the original absolute
            // URL — `unprovided_consumes`/`ambiguous_consumes` feed the near-miss family, whose
            // segment logic has never seen (and must not see) a scheme-carrying key. Provenance
            // lands in `raw`: the original absolute spelling, unless `raw` is already set (a
            // late-resolved consume keeps its richer const-expr provenance — the earlier stage
            // wins, same contract as late cross-file resolution filling `key` in).
            let mut rekeyed_consume: Option<IoConsume> = None;
            let key: &String = match rekey_if_internal_host(key, &opts.internal_hosts) {
                Some((new_key, host)) => {
                    if let Some(entry) = host_rekey_counts.iter_mut().find(|(h, _)| *h == host) {
                        entry.1 += 1;
                    }
                    let mut cc = c.clone();
                    cc.raw = cc.raw.take().or_else(|| cc.key.clone());
                    cc.key = Some(new_key.clone());
                    rekeyed_consume = Some(cc);
                    rekeyed = new_key;
                    &rekeyed
                }
                None => key,
            };
            if key.contains("://") {
                // A host-carrying key is third-party egress — never cross-tree joined, never
                // `unprovidedConsumes`.
                external_consumes.push(TaggedConsume {
                    source: source.clone(),
                    consume: c.clone(),
                });
                continue;
            }
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
                unprovided_consumes.push(TaggedConsume {
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
                    key: key.clone(),
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
    // `providers_by_key` is a HashMap — sort so the serialized `unconsumedProvides` order is stable
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

/// Attempts to re-key an absolute-URL consume key (`"METHOD http(s)://authority/rest..."`) against
/// `internal_hosts` — see [`LinkOptions::internal_hosts`]'s doc for the exact matching rule. Returns
/// `Some((rekeyed_key, matched_host))` on a hit (`matched_host` is the literal entry from
/// `internal_hosts` that matched, for [`CrossLayerResult::host_rekey_counts`] bookkeeping); `None` when
/// the key isn't a `"METHOD scheme://..."` shape, the scheme isn't `http`/`https`, or the authority
/// matches no declared host — the caller falls through to the ordinary external-egress gate untouched.
fn rekey_if_internal_host(key: &str, internal_hosts: &[String]) -> Option<(String, String)> {
    let (method, rest) = key.split_once(' ')?;
    let scheme_end = rest.find("://")?;
    let scheme = &rest[..scheme_end];
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None; // ws/wss (and anything else) stay external in v1
    }
    let after_scheme = &rest[scheme_end + 3..];
    let (authority, path) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => (after_scheme, "/"),
    };
    let authority_host = authority.split(':').next().unwrap_or(authority);
    for declared in internal_hosts {
        let matched = match declared.split_once(':') {
            // Declared host carries an explicit port — the consume must match host:port exactly.
            Some((decl_host, decl_port)) => match authority.split_once(':') {
                Some((host, port)) => host.eq_ignore_ascii_case(decl_host) && port == decl_port,
                None => false,
            },
            // Declared host carries no port — the consume side's port (if any) is ignored.
            None => authority_host.eq_ignore_ascii_case(declared),
        };
        if matched {
            return Some((http_consume_interface_key(method, path), declared.clone()));
        }
    }
    None
}

fn id_key(kind: &str, key: &str) -> String {
    format!("{kind} {key}")
}

#[cfg(test)]
mod tests;
