//! `query`'s matching/collection helpers: per-bucket scanning, the related-findings sweep sources,
//! not-found suggestions, and the single-tree guided error. Pure functions over `serde_json::Value`
//! — the parent module (`query.rs`) owns the wire contract (verdict vocabulary, caps, assembly).

use serde_json::Value;

use super::{BUCKETS, QUERY_MATCH_LIMIT, QUERY_SUGGESTIONS_LIMIT};

/// The queryable identity of one bucket item: `key` for every resolved shape (edges and all keyed
/// buckets), falling back to `raw` for an unresolved consume (`key: null`). `None` = unmatched by
/// construction (an unresolved consume that recorded no raw expression has nothing to match on).
pub(super) fn item_key(item: &Value) -> Option<&str> {
    match item.get("key").and_then(Value::as_str) {
        Some(k) => Some(k),
        None => item.get("raw").and_then(Value::as_str),
    }
}

pub(super) fn matches_pattern(candidate: &str, needle_lower: &str) -> bool {
    candidate.to_lowercase().contains(needle_lower)
}

/// Last non-empty `/`-separated segment, trimmed and lowercased — `"GET /api/users"` -> `"users"`,
/// a segmentless key (`"DATABASE_URL"`) is its own last segment.
fn last_segment_lower(s: &str) -> String {
    s.rsplit('/')
        .map(str::trim)
        .find(|seg| !seg.is_empty())
        .unwrap_or("")
        .to_lowercase()
}

/// The single-tree guided error: match the pattern over the raw `ir.io` channel the single output
/// actually exposes, report those counts, and point at a trees analysis for a join verdict.
pub(super) fn single_tree_err(analysis: &Value, pattern: &str) -> String {
    let needle = pattern.to_lowercase();
    let count = |list: &str| {
        analysis["ir"]["io"][list]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|i| item_key(i).is_some_and(|k| matches_pattern(k, &needle)))
                    .count()
            })
            .unwrap_or(0)
    };
    format!(
        "zzop-facade: queryIo() got a single-tree analyze() output — it carries raw io facts \
         (ir.io: {} provides / {} consumes match {pattern:?}) but no cross-layer join, and every \
         verdict (linked/provided-only/consumed-unprovided/...) is a join fact. Run an \
         analyzeTrees() analysis instead (a single tree still gets the join, intra-tree edges \
         included) and query that output.",
        count("provides"),
        count("consumes"),
    )
}

/// One bucket's scan result: full match count, capped original objects, and the distinct matched
/// keys (lowercased, for the related-findings scan).
pub(super) struct BucketMatches {
    pub(super) count: usize,
    pub(super) shown: Vec<Value>,
    pub(super) matched_keys_lower: Vec<String>,
}

pub(super) fn scan_bucket(items: Option<&Vec<Value>>, needle_lower: &str) -> BucketMatches {
    let mut count = 0;
    let mut shown = Vec::new();
    let mut matched_keys_lower = Vec::new();
    for item in items.map(Vec::as_slice).unwrap_or(&[]) {
        let Some(key) = item_key(item) else { continue };
        if !matches_pattern(key, needle_lower) {
            continue;
        }
        count += 1;
        if shown.len() < QUERY_MATCH_LIMIT {
            shown.push(item.clone());
        }
        let lower = key.to_lowercase();
        if !matched_keys_lower.contains(&lower) {
            matched_keys_lower.push(lower);
        }
    }
    BucketMatches {
        count,
        shown,
        matched_keys_lower,
    }
}

/// Every finding array the trees output carries: each tree's own `findings` plus the top-level
/// `crossLayerFindings`, in output order.
pub(super) fn all_findings(analysis: &Value) -> Vec<&Value> {
    let mut findings = Vec::new();
    for tree in analysis["trees"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        for f in tree["output"]["findings"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            findings.push(f);
        }
    }
    for f in analysis["crossLayerFindings"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        findings.push(f);
    }
    findings
}

/// `suggestions` (not-found only): distinct keys in engine order whose last path segment equals
/// the pattern's, falling back to keys containing any single `/`-segment of the pattern.
pub(super) fn suggestions(cross_layer: &Value, pattern: &str) -> Vec<String> {
    let mut distinct: Vec<&str> = Vec::new();
    for (bucket, _) in BUCKETS {
        for item in cross_layer[bucket]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            if let Some(key) = item.get("key").and_then(Value::as_str) {
                if !distinct.contains(&key) {
                    distinct.push(key);
                }
            }
        }
    }
    let pattern_last = last_segment_lower(pattern);
    let mut picked: Vec<String> = distinct
        .iter()
        .filter(|k| !pattern_last.is_empty() && last_segment_lower(k) == pattern_last)
        .map(|k| k.to_string())
        .collect();
    if picked.is_empty() {
        let segments: Vec<String> = pattern
            .split('/')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        picked = distinct
            .iter()
            .filter(|k| {
                let lower = k.to_lowercase();
                segments.iter().any(|seg| lower.contains(seg))
            })
            .map(|k| k.to_string())
            .collect();
    }
    picked.truncate(QUERY_SUGGESTIONS_LIMIT);
    picked
}
