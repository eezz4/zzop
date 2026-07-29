//! Per-site lookup maps derived once from `source_ios` and handed to the rules that join against them.
//! Split out of `mod.rs` to keep that file under the line-count ratchet; the derivations are mechanical
//! and share one anchor key, so they belong together.
//!
//! Every map here is keyed `(source, file, line)` — the same triple `CrossLayerEdge`'s `from`/`to` anchors
//! carry, so a rule holding an edge joins straight in with no second key vocabulary to keep in step.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::{ConsumeBodyShape, ProvideBodyShape, SourceIo};

/// The site-keyed join inputs `cross-layer/body-field-drift` and
/// `cross-layer/retrying-write-no-idempotency` need.
#[derive(Default)]
pub(super) struct JoinMaps {
    /// FE-witnessed request-body shapes, first occurrence wins per site.
    pub consume_bodies: BTreeMap<(String, String, u32), ConsumeBodyShape>,
    /// BE handler DTO shapes, first occurrence wins per site.
    pub provide_bodies: BTreeMap<(String, String, u32), ProvideBodyShape>,
    /// Retry-configured consume sites (a write-only tag upstream).
    pub retry_sites: BTreeSet<zzop_rules_cross_layer::RetrySite>,
}

/// Walks every source's `http` io once, filling all three maps. Only `Some`-body entries are kept —
/// absence means "no body shape was witnessed", never "an empty body", so a defaulted entry would invent
/// a fact the extractor declined to state.
pub(super) fn build(source_ios: &[SourceIo]) -> JoinMaps {
    let mut maps = JoinMaps::default();
    for s in source_ios {
        for c in s.io.consumes.iter().filter(|c| c.kind == "http") {
            if let Some(body) = &c.body {
                maps.consume_bodies
                    .entry((s.source.clone(), c.file.clone(), c.line))
                    .or_insert_with(|| body.clone());
            }
            if c.retry_configured == Some(true) {
                maps.retry_sites
                    .insert((s.source.clone(), c.file.clone(), c.line));
            }
        }
        for p in s.io.provides.iter().filter(|p| p.kind == "http") {
            if let Some(body) = &p.body {
                maps.provide_bodies
                    .entry((s.source.clone(), p.file.clone(), p.line))
                    .or_insert_with(|| body.clone());
            }
        }
    }
    maps
}

/// Per-source `http` consume totals — the denominator the blindness-ratio rules divide by. Sources with
/// ZERO http consumes are omitted rather than recorded as 0: a ratio over an empty population is not a
/// small number, it is undefined, and every consumer of this list treats absence as "not eligible".
pub(super) fn http_consume_totals(source_ios: &[SourceIo]) -> Vec<(String, usize)> {
    source_ios
        .iter()
        .filter_map(|s| {
            let n = s.io.consumes.iter().filter(|c| c.kind == "http").count();
            (n > 0).then(|| (s.source.clone(), n))
        })
        .collect()
}
