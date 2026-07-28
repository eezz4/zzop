//! Deterministic nearest-key fallback for `check_endpoint`'s `not-found` `suggestions`.
//!
//! The facade's own `suggestions` (crates/facade/src/query/scan.rs) is substring-driven: it matches
//! the pattern's last `/`-segment (or any segment) literally against every io key. A realistic typo
//! — `atricles` for `articles` — shares no substring with the real key, so that pass returns `[]`
//! even though a near-miss key plainly exists; meanwhile a fake path that happens to CONTAIN a real
//! substring gets a full page of suggestions. This module is the fallback `check_endpoint` runs when
//! the substring pass comes back empty: rank every distinct io key by lowest Levenshtein distance
//! between the pattern and the key's lowercase `/`-split token parts, keep only keys within a sane
//! distance (garbage patterns then still yield `[]`), cap at the facade's own suggestion limit, and
//! tie-break lexicographically so the result is deterministic regardless of bucket/scan order.

use serde_json::Value;

/// Mirrors `zzop_facade::query`'s own cap (`QUERY_SUGGESTIONS_LIMIT`) — the fallback never offers
/// more candidates than the substring pass would have.
const SUGGESTIONS_LIMIT: usize = 10;

/// Cross-layer bucket names carrying keyed io items — the same set `queryIo()` scans.
const BUCKETS: [&str; 6] = [
    "edges",
    "unconsumedProvides",
    "unprovidedConsumes",
    "unresolvedConsumes",
    "externalConsumes",
    "ambiguousConsumes",
];

/// A bucket item's queryable identity: `key` for every resolved shape, falling back to `raw` for an
/// unresolved consume with no resolved key — same fallback the facade's own scan uses.
fn item_key(item: &Value) -> Option<&str> {
    item.get("key")
        .and_then(Value::as_str)
        .or_else(|| item.get("raw").and_then(Value::as_str))
}

/// Plain Levenshtein edit distance (insert/delete/substitute), over `char`s so non-ASCII keys are
/// measured correctly. Classic two-row DP — no crate needed for ~15 lines.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Ranks every distinct io key across the cross-layer buckets against `pattern`, returning the
/// nearest ones (lowest Levenshtein distance to any of the key's lowercase `/`-split token parts,
/// or the whole lowercased key). Only keys within half the pattern's length qualify, so a garbage
/// pattern with no real near-miss still returns `[]` rather than dumping the whole key set. Empty
/// pattern returns `[]` (nothing meaningful to rank against).
///
/// Returns `(capped list, total that qualified before the cap)` — the caller discloses the difference.
pub(crate) fn nearest_keys(cross_layer: &Value, pattern: &str) -> (Vec<String>, usize) {
    let needle = pattern.to_lowercase();
    if needle.is_empty() {
        return (Vec::new(), 0);
    }
    let threshold = (needle.chars().count() / 2).max(1);

    let mut distinct: Vec<&str> = Vec::new();
    for bucket in BUCKETS {
        for item in cross_layer[bucket]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            if let Some(key) = item_key(item) {
                if !distinct.contains(&key) {
                    distinct.push(key);
                }
            }
        }
    }

    let mut ranked: Vec<(usize, &str)> = distinct
        .into_iter()
        .filter_map(|key| {
            let lower = key.to_lowercase();
            let mut tokens: Vec<&str> = lower
                .split('/')
                .map(str::trim)
                .filter(|seg| !seg.is_empty())
                .collect();
            tokens.push(&lower);
            let best = tokens.iter().map(|t| levenshtein(&needle, t)).min()?;
            (best <= threshold).then_some((best, key))
        })
        .collect();

    // Distance first, then lexicographic — deterministic regardless of the buckets' own iteration
    // order (a same-distance tie must not depend on which bucket happened to hold the key).
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    // Total BEFORE the cap rides back with the list. This lane is the one that most needs it: it runs
    // only when the facade's substring pass found NOTHING, so the facade's own `suggestionsTruncated`
    // is always absent here (its total was 0) and this cap would otherwise be the single undisclosed
    // truncation on the whole reply. A `not-found` verdict plus ten keys that do not include the one
    // the caller meant reads as "it does not exist".
    let total = ranked.len();
    ranked.truncate(SUGGESTIONS_LIMIT);
    (
        ranked.into_iter().map(|(_, k)| k.to_string()).collect(),
        total,
    )
}

#[cfg(test)]
mod tests {
    use super::nearest_keys;
    use serde_json::json;

    fn cross_layer_with_keys(keys: &[&str]) -> serde_json::Value {
        let unconsumed: Vec<serde_json::Value> = keys.iter().map(|k| json!({ "key": k })).collect();
        json!({
            "edges": [],
            "unconsumedProvides": unconsumed,
            "unprovidedConsumes": [],
            "unresolvedConsumes": [],
            "externalConsumes": [],
            "ambiguousConsumes": [],
        })
    }

    #[test]
    fn typo_finds_the_near_miss_route() {
        let cl = cross_layer_with_keys(&["GET /api/articles", "GET /api/users"]);
        let (got, _) = nearest_keys(&cl, "atricles");
        assert!(
            got.iter().any(|k| k.contains("articles")),
            "expected an articles route among {got:?}"
        );
    }

    #[test]
    fn garbage_pattern_still_returns_nothing() {
        let cl = cross_layer_with_keys(&["GET /api/articles", "GET /api/users", "DATABASE_URL"]);
        let (got, _) = nearest_keys(&cl, "nonexistent-route-xyz");
        assert!(got.is_empty(), "garbage must not force a guess: {got:?}");
    }

    #[test]
    fn empty_pattern_returns_nothing() {
        let cl = cross_layer_with_keys(&["GET /api/articles"]);
        assert_eq!(nearest_keys(&cl, ""), (Vec::<String>::new(), 0));
    }

    /// Seals the DISCLOSURE half of the cap: the returned total counts everything that qualified, not
    /// what survived `SUGGESTIONS_LIMIT`. Without it `check_endpoint` shipped ten keys and no sign that
    /// more existed — on a `not-found` verdict, which reads as "the key does not exist".
    #[test]
    fn the_total_counts_qualifying_keys_beyond_the_cap() {
        let keys: Vec<String> = (0..15).map(|i| format!("GET /api/articles{i}")).collect();
        let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let cl = cross_layer_with_keys(&refs);
        let (shown, total) = nearest_keys(&cl, "articles");
        assert_eq!(shown.len(), 10, "the cap still applies: {shown:?}");
        assert_eq!(
            total, 15,
            "but the caller learns how many there really were"
        );
    }

    #[test]
    fn same_input_yields_the_same_order_every_time() {
        let cl = cross_layer_with_keys(&[
            "GET /api/articles",
            "GET /api/articels",
            "GET /api/artikles",
        ]);
        let first = nearest_keys(&cl, "articles");
        for _ in 0..5 {
            assert_eq!(
                nearest_keys(&cl, "articles"),
                first,
                "must be deterministic"
            );
        }
    }

    #[test]
    fn ties_break_lexicographically() {
        // "artacle" is distance 1 from both "article" and "aatacle" would not tie; construct two
        // keys equidistant from the pattern to pin the tie-break rule directly.
        let cl = cross_layer_with_keys(&["GET /api/bxxxxx", "GET /api/axxxxx"]);
        let (got, _) = nearest_keys(&cl, "xxxxx");
        assert_eq!(
            got,
            vec!["GET /api/axxxxx".to_string(), "GET /api/bxxxxx".to_string()],
            "equal-distance keys must sort lexicographically: {got:?}"
        );
    }
}
