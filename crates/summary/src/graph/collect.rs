//! Bucket collection: the `analyzeTrees` `crossLayer` block -> the parent module's node/edge model,
//! plus the per-bucket census that makes `--scope`/`--top` disclosable. Split out of `mod.rs` for the
//! 300-line source cap; the two halves stay one module in every other sense (this file reads the
//! parent's private model directly, which is exactly the coupling a projection and its data shape
//! should have).
//!
//! ## Rows vs. relations — why `--top` counts what is DRAWN
//! A join bucket is a list of CALL SITES: 60 `edges` rows over the OSS corpus's express/axios pair
//! collapse into 4 distinct `(source, key)` relations, because a node here aggregates sites (see the
//! parent module doc). Capping raw rows would therefore cap something the viewer cannot see — on that
//! measured pair, `--top 5` over rows drew exactly ONE arrow, so the disclosure said "5 shown" about a
//! picture with one relation in it. So the dedup happens FIRST and the cap applies to distinct
//! relations; the census keeps the raw row count beside it (`… (60 sites)`) so the aggregation is
//! visible rather than merely true.

use std::collections::BTreeSet;

use super::model::{BucketCount, Graph, GraphEdge, CONSUME, PROVIDE};

/// Does a row survive `--scope`? The rule, stated once and documented on the CLI: a row is kept when
/// ANY of its identity strings — the source id or a site's file path — starts with the prefix. One
/// rule for both spellings, because a `sourceId` in this repo is very often a path (`./api`) and a
/// two-mode filter that silently picks one would be the ambiguity a scope filter exists to remove.
pub(super) fn in_scope(scope: Option<&str>, fields: &[Option<&str>]) -> bool {
    match scope {
        None => true,
        Some(prefix) => fields.iter().flatten().any(|s| s.starts_with(prefix)),
    }
}

/// Accumulates one bucket's distinct relations: `total` counts every renderable row's relation
/// (ignoring `--scope`), `kept` holds the in-scope ones in first-seen order. Two `BTreeSet`s rather
/// than one, because the two counts answer different questions — "how much is there" and "how much
/// survived the filter" — and the difference between them is what the scope disclosure reports.
#[derive(Default)]
struct Distinct<T> {
    total: BTreeSet<String>,
    seen_in_scope: BTreeSet<String>,
    kept: Vec<T>,
    unlabelable: usize,
}

impl<T> Distinct<T> {
    /// Records one renderable row under its relation `id`; `in_scope` decides whether it can also be
    /// drawn. Returns nothing — a duplicate is silently folded into the relation it repeats, which is
    /// the aggregation the census discloses.
    fn push(&mut self, id: String, in_scope: bool, row: T) {
        self.total.insert(id.clone());
        if in_scope && self.seen_in_scope.insert(id) {
            self.kept.push(row);
        }
    }

    fn count(self, bucket: &'static str, rows: usize, top: usize) -> (Vec<T>, BucketCount) {
        let count = BucketCount {
            bucket,
            rows,
            total: self.total.len(),
            in_scope: self.kept.len(),
            shown: self.kept.len().min(top),
            unlabelable: self.unlabelable,
        };
        (self.kept, count)
    }
}

/// `crossLayer.edges` -> one arrow per distinct (consumer node, provider node). A
/// `lowConfidenceReason` edge is drawn DOTTED and labelled with the reason: the engine emits it as a
/// real edge, but the key shape is generic enough that the match is a weaker claim, and a picture that
/// draws it identically to a distinctive match over-claims.
pub(super) fn collect_edges(
    g: &mut Graph,
    edges: &serde_json::Value,
    scope: Option<&str>,
    top: usize,
) -> BucketCount {
    let items = edges.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut d = Distinct::default();
    for e in items {
        let (from, to) = (&e["from"], &e["to"]);
        let (Some(kind), Some(key), Some(fs), Some(ts)) = (
            e["kind"].as_str(),
            e["key"].as_str(),
            from["source"].as_str(),
            to["source"].as_str(),
        ) else {
            d.unlabelable += 1;
            continue;
        };
        let low = e["lowConfidenceReason"].as_str();
        // The relation's identity is exactly what gets drawn — the two endpoint nodes plus the arrow's
        // own claim strength. Two call sites that draw the same arrow are one relation.
        let id = format!(
            "{fs}\u{1}{ts}\u{1}{kind}\u{1}{key}\u{1}{}",
            low.unwrap_or("")
        );
        let ok = in_scope(
            scope,
            &[
                Some(fs),
                Some(ts),
                from["file"].as_str(),
                to["file"].as_str(),
            ],
        );
        d.push(id, ok, (kind, key, fs, ts, low));
    }
    let (kept, count) = d.count("edges", items.len(), top);
    for (kind, key, fs, ts, low) in kept.iter().take(top) {
        g.node(fs, CONSUME, kind, key, "linked");
        g.node(ts, PROVIDE, kind, key, "linked");
        g.edges.insert(GraphEdge {
            from: (fs.to_string(), CONSUME, kind.to_string(), key.to_string()),
            to: (ts.to_string(), PROVIDE, kind.to_string(), key.to_string()),
            dotted: low.is_some(),
            label: low.map(|r| format!("low confidence: {r}")),
        });
    }
    count
}

