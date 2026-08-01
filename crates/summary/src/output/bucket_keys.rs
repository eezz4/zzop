//! The cross-layer join summary's `distinctBucketKeys`/`distinctBucketKeyFirstSites` shaping — UNCAPPED.
//!
//! ## Why the names say `distinct` and `first` (2026-07-31)
//! These two fields sit beside a `buckets` object that counts something ELSE. `buckets.X` is a count of
//! RAW ROWS — one per recorded call site — while this module DEDUPES, so `buckets.X` and the length of
//! the key list here are legitimately different numbers (measured on the `cases/` corpus:
//! `unprovidedConsumes` is 23 rows over 14 distinct keys, `edges` 18 over 11). Under the old names
//! (`bucketKeys`/`bucketKeySites`) a reader checking `buckets.X == len(bucketKeys.X)` got a mismatch with
//! nothing on the wire to explain it. The names now state the membership RULE, and `bucketMeaning` (see
//! `crate::cross`) states the arithmetic in words — the same repair the graph census already shipped for
//! the identical confusion ("60 rows are 4 relations", `crate::graph::collect`).
//!
//! `distinctBucketKeyFirstSites` carries a second correction in its name: it is one site per key, the
//! FIRST recorded one, not every site backing that key. The MCP tool description said so; the plural
//! `bucketKeySites` did not, and the CLI twin shipped the bare name with no description to read.
//!
//! ## Uncapped, still
//! There was a `DEFAULT_BUCKET_KEYS_LIMIT = 20` here until 2026-07-29, with the usual
//! `bucketKeysTruncated` remainder disclosure beside it. The cap is gone, by user decision, and with it
//! the truncation branch, the disclosure field, `snapshot.mjs`'s abort-on-truncation path and its
//! `--tolerate-bucket-key-cap` escape hatch. What made it go was not the cost of the cap but the cost of
//! everything the cap needed in order to stay honest: five moving parts existed so that twenty keys could
//! be shown instead of all of them.
//!
//! It also failed on its own terms. The `cases/` benchmark hit the cap in normal use — a batch adding
//! negative-case trees truncated `unconsumedProvides` and scored ZERO lines, because `snapshot.mjs`
//! correctly refuses to grade a truncated snapshot. The wall was invisible until it was hit, and the
//! escape hatch could not be used (grading a truncated snapshot is grading the wrong thing). A cap whose
//! own repository routes around it is not paying for itself.
//!
//! KNOWN AND ACCEPTED: on a large repo a single list field here can now run to hundreds of lines, and the
//! primary consumer is an agent reading a reply. That cost was weighed and taken. If it is ever MEASURED to
//! hurt, the answer is not this cap back — it is a deliberate paging or query surface. (`zzop facts` and
//! `zzop manifest` were already uncapped, so the large-output case has been shipping on those lanes all
//! along.)

/// The five non-edge cross-layer buckets, in engine (`CrossLayerResult`) field order. Shared
/// (`pub(crate)`) with `crate::manifest`, which walks the same five buckets to record bucket
/// MEMBERSHIP: one vocabulary, so a sixth bucket cannot reach one surface and not the other.
/// `crate::cross`'s `bucketMeaning` sentence names this same set, so the prose and the walk cannot
/// disagree about which buckets the rows-vs-distinct arithmetic applies to.
pub(crate) const KEY_BUCKETS: [&str; 5] = [
    "unconsumedProvides",
    "unprovidedConsumes",
    "unresolvedConsumes",
    "externalConsumes",
    "ambiguousConsumes",
];

/// `distinctBucketKeys`: per non-edge bucket, EVERY distinct key (deduped, engine order preserved) so an
/// agent can see WHICH keys sit in a bucket instead of only how many. An unresolved consume
/// (`key: null`) contributes its `raw` expression when recorded — nothing otherwise (never guessed).
///
/// Returns `(distinctBucketKeys, distinctBucketKeyFirstSites)`. There is no truncation value: the list is
/// complete by construction, which is the strongest form of the never-silent stance — nothing is dropped,
/// so nothing needs disclosing. `distinctBucketKeyFirstSites` mirrors the key map's shape exactly (same
/// buckets, same order, same length) but each entry is the FIRST call site backing that distinct key, as
/// `"file:line"` — every bucket item already carries `file`/`line` (the engine's `IoProvide`/`IoConsume`
/// facts, flattened onto the bucket entry), so this is a same-layer read, never a facade change; `null`
/// only if an item is missing one of the two (never guessed).
///
/// The DEDUP here is the whole reason the sibling `buckets` counts differ from these lengths — see the
/// module doc, and `crate::cross`'s `bucketMeaning`, which puts that relationship on the wire.
pub(crate) fn distinct_bucket_keys(
    cross_layer: &serde_json::Value,
) -> (serde_json::Value, serde_json::Value) {
    let mut keys_out = serde_json::Map::new();
    let mut sites_out = serde_json::Map::new();
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
        keys_out.insert(bucket.to_string(), serde_json::json!(distinct));
        sites_out.insert(bucket.to_string(), serde_json::Value::Array(sites));
    }
    (
        serde_json::Value::Object(keys_out),
        serde_json::Value::Object(sites_out),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume(key: Option<&str>, raw: Option<&str>, file: &str, line: u64) -> serde_json::Value {
        serde_json::json!({ "key": key, "raw": raw, "file": file, "line": line })
    }

    /// The old cap was 20 and `cases/` sat at exactly 20 the day it was removed. 23 is that number plus
    /// the batch that could not be added under it — a size that used to truncate and now must not.
    #[test]
    fn distinct_keys_are_deduped_and_never_truncated() {
        let mut cross_layer = serde_json::json!({
            "unconsumedProvides": [],
            "unprovidedConsumes": [],
            "unresolvedConsumes": [],
            "externalConsumes": [],
            "ambiguousConsumes": [],
        });
        let items: Vec<serde_json::Value> = (0..23)
            .map(|i| {
                consume(
                    Some(&format!("GET /x/{i}")),
                    None,
                    "src/api.ts",
                    i as u64 + 1,
                )
            })
            // One exact duplicate, so "deduped" is still pinned alongside "uncapped".
            .chain(std::iter::once(consume(
                Some("GET /x/0"),
                None,
                "src/other.ts",
                99,
            )))
            .collect();
        cross_layer["unprovidedConsumes"] = serde_json::json!(items);
        let (keys, sites) = distinct_bucket_keys(&cross_layer);
        let shown = keys["unprovidedConsumes"].as_array().unwrap();
        assert_eq!(shown.len(), 23, "every distinct key, no cap");
        let site_list = sites["unprovidedConsumes"].as_array().unwrap();
        assert_eq!(site_list.len(), shown.len(), "sites parallel to keys");
        assert_eq!(
            site_list[0], "src/api.ts:1",
            "FIRST site wins for a dup key"
        );
    }

    #[test]
    fn an_unresolved_consume_contributes_its_raw_expression_as_the_key() {
        let mut cross_layer = serde_json::json!({
            "unconsumedProvides": [], "unprovidedConsumes": [], "unresolvedConsumes": [],
            "externalConsumes": [], "ambiguousConsumes": [],
        });
        cross_layer["unresolvedConsumes"] =
            serde_json::json!([consume(None, Some("usersUrl(x)"), "src/api.ts", 7)]);
        let (keys, sites) = distinct_bucket_keys(&cross_layer);
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
        let (_, sites) = distinct_bucket_keys(&cross_layer);
        assert!(sites["unprovidedConsumes"][0].is_null());
    }
}
