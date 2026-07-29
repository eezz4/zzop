//! `cross-layer/db-table-name-in-multiple-sources` (warning) — the same `db-table` key CONSUMED by 2+ distinct sources.
//! Consumes are the signal; provides don't matter here (unlike `duplicate_route`, which is about who
//! PROVIDES a key) — a table is "shared" when multiple sources read/write it, regardless of which source
//! declares its schema. Signal is pulled from three places — `edges` (kind `db-table`, consumer side),
//! `ambiguous_consumes`, and `unprovided_consumes` — since all three can carry a `db-table` consume.
//!
//! **ONE FINDING PER PARTICIPATING SOURCE, each anchored in that source's own tree** (2026-07-29). Until
//! then this emitted a single finding anchored at the alphabetically-first source's first site, which made
//! WHICH tree could silence it an accident of sorting: that tree excluding its own paths deleted the whole
//! finding, including the half that was about the OTHER trees — and the point of this rule is the other
//! trees. Anchoring is not a presentation detail here, because `exclude` is applied to the anchor.
//!
//! This is not the per-call wall `all_consumes_unjoined` folds. The unit of the fact IS the (table,
//! source) pair — each participating tree genuinely has something to answer for — and any single reader
//! sees exactly one copy, in their own files. Every copy still lists the full source set, so nothing is
//! lost relative to the single-finding form.
//!
//! Three sibling rules still anchor an N-source fact at `sites[0]` the old way (`duplicate_route`,
//! `external_duplicated_integration`, `external_host_fanout`). That is the same shape, was measured while
//! fixing this one, and is queued rather than fixed here — widening the change silently would move three
//! more rules' output in a commit whose subject is this one.
//!
//! Two sources merely consuming the same table-key string is only evidence of a naming coincidence, not
//! proof of a shared physical database (an unrelated repo providing its own same-named table lands in
//! `ambiguous_consumes` instead, via join integrity) — the finding message says so explicitly, which is
//! why the id names the table NAME rather than a shared table. Renamed from
//! `cross-layer/shared-db-table`, whose message already denied it ("not that they physically share one
//! database"); the old id is recorded in `VERSIONING.md`.

use std::collections::BTreeSet;

use zzop_core::io::CrossLayerResult;
use zzop_core::{disable_hint, Finding, Severity};

pub fn shared_db_table_findings(cross_layer: &CrossLayerResult) -> Vec<Finding> {
    let mut by_key: std::collections::BTreeMap<String, Vec<(String, String, u32)>> =
        std::collections::BTreeMap::new();

    for e in cross_layer.edges.iter().filter(|e| e.kind == "db-table") {
        by_key.entry(e.key.clone()).or_default().push((
            e.from.source.clone(),
            e.from.file.clone(),
            e.from.line,
        ));
    }
    for a in cross_layer
        .ambiguous_consumes
        .iter()
        .filter(|a| a.consume.kind == "db-table")
    {
        if let Some(key) = &a.consume.key {
            by_key.entry(key.clone()).or_default().push((
                a.source.clone(),
                a.consume.file.clone(),
                a.consume.line,
            ));
        }
    }
    for d in cross_layer
        .unprovided_consumes
        .iter()
        .filter(|d| d.consume.kind == "db-table")
    {
        if let Some(key) = &d.consume.key {
            by_key.entry(key.clone()).or_default().push((
                d.source.clone(),
                d.consume.file.clone(),
                d.consume.line,
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
        let sources_list: Vec<&str> = distinct_sources.iter().copied().collect();
        for source in &distinct_sources {
            // This source's OWN first site — `sites` is sorted, so the first match is deterministic.
            let Some((_, file, line)) = sites.iter().find(|(s, _, _)| s == source) else {
                continue;
            };
            let others: Vec<&str> = sources_list
                .iter()
                .copied()
                .filter(|s| s != source)
                .collect();
            let message = format!(
                "db table `{key}` is consumed by this source (`{source}`, first at {file}:{line}) and by \
                 {} other analyzed source(s) ({}). This only shows the same table identifier is referenced \
                 from multiple analyzed sources, not that they physically share one database: unrelated \
                 repos with independent databases can coincidentally name a table the same. Verify these \
                 sources actually share one database before treating this as real coupling. Each \
                 participating source gets its own copy of this finding, anchored in its own tree, so \
                 excluding one source's paths never silences the others. {} if table-name collisions \
                 across independent databases are expected in your stack.",
                others.len(),
                others.join(", "),
                disable_hint("cross-layer/db-table-name-in-multiple-sources"),
            );
            out.push(Finding {
                rule_id: "cross-layer/db-table-name-in-multiple-sources".to_string(),
                severity: Severity::Warning,
                file: file.clone(),
                line: *line,
                message,
                // The sibling consume sites this copy's message counts. Deduped and sorted.
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
                    "consumeSource": source,
                    "sources": sources_list,
                })),
            });
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
