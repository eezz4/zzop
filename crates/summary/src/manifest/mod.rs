//! `zzop manifest` — the structural CONTRACT MANIFEST of a cross-layer run, and (in [`diff`]) the
//! delta between two of them. Two pure functions, zero engine state: zzop produces manifests, the
//! USER keeps them (commit one next to the code, like `scripts/max-file-lines-baseline.txt`) — we
//! never store, name, or garbage-collect a snapshot, so this feature adds no cleanup/determinism
//! duty of its own.
//!
//! ## Why this exists at all (the cap is the whole argument)
//! "Just diff two runs yourself" holds only BELOW the caps. The shipped reply is not raw output, it
//! is a CAPPED summary (`crossLayer.edges` <= `DEFAULT_EDGES_LIMIT`, `bucketKeys` <=
//! `DEFAULT_BUCKET_KEYS_LIMIT`, findings/degraded <= their own limits — see
//! `docs/contracts/surface-parity.json`). Above a cap, two runs' texts still agree on the COUNTS
//! while saying nothing about WHICH route left the join. This module is the surface that stays
//! structurally readable there, and it stays small by carrying IDENTITY ONLY.
//!
//! ## What a manifest carries, and what it deliberately does not
//! - `tool` — `zzop_facade::version_string()` verbatim: release version + every parser fingerprint.
//!   The honesty gate ([`diff`] refuses a cross-build diff unless told otherwise): without it, OUR
//!   extraction improvement reads as "the other team broke the contract".
//! - `sources[]` — `{sourceId, joinContributionZero, degraded}`: the coverage facts [`diff`] needs to
//!   tell a DELETION from a BLINDNESS. Deliberately no `root`: an absolute path differs between a
//!   laptop and CI, which would make every manifest un-diffable across the two machines that most
//!   need to compare.
//! - `provides[]` `{kind, key, source}` · `edges[]` `{kind, key, from, to}` · `buckets[]`
//!   `{bucket, kind, key, source}` — sorted and deduped, so the output is byte-identical run over run
//!   and a pure refactor (files moved, lines shifted) produces an EMPTY diff.
//! - NOT carried: file/line (a rename would drown the real signal), findings (finding identity drifts
//!   with line numbers, and severity totals already ride uncapped counts — v1 is structural contract
//!   state only), and no `manifestVersion` (a schema change ships in a zzop release, so the `tool`
//!   gate already refuses those two manifests to each other).

mod diff;
#[cfg(test)]
mod tests;

pub use diff::diff_manifests_json;

/// The three identity relations a manifest carries. **Policy-value tier T1 + pin**
/// (`rule-quality.md` §6): the READER's two uses — the shape check that rejects a non-manifest
/// argument and the delta loop itself ([`diff`]) — share this one symbol rather than each spelling
/// the names, so a fourth relation cannot be validated and then not diffed. The PRODUCER
/// ([`project`]) deliberately writes its own literal keys (a positional zip against this const would
/// publish one relation's rows under another's name the day it is reordered) and is held equal by
/// `tests::the_manifests_top_level_keys_are_exactly_the_shared_relation_vocabulary`. Nothing outside
/// this module names the three mechanically — `docs/modules/facade.md` documents them in prose, like
/// every other field name on that page.
const RELATIONS: [&str; 3] = ["provides", "edges", "buckets"];

/// Builds the structural manifest for a cross-layer run — same two source modes as `cross_summary`
/// (`paths` XOR `configPath`, both through `crate::trees::load_trees_request`), same single
/// `analyzeTrees` engine path, different projection: identity instead of a capped summary.
pub fn manifest_json(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    let loaded = crate::trees::load_trees_request("manifest", paths, config_path)?;
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
    Ok(project(&v))
}