/// Which side a bucket's node sits on, and the role word its label carries — TOTAL over
/// `crate::output::KEY_BUCKETS` and explicit for every member, with `None` for anything else.
///
/// It used to be a `match` ending in `_ => (CONSUME, "ambiguous")`, and that wildcard is what made the
/// graph's own bucket-coverage test blind: a SIXTH bucket added to `KEY_BUCKETS` was silently absorbed
/// into the role the test already expected to see, so the picture claimed to be complete while drawing
/// the new bucket's rows under someone else's word. `None` instead of a fallback role is the whole
/// point — a bucket with no declared role draws nothing and is DISCLOSED as an unlabelable remainder
/// (see `collect_bucket`), and `tests::every_join_bucket_has_an_explicit_graph_role` turns it red.
///
/// A compile-time-exhaustive version (an enum instead of the wire strings) was considered and rejected:
/// these names are the engine's JSON keys, shared verbatim with `bucket_keys`/`manifest`, so an enum
/// would add a conversion at three call sites to buy what the two derived pins below already give.
pub(super) fn bucket_role(bucket: &str) -> Option<(&'static str, &'static str)> {
    match bucket {
        "unconsumedProvides" => Some((PROVIDE, "unconsumed")),
        "unprovidedConsumes" => Some((CONSUME, "unprovided")),
        "unresolvedConsumes" => Some((CONSUME, "unresolved")),
        "externalConsumes" => Some((CONSUME, "external")),
        "ambiguousConsumes" => Some((CONSUME, "ambiguous")),
        _ => None,
    }
}

/// One of the five non-edge buckets -> nodes (plus, for `ambiguousConsumes`, one dotted arrow per
/// candidate provider, because "which of these three trees serves it" is the entire content of that
/// bucket and a bare node would drop it).
pub(super) fn collect_bucket(
    g: &mut Graph,
    bucket: &'static str,
    value: &serde_json::Value,
    scope: Option<&str>,
    top: usize,
) -> BucketCount {
    let items = value.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut d: Distinct<(&str, &str, &str, &serde_json::Value)> = Distinct::default();
    // No declared role -> nothing is drawn, and every row is disclosed as an unlabelable remainder
    // rather than borrowing another bucket's word. `bucket_role`'s doc says why this is not a fallback.
    let Some((side, role)) = bucket_role(bucket) else {
        d.unlabelable += items.len();
        return d.count(bucket, items.len(), top).1;
    };
    for item in items {
        // Same key fallback `bucket_keys`/`manifest` use: an unresolved consume has no key, so its
        // `raw` source text IS its label. An item with neither is counted and dropped, never guessed.
        let label = item["key"]
            .as_str()
            .or_else(|| item["raw"].as_str())
            .filter(|s| !s.is_empty());
        let (Some(label), Some(source), Some(kind)) =
            (label, item["source"].as_str(), item["kind"].as_str())
        else {
            d.unlabelable += 1;
            continue;
        };
        let ok = in_scope(scope, &[Some(source), item["file"].as_str()]);
        d.push(
            format!("{source}\u{1}{kind}\u{1}{label}"),
            ok,
            (source, kind, label, &item["candidates"]),
        );
    }
    let (kept, count) = d.count(bucket, items.len(), top);
    for (source, kind, label, candidates) in kept.iter().take(top) {
        g.node(source, side, kind, label, role);
        for c in candidates.as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let Some(cs) = c["source"].as_str() else {
                continue;
            };
            g.node(cs, PROVIDE, kind, label, "candidate");
            g.edges.insert(GraphEdge {
                from: (
                    source.to_string(),
                    side,
                    kind.to_string(),
                    label.to_string(),
                ),
                to: (cs.to_string(), PROVIDE, kind.to_string(), label.to_string()),
                dotted: true,
                label: Some("ambiguous".to_string()),
            });
        }
    }
    count
}
