//! Shared `#[serde(serialize_with = ...)]` helpers for `HashMap`-typed fields that reach a serialized
//! output shape (`AnalyzeOutputView` in `crates/facade/src/lib.rs` and everything it borrows from).
//!
//! ## Why this exists
//! `analyze()`/`analyzeTrees()`/`analyzeEnvelope()` are a locked determinism contract: two identical runs
//! must produce byte-identical JSON. A `HashMap` has no defined iteration order (it is randomized per
//! process via `RandomState` for DoS-resistance), so any field typed as one serializes in an arbitrary,
//! run-to-run-varying key order. Deserialization is unaffected either way (a JSON object's key order never
//! changes which entries end up in the resulting map), so this is a serialize-only concern — these helpers
//! are applied via `serialize_with` only, never `with` (which would also require a matching custom
//! `deserialize_with`), leaving the field's Rust type and its `Deserialize` impl untouched.
//!
//! Deliberately NOT a blanket switch to `BTreeMap`: the fields this fixes (`MinimalIr::dep`,
//! `MinimalIr::loc`, `FileNode::tag_counts`, `FileNode::author_commits`, `FileNode::recent_author_commits`)
//! are all populated once, in bulk, then serialized — sorting only at the serialize boundary is strictly
//! cheaper than paying `BTreeMap`'s O(log n) insert cost on every build-time write, and avoids widening the
//! diff to touch `DepGraph`'s public type alias (`HashMap<String, Vec<String>>`, used pervasively as a
//! build-time accumulator across `zzop-core`/`zzop-engine`/`zzop-metrics`) or `FileNode`'s public field types.

use std::collections::HashMap;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Serializes a `HashMap<String, V>` as a JSON object with keys in ascending sorted order, making the
/// output byte-deterministic across runs regardless of the map's actual (hasher-randomized) iteration
/// order. Use via `#[serde(serialize_with = "zzop_core::serde_util::sorted_map")]`.
pub fn sorted_map<S, V>(map: &HashMap<String, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    V: Serialize,
{
    let mut entries: Vec<(&String, &V)> = map.iter().collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut out = serializer.serialize_map(Some(entries.len()))?;
    for (k, v) in entries {
        out.serialize_entry(k, v)?;
    }
    out.end()
}

/// [`sorted_map`]'s `Option<HashMap<...>>` counterpart, for fields like `FileNode::author_commits` that
/// pair `serialize_with` with `#[serde(skip_serializing_if = "Option::is_none")]` — `serialize_with`
/// receives the whole field type (`&Option<HashMap<...>>`), not the unwrapped map, so `sorted_map` itself
/// cannot be reused directly for an `Option`-wrapped field.
pub fn sorted_map_option<S, V>(
    map: &Option<HashMap<String, V>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    V: Serialize,
{
    match map {
        Some(m) => sorted_map(m, serializer),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Wrapper {
        #[serde(serialize_with = "sorted_map")]
        m: HashMap<String, u32>,
    }

    #[derive(Serialize)]
    struct OptWrapper {
        #[serde(
            serialize_with = "sorted_map_option",
            skip_serializing_if = "Option::is_none"
        )]
        m: Option<HashMap<String, u32>>,
    }

    /// The whole point: build a map whose hasher-randomized iteration order is very unlikely to already be
    /// sorted (many keys, deliberately out-of-order insertion), then check the serialized JSON text itself
    /// (not a `Value` structural comparison, which loses order) matches the ascending-key form exactly.
    #[test]
    fn sorted_map_emits_keys_in_ascending_order_regardless_of_insertion_order() {
        let mut m = HashMap::new();
        for k in ["zebra", "mango", "apple", "kiwi", "banana", "fig", "date"] {
            m.insert(k.to_string(), k.len() as u32);
        }
        let json = serde_json::to_string(&Wrapper { m }).unwrap();
        assert_eq!(
            json,
            r#"{"m":{"apple":5,"banana":6,"date":4,"fig":3,"kiwi":4,"mango":5,"zebra":5}}"#
        );
    }

    #[test]
    fn sorted_map_empty_map_serializes_to_empty_object() {
        let json = serde_json::to_string(&Wrapper { m: HashMap::new() }).unwrap();
        assert_eq!(json, r#"{"m":{}}"#);
    }

    #[test]
    fn sorted_map_option_some_is_sorted_and_none_is_skipped() {
        let mut m = HashMap::new();
        m.insert("b".to_string(), 2u32);
        m.insert("a".to_string(), 1u32);
        let json = serde_json::to_string(&OptWrapper { m: Some(m) }).unwrap();
        assert_eq!(json, r#"{"m":{"a":1,"b":2}}"#);

        let json_none = serde_json::to_string(&OptWrapper { m: None }).unwrap();
        assert_eq!(json_none, r#"{}"#);
    }

    /// Two independently-built maps with the same entries inserted in different orders (simulating
    /// process-to-process hasher randomization) must serialize byte-identically.
    #[test]
    fn sorted_map_is_stable_across_different_insertion_orders() {
        let mut m1 = HashMap::new();
        for k in ["one", "two", "three", "four", "five"] {
            m1.insert(k.to_string(), 1u32);
        }
        let mut m2 = HashMap::new();
        for k in ["five", "four", "three", "two", "one"] {
            m2.insert(k.to_string(), 1u32);
        }
        let j1 = serde_json::to_string(&Wrapper { m: m1 }).unwrap();
        let j2 = serde_json::to_string(&Wrapper { m: m2 }).unwrap();
        assert_eq!(j1, j2);
    }
}