/// The pure projection `analyzeTrees` output -> manifest JSON. Split out from the analysis call so the
/// whole shape is unit-testable from a literal engine output with no filesystem.
fn project(v: &serde_json::Value) -> String {
    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);

    // Per-source coverage facts — the blindness half of the honesty contract. Sorted by sourceId so
    // tree ORDER in the request never changes the manifest bytes.
    let mut sources: Vec<serde_json::Value> = trees
        .iter()
        .map(|t| {
            let coverage = &t["output"]["coverage"];
            serde_json::json!({
                "sourceId": t["sourceId"],
                // `true` = this tree extracted zero JOINABLE io, so it is invisible to the join and
                // every "missing" verdict about it is blindness, not deletion.
                "joinContributionZero": coverage["joinContributionZero"],
                "degraded": coverage["degraded"],
            })
        })
        .collect();
    // Sorted by sourceId EXPLICITLY, not by `sort_key`'s serialized-text order like the identity
    // relations below: a source row mixes identity (`sourceId`) with facts that legitimately change
    // between runs (`degraded`, `joinContributionZero`), and those serialize alphabetically AHEAD of
    // `sourceId` — so a text sort would re-order the whole array the moment one tree's coverage moved,
    // churning the git diff of a committed manifest for a non-identity reason.
    sources.sort_by(|a, b| a["sourceId"].as_str().cmp(&b["sourceId"].as_str()));

    // `ir` is the ONLY place the full provide list lives (the capped reply never carries it) — the
    // manifest reads it for identity `(kind, key, source)` and nothing else: no file, no line, no
    // symbol, no body shape.
    let mut provides: Vec<serde_json::Value> = Vec::new();
    for t in trees {
        let source = &t["sourceId"];
        for p in t["output"]["ir"]["io"]["provides"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            provides.push(serde_json::json!({
                "kind": p["kind"], "key": p["key"], "source": source,
            }));
        }
    }
    dedup_sorted(&mut provides);

    let cl = &v["crossLayer"];
    let mut edges: Vec<serde_json::Value> = cl["edges"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|e| {
            serde_json::json!({
                "kind": e["kind"], "key": e["key"],
                "from": e["from"]["source"], "to": e["to"]["source"],
            })
        })
        .collect();
    dedup_sorted(&mut edges);

    let mut buckets: Vec<serde_json::Value> = Vec::new();
    for bucket in crate::output::KEY_BUCKETS {
        for item in cl[bucket].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            // Same key fallback `bucket_keys` uses: an unresolved consume has no key, so its `raw`
            // source text IS its identity. An item with neither is skipped, never guessed.
            let key = item
                .get("key")
                .filter(|v| v.is_string())
                .or_else(|| item.get("raw").filter(|v| v.is_string()));
            let Some(key) = key else { continue };
            buckets.push(serde_json::json!({
                "bucket": bucket, "kind": item["kind"], "key": key, "source": item["source"],
            }));
        }
    }
    dedup_sorted(&mut buckets);

    // Each relation is written under its own literal key rather than zipped against `RELATIONS`'
    // order — a positional zip would silently publish `provides` under `edges` the day someone
    // reorders that const. The two are held equal by a pin instead (see `tests`), which is the
    // direction that fails loudly.
    //
    // Pretty-printed like every other summary this crate emits, and byte-identical run over run:
    // every array above is sorted, and `serde_json::Map` is a `BTreeMap` here (no `preserve_order`
    // feature), so object key order is alphabetical rather than insertion-dependent.
    serde_json::to_string_pretty(&serde_json::json!({
        "tool": zzop_facade::version_string(),
        "sources": sources,
        "provides": provides,
        "edges": edges,
        "buckets": buckets,
    }))
    .expect("manifest is plain JSON values")
}

/// Total order for identity rows: their compact JSON text. Each row is a flat object whose every
/// field is part of the row's IDENTITY, and `serde_json::Map` is a `BTreeMap` here (no
/// `preserve_order` feature), so the serialized form is a stable alphabetical-by-key concatenation of
/// exactly those fields — a total order for free, with no comparator per row shape. This holds only
/// because no mutable fact rides an identity row; the `sources` array, which does carry mutable
/// coverage numbers, is sorted by its `sourceId` explicitly instead (see the call site).
fn dedup_sorted(rows: &mut Vec<serde_json::Value>) {
    rows.sort_by_key(serde_json::Value::to_string);
    rows.dedup_by(|a, b| a == b);
}
