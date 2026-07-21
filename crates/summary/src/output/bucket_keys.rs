//! `cross_repo`'s `bucketKeys`/`bucketKeySites` shaping.

/// Default cap for `cross_repo`'s `bucketKeys` distinct-key lists (see `bucket_keys`).
pub const DEFAULT_BUCKET_KEYS_LIMIT: usize = 20;

/// The five non-edge cross-layer buckets, in engine (`CrossLayerResult`) field order.
const KEY_BUCKETS: [&str; 5] = [
    "unconsumedProvides",
    "unprovidedConsumes",
    "unresolvedConsumes",
    "externalConsumes",
    "ambiguousConsumes",
];

/// `cross_repo`'s `bucketKeys`: per non-edge bucket, up to `DEFAULT_BUCKET_KEYS_LIMIT` DISTINCT keys
/// (deduped, engine order preserved) so an agent can see WHICH keys sit in a bucket instead of only
/// how many. An unresolved consume (`key: null`) contributes its `raw` expression when recorded —
/// nothing otherwise (never guessed). Returns `(bucketKeys, bucketKeysTruncated?, bucketKeySites)`:
/// the truncation value is `Some({bucket: remainingDistinctCount})` only when a bucket's distinct-key
/// list was capped — the same explicit-truncation-disclosure stance as `shape_list`, in a per-bucket
/// remainder shape; `bucketKeySites` mirrors `bucketKeys`' shape exactly (same buckets, same order,
/// same length after capping) but each entry is the FIRST call site backing that distinct key, as
/// `"file:line"` — every bucket item already carries `file`/`line` (the engine's `IoProvide`/
/// `IoConsume` facts, flattened onto the bucket entry), so this is a same-layer read, never a facade
/// change; `null` only if an item is missing one of the two (never guessed).
pub(crate) fn bucket_keys(
    cross_layer: &serde_json::Value,
) -> (
    serde_json::Value,
    Option<serde_json::Value>,
    serde_json::Value,
) {
    let mut keys_out = serde_json::Map::new();
    let mut sites_out = serde_json::Map::new();
    let mut truncated = serde_json::Map::new();
    for bucket in KEY_BUCKETS {
        let mut seen = std::collections::HashSet::new();
        let mut distinct: Vec<&str> = Vec::new();
        let mut sites: Vec<serde_json::Value> = Vec::new();
        for item in cross_layer[bucket]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let key = item
                .get("key")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("raw").and_then(|v| v.as_str()));
            if let Some(k) = key {
                if seen.insert(k) {
                    distinct.push(k);
                    let site = match (
                        item.get("file").and_then(|v| v.as_str()),
                        item.get("line").and_then(|v| v.as_u64()),
                    ) {
                        (Some(file), Some(line)) => serde_json::json!(format!("{file}:{line}")),
                        _ => serde_json::Value::Null,
                    };
                    sites.push(site);
                }
            }
        }
        if distinct.len() > DEFAULT_BUCKET_KEYS_LIMIT {
            truncated.insert(
                bucket.to_string(),
                serde_json::json!(distinct.len() - DEFAULT_BUCKET_KEYS_LIMIT),
            );
            distinct.truncate(DEFAULT_BUCKET_KEYS_LIMIT);
            sites.truncate(DEFAULT_BUCKET_KEYS_LIMIT);
        }
        keys_out.insert(bucket.to_string(), serde_json::json!(distinct));
        sites_out.insert(bucket.to_string(), serde_json::Value::Array(sites));
    }
    (
        serde_json::Value::Object(keys_out),
        (!truncated.is_empty()).then_some(serde_json::Value::Object(truncated)),
        serde_json::Value::Object(sites_out),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume(key: Option<&str>, raw: Option<&str>, file: &str, line: u64) -> serde_json::Value {
        serde_json::json!({ "key": key, "raw": raw, "file": file, "line": line })
    }

    #[test]
    fn distinct_keys_are_deduped_capped_and_disclose_their_remainder() {
        let mut cross_layer = serde_json::json!({
            "unconsumedProvides": [],
            "unprovidedConsumes": [],
            "unresolvedConsumes": [],
            "externalConsumes": [],
            "ambiguousConsumes": [],
        });
        let items: Vec<serde_json::Value> = (0..DEFAULT_BUCKET_KEYS_LIMIT + 3)
            .map(|i| {
                consume(
                    Some(&format!("GET /x/{i}")),
                    None,
                    "src/api.ts",
                    i as u64 + 1,
                )
            })
            .collect();
        cross_layer["unprovidedConsumes"] = serde_json::json!(items);
        let (keys, truncated, sites) = bucket_keys(&cross_layer);
        let shown = keys["unprovidedConsumes"].as_array().unwrap();
        assert_eq!(shown.len(), DEFAULT_BUCKET_KEYS_LIMIT);
        assert_eq!(
            truncated.unwrap()["unprovidedConsumes"],
            3,
            "remainder disclosed, never silent"
        );
        let site_list = sites["unprovidedConsumes"].as_array().unwrap();
        assert_eq!(site_list.len(), shown.len(), "sites parallel to keys");
        assert_eq!(site_list[0], "src/api.ts:1");
    }

    #[test]
    fn an_unresolved_consume_contributes_its_raw_expression_as_the_key() {
        let mut cross_layer = serde_json::json!({
            "unconsumedProvides": [], "unprovidedConsumes": [], "unresolvedConsumes": [],
            "externalConsumes": [], "ambiguousConsumes": [],
        });
        cross_layer["unresolvedConsumes"] =
            serde_json::json!([consume(None, Some("usersUrl(x)"), "src/api.ts", 7)]);
        let (keys, _, sites) = bucket_keys(&cross_layer);
        assert_eq!(keys["unresolvedConsumes"][0], "usersUrl(x)");
        assert_eq!(sites["unresolvedConsumes"][0], "src/api.ts:7");
    }

    #[test]
    fn a_site_missing_file_or_line_is_null_never_guessed() {
        let mut cross_layer = serde_json::json!({
            "unconsumedProvides": [], "unprovidedConsumes": [], "unresolvedConsumes": [],
            "externalConsumes": [], "ambiguousConsumes": [],
        });
        cross_layer["unprovidedConsumes"] = serde_json::json!([{ "key": "GET /x", "raw": null }]);
        let (_, _, sites) = bucket_keys(&cross_layer);
        assert!(sites["unprovidedConsumes"][0].is_null());
    }
}
