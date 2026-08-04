//! Per-site lookup maps derived once from `source_ios` and handed to the rules that join against them.
//! Split out of `mod.rs` to keep that file under the line-count ratchet; the derivations are mechanical
//! and share one anchor key, so they belong together.
//!
//! Every map here is keyed `(source, file, line)` — the same triple `CrossLayerEdge`'s `from`/`to` anchors
//! carry, so a rule holding an edge joins straight in with no second key vocabulary to keep in step.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::{ConsumeBodyShape, ProvideBodyShape, SourceIo};

/// The site-keyed join inputs `cross-layer/body-field-drift`,
/// `cross-layer/retrying-write-no-idempotency` and `cross-layer/sensitive-response-field` need.
#[derive(Default)]
pub(super) struct JoinMaps {
    /// FE-witnessed request-body shapes, first occurrence wins per site.
    pub consume_bodies: BTreeMap<(String, String, u32), ConsumeBodyShape>,
    /// BE handler DTO shapes, first occurrence wins per site.
    pub provide_bodies: BTreeMap<(String, String, u32), ProvideBodyShape>,
    /// Retry-configured consume sites (a write-only tag upstream).
    pub retry_sites: BTreeSet<zzop_rules_cross_layer::RetrySite>,
    /// Every `http` provide carrying a resolved declared-response shape (`response-shape-v1`) — a
    /// Vec, not a site-keyed map, because one line can register several routes (array-path
    /// decorators) and a `(source, file, line)` key would collapse them (see
    /// `ResponseProvideSite`'s own doc). Input order preserved; the rule sorts its own output.
    pub response_sites: Vec<zzop_rules_cross_layer::ResponseProvideSite>,
}

/// Walks every source's `http` io once, filling all four channels. Only `Some`-shape entries are kept —
/// absence means "no shape was witnessed", never "an empty one", so a defaulted entry would invent
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
            if let Some(response) = &p.response {
                maps.response_sites
                    .push(zzop_rules_cross_layer::ResponseProvideSite {
                        source: s.source.clone(),
                        key: p.key.clone(),
                        file: p.file.clone(),
                        line: p.line,
                        response: response.clone(),
                    });
            }
        }
    }
    maps
}

/// The three shape/edge-keyed rule gates over [`JoinMaps`] — body drift, sensitive response field,
/// retrying write — pushed in this fixed order into `sources` (order matters only for determinism of
/// the pre-merge vector; `merge_findings` re-sorts). Lives here rather than in `mod.rs` so the maps
/// and every rule that reads them stay in one file (and `mod.rs` stays under the line-count ratchet).
pub(super) fn push_shape_rule_findings(
    sources: &mut Vec<Vec<zzop_core::Finding>>,
    gate: &zzop_core::RuleConfig,
    edges: &[zzop_core::CrossLayerEdge],
    maps: &JoinMaps,
    sensitive_vocab: zzop_rules_cross_layer::SensitiveResponseVocab<'_>,
    attribute_stores: &BTreeMap<String, &zzop_core::AttributeStore>,
) {
    if zzop_core::is_enabled(gate, "cross-layer/body-field-drift") {
        sources.push(zzop_rules_cross_layer::body_field_drift_findings(
            edges,
            &maps.consume_bodies,
            &maps.provide_bodies,
        ));
    }
    if zzop_core::is_enabled(gate, "cross-layer/sensitive-response-field") {
        sources.push(zzop_rules_cross_layer::sensitive_response_field_findings(
            &maps.response_sites,
            edges,
            sensitive_vocab,
        ));
    }
    if zzop_core::is_enabled(gate, "cross-layer/retrying-write-no-idempotency") {
        sources.push(
            zzop_rules_cross_layer::retrying_write_no_idempotency_findings(
                edges,
                &maps.retry_sites,
                attribute_stores,
            ),
        );
    }
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
