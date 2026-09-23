//! `cross-layer/duplicate-route` (warning) — the same `http` `(method, path)` key PROVIDED by 2+ DISTINCT
//! sources. This is the multi-tree-provider condition `zzop_core::io::link_cross_layer_io` already computes
//! internally (`ambiguous_keys`, io.rs), read back off its two observable traces: an unconsumed multi-source
//! key lands in `unconsumed_provides` (grouped by key), and one some consume references lands in
//! `ambiguous_consumes` with its full candidate list. A key provided by 2+ distinct sources can never
//! produce a `CrossLayerEdge` — the linker routes it to `ambiguous_consumes` before edge emission — so those
//! two buckets together cover every multi-source-provided http key, and `edges` is never a source here.
//!
//! Distinct from the existing single-tree `zzop_rules_http::duplicate_route` rule (id `"duplicate-route"`): that one
//! flags 2+ registrations of a route WITHIN one tree; this one only fires when the duplicates span 2+
//! DIFFERENT trees. Different id, so both can be registered/disabled independently.
//!
//! **ONE FINDING PER PROVIDING SOURCE, each anchored in that source's own tree** (2026-07-29), following
//! `shared_db_table`'s precedent — read its module doc for the full argument. In short: `exclude` applies
//! to the ANCHOR, so a single representative made WHICH tree could silence an N-tree fact an accident of
//! sorting, and that tree excluding its own paths deleted the half about the other trees. Every copy still
//! lists the full source set and every site, so nothing is lost.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped, same policy as
//! `mutating-route-no-auth`. A dead multi-tree route also yields per-provider
//! `cross-layer/unconsumed-endpoint` info findings — the overlap is intentional (different questions: "who
//! serves this?" vs "who calls this?").

use std::collections::BTreeSet;

use super::landing::{CALLER_BREAKAGE_LANDING, HANDLER_SUBSTITUTION_LANDING};
use zzop_core::io::CrossLayerResult;
use zzop_core::{disable_hint, Finding, Severity};

pub fn cross_layer_duplicate_route_findings(cross_layer: &CrossLayerResult) -> Vec<Finding> {
    let mut by_key: std::collections::BTreeMap<String, Vec<(String, String, u32)>> =
        std::collections::BTreeMap::new();

    for p in cross_layer
        .unconsumed_provides
        .iter()
        // A verb-unknown route (`UNKNOWN_VERB` sentinel `"? <path>"`) is never a "duplicate route"
        // candidate — its method is unknown, so comparing sentinel keys across trees is meaningless, and
        // its `"? <path>"` key must never surface in a finding message. It is disclosed via
        // `cross-layer/unknown-verb-route` instead.
        .filter(|p| {
            p.provide.kind == "http"
                && zzop_core::unknown_verb_route_path(&p.provide.key).is_none()
                && !zzop_core::is_test_file(&p.provide.file)
        })
    {
        by_key.entry(p.provide.key.clone()).or_default().push((
            p.source.clone(),
            p.provide.file.clone(),
            p.provide.line,
        ));
    }
    for a in cross_layer
        .ambiguous_consumes
        .iter()
        .filter(|a| a.consume.kind == "http")
    {
        for cand in &a.candidates {
            if zzop_core::is_test_file(&cand.provide.file) {
                continue;
            }
            by_key.entry(cand.provide.key.clone()).or_default().push((
                cand.source.clone(),
                cand.provide.file.clone(),
                cand.provide.line,
            ));
        }
    }

    let mut out = Vec::new();
    for (key, mut sites) in by_key {
        sites.sort();
        sites.dedup();
        let distinct_sources: BTreeSet<&str> = sites.iter().map(|(s, _, _)| s.as_str()).collect();
        if distinct_sources.len() < 2 {
            continue;
        }
        let sites_desc: Vec<String> = sites
            .iter()
            .map(|(s, f, l)| format!("{s}:{f}:{l}"))
            .collect();
        let sources_list: Vec<&str> = distinct_sources.iter().copied().collect();
        // ONE COPY PER PROVIDING SOURCE, each anchored in its own tree — see the module doc. `sites` is
        // sorted, so each source's first match is deterministic.
        for source in &distinct_sources {
            let Some((_, file, line)) = sites.iter().find(|(s, _, _)| s == source) else {
                continue;
            };
            let others: Vec<&str> = sources_list
                .iter()
                .copied()
                .filter(|s| s != source)
                .collect();
            let message = format!(
                "route `{key}` is provided by this source (`{source}`, first at {file}:{line}) and by {} \
                 other analyzed source(s) ({}). A caller cannot deterministically tell which source's \
                 handler serves a request for this route; if these sources are ever deployed behind the \
                 same host/gateway, whichever one wins is a deploy-order accident, not a design decision. \
                 If they never share a host, this finding does not bite — confirm that before you edit \
                 anything. {CALLER_BREAKAGE_LANDING} IF YOU KEEP BOTH HANDLERS: namespace the routes \
                 apart (a path prefix, a different host). {HANDLER_SUBSTITUTION_LANDING} IF YOU MERGE \
                 THEM INSTEAD: keep the address and make one implementation serve both sets of callers. \
                 All sites: {}. Each providing source gets its own copy of this finding, anchored in its own \
                 tree, so excluding one source's paths never silences the others. {} if these are \
                 intentionally separate services on different hosts that happen to share a route shape.",
                others.len(),
                others.join(", "),
                sites_desc.join(", "),
                disable_hint("cross-layer/duplicate-route"),
            );
            out.push(Finding {
                rule_id: "cross-layer/duplicate-route".to_string(),
                severity: Severity::Warning,
                file: file.clone(),
                line: *line,
                message,
                // Every OTHER site this copy prints in `sites`/`sites_desc`. Deduped and sorted so the
                // field is deterministic when two sources collide in one file.
                evidence_paths: sites
                    .iter()
                    .map(|(_, f, _)| f)
                    .filter(|f| *f != file)
                    .cloned()
                    .collect::<BTreeSet<String>>()
                    .into_iter()
                    .collect(),
                data: Some(serde_json::json!({
                    "key": key,
                    "provideSource": source,
                    "sources": sources_list,
                    "sites": sites_desc,
                })),
            });
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
