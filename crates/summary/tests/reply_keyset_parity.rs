//! The surface-parity registry's DELIVERY-SHAPE half, pinned against a REAL reply (D10).
//!
//! # The hole this closes
//! `crates/engine/tests/rule_contracts/surface_parity.rs` TEST 3 asks whether the MCP lane forwards
//! the `carry` rows and never the `omit` ones, and it answers by scanning source text for the field's
//! own key literal (`"configWarnings":`). That matcher is structurally blind to the way these replies
//! are actually built: `crates/facade/src/output.rs` derives its keys with
//! `#[serde(rename_all = "camelCase")]`, so a camelCase key can appear in a shipped reply while the
//! literal appears in ZERO source files. The consequence, recorded in that test's own doc: the `omit`
//! direction only ever catches a `json!`-style re-emission, and a handler forwarding a whole struct
//! puts every `omit` field on the wire silently.
//!
//! So this file asks the same question a different way — it builds one real reply per surface and
//! reads the keys that came out. Text cannot see derived keys; a JSON value cannot miss them.
//!
//! # The keys no row could ever govern (added 2026-08-29)
//! The registry's field rows are pinned to the FACADE VIEWS, so a reply key that is not a view
//! field — a projection (`sources`, `architecture`), a legend (`bucketMeaning`), a census
//! (`buckets`, `coverageGaps`), or a per-tree field the join lifts to its root (`configWarnings`,
//! `packsLoadedMeaning`, `nativeAnalyses`) — can hold no row, no status, and therefore no guard at
//! all. That is a hole in the registry's SUBJECT rather than in its checks, and it was paid for:
//! `nativeAnalyses` sat at `carry-conditional` on its own row while being absent from every byte of
//! the cross reply, root and `sources[]` alike, and every guard stayed green. The registry now
//! declares those keys in `_replyRootKeys` and [`assert_shape`] holds the declaration against a
//! real reply in both directions: an undeclared key fails, and a key declared `always` that is
//! missing fails. What stays uncovered, deliberately, is anything NESTED — the eight fields of a
//! `sources[]` element are not top-level keys and no keyset can see them.
//!
//! # Deliberately ONE reply per surface, top-level keys only
//! Not a matrix over every input combination. The registry is a claim about SHAPE ("this field either
//! reaches the reply or does not"), and one honest response answers it. A combinatorial harness would
//! be a second implementation of the shaping rules, and the moment it disagreed with the first nobody
//! could say which was right — the two-owners failure this repo keeps paying for. `carry-conditional`
//! rows are therefore asserted only in the `omit`-is-absent direction: whether their precondition held
//! in THIS fixture is not something a keyset can know.
//!
//! # Why it lives in crates/summary/tests
//! The backlog row that ordered this work assumed `crates/engine`, which would have needed a
//! dev-dependency on a product crate — a dependency cycle Cargo permits but nobody should reach for
//! when an alternative exists. `crates/summary` already sits above the whole stack and already hosts
//! the sibling end-to-end reply tests (`host_dispatch.rs`), so the pin costs no new edge at all.

use std::fs;

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

const REGISTRY: &str = include_str!("../../../docs/contracts/surface-parity.json");

/// Rows whose `mcpAnalyzeReply` verdict is exactly `omit` — the set that must NOT appear.
fn omitted_fields(surface_block: &serde_json::Value) -> Vec<String> {
    surface_block
        .as_object()
        .expect("a surface block is an object of field -> row")
        .iter()
        .filter(|(name, _)| !name.starts_with('_'))
        .filter(|(_, row)| row.get("mcpAnalyzeReply").and_then(|v| v.as_str()) == Some("omit"))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Rows whose verdict is exactly `carry` — the set that MUST appear.
fn carried_fields(surface_block: &serde_json::Value) -> Vec<String> {
    surface_block
        .as_object()
        .unwrap()
        .iter()
        .filter(|(name, _)| !name.starts_with('_'))
        .filter(|(_, row)| row.get("mcpAnalyzeReply").and_then(|v| v.as_str()) == Some("carry"))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The reply-root keys one lane declares: `key -> {presence, why}` for every top-level key that is
/// not a field of that lane's facade view. `_`-prefixed entries are the block's own prose.
fn declared_root_keys(lane: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut declared = registry()["_replyRootKeys"][lane]
        .as_object()
        .unwrap_or_else(|| {
            panic!(
                "surface-parity.json's `_replyRootKeys` declares no `{lane}` block — every shipped \
                 reply needs one, or its non-view keys ride with nothing saying they exist"
            )
        })
        .clone();
    declared.retain(|k, _| !k.starts_with('_'));
    declared
}

fn registry() -> serde_json::Value {
    serde_json::from_str(REGISTRY).expect("surface-parity.json parses")
}

fn tmp_tree(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-keyset-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("zzop.config.jsonc"),
        zzop_config::template::CONFIG_TEMPLATE_JSONC,
    )
    .unwrap();
    dir
}

fn top_level_keys(reply: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(reply).expect("a reply is a JSON object");
    v.as_object()
        .expect("top level is an object")
        .keys()
        .cloned()
        .collect()
}

fn assert_shape(reply: &str, block: &serde_json::Value, surface: &str) {
    let keys = top_level_keys(reply);
    let leaked: Vec<&String> = omitted_fields(block)
        .iter()
        .filter(|f| keys.contains(f))
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .leak()
        .iter()
        .collect();
    assert!(
        leaked.is_empty(),
        "{surface}: fields the registry marks `omit` are on the wire: {leaked:?}\n\
         Either the reply started forwarding them (fix the reply, or change the row and its note), or \
         a whole struct is being forwarded where a shaped projection was intended. Text-scanning \
         cannot see this — that is why this pin exists.\nreply keys: {keys:?}"
    );

    let missing: Vec<String> = carried_fields(block)
        .into_iter()
        .filter(|f| !keys.contains(f))
        .collect();
    assert!(
        missing.is_empty(),
        "{surface}: fields the registry marks `carry` are absent from a real reply: {missing:?}\n\
         The registry documents TODAY's truth — if the field is genuinely gone, move its row to `omit` \
         with a note saying where the data is now.\nreply keys: {keys:?}"
    );

    // EVERY key accounted for. A field row covers a key only when the key IS a view field; the rest
    // are declared in `_replyRootKeys`, and this is the assertion that makes the declaration cost
    // something. Without it a reply can grow a channel that no document mentions — which is exactly
    // how the join reply went a day with no native-analysis roster while the registry looked green.
    let rows: Vec<String> = block
        .as_object()
        .expect("a surface block is an object of field -> row")
        .keys()
        .filter(|k| !k.starts_with('_'))
        .cloned()
        .collect();
    let declared = declared_root_keys(surface);
    let undeclared: Vec<&String> = keys
        .iter()
        .filter(|k| !rows.contains(k) && !declared.contains_key(*k))
        .collect();
    assert!(
        undeclared.is_empty(),
        "{surface}: top-level reply keys that are neither a registry field row nor declared in \
         `_replyRootKeys.{surface}`: {undeclared:?}\n\
         A key that is not a field of this lane's facade view CANNOT get a row (TEST 1/2 pin each \
         root's key set to the view), so declare it in `_replyRootKeys` with its `presence` and a \
         `why` — that block is the only place such a key can be said to exist."
    );
    let missing_declared: Vec<&String> = declared
        .iter()
        .filter(|(_, spec)| spec["presence"] == "always")
        .map(|(k, _)| k)
        .filter(|k| !keys.contains(k))
        .collect();
    assert!(
        missing_declared.is_empty(),
        "{surface}: keys declared `always` in `_replyRootKeys.{surface}` are absent from a real \
         reply: {missing_declared:?}\n\
         Either the reply stopped writing them (fix the reply) or they became conditional — in which \
         case say `presence: conditional` and let the `why` name the condition, because `always` is \
         the only half of this block a keyset can check.\nreply keys: {keys:?}"
    );
}

/// `analyze_repo` — the single-tree reply.
#[test]
fn the_analyze_reply_carries_and_omits_exactly_what_the_registry_says() {
    let dir = tmp_tree("analyze");
    fs::write(
        dir.join("api.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    let out =
        zzop_summary::analyze_summary(Some(&dir.display().to_string()), None, &default_filters())
            .expect("analyze must succeed on a configured tree");
    let reg = registry();
    assert_shape(&out, &reg["analyzeOutputView"], "analyze_repo");
}

/// `cross_repo` — the multi-tree join reply, whose registry block is the other half.
#[test]
fn the_cross_reply_carries_and_omits_exactly_what_the_registry_says() {
    let fe = tmp_tree("cross-fe");
    fs::write(
        fe.join("api.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    let be = tmp_tree("cross-be");
    fs::write(be.join("b.ts"), "export const b = 2;\n").unwrap();
    let paths = vec![fe.display().to_string(), be.display().to_string()];
    let out = zzop_summary::cross_summary(&paths, None, &default_filters())
        .expect("cross must succeed on two configured trees");
    let reg = registry();
    assert_shape(&out, &reg["multiAnalyzeOutputView"], "cross_repo");
}

/// The join reply must answer "was this replayed or recomputed?" PER TREE, and must say what that
/// means exactly ONCE.
///
/// 🔴 It answered neither until 2026-09-09: `hitFiles` appeared in no byte of a cross reply, while the
/// single-tree lane had published cache provenance since the day that silence was named (review ledger
/// V146). A reader of a join could not tell a recomputed finding list from a replayed one.
///
/// ⚠ The second half of this test is the one that will actually catch a regression. Reusing
/// `shape_cache_signal` here instead of `shape_cache_numbers` would look correct, pass every other
/// test, and quietly put a 1,045-byte run-invariant string in every `sources[]` row — 26 KB on a
/// 25-tree join, in a reply whose size is already an open question. `1` is the assertion; `N` is the
/// bug that reads like a fix.
#[test]
fn cache_provenance_rides_per_tree_and_its_meaning_ships_once() {
    let fe = tmp_tree("cache-fe");
    fs::write(fe.join("a.ts"), "export const a = 1;\n").unwrap();
    let be = tmp_tree("cache-be");
    fs::write(be.join("b.ts"), "export const b = 2;\n").unwrap();
    let paths = vec![fe.display().to_string(), be.display().to_string()];
    let doc = zzop_summary::cross_summary(&paths, None, &default_filters())
        .expect("cross must succeed on two configured trees");
    let out: serde_json::Value = serde_json::from_str(&doc).expect("the reply is a JSON document");

    let sources = out["sources"].as_array().expect("sources[]");
    assert_eq!(sources.len(), 2, "two trees in, two rows out");
    for row in sources {
        let cache = &row["cache"];
        assert!(
            cache.get("hitFiles").is_some() && cache.get("missFiles").is_some(),
            "a sources[] row carries no cache provenance: {row}"
        );
        assert_eq!(
            cache["fileCount"], row["fileCount"],
            "the ratio must be readable without joining two keys, so the denominator rides inside"
        );
        assert!(
            cache.get("meaning").is_none(),
            "the run-invariant sentence belongs at the root of this lane, not in every row: {cache}"
        );
    }

    let meaning = out["cacheMeaning"]
        .as_str()
        .expect("cacheMeaning at the root");
    assert!(
        meaning.contains("REPLAYED"),
        "the legend must say what a hit does to findings"
    );
    assert_eq!(
        doc.matches("were served whole from the cache").count(),
        1,
        "the cache legend appears more than once -- N trees must not cost N copies of it"
    );
}

/// `module_map` — the orientation reply, declared in the same commit that put it on the wire.
///
/// That ordering is the point, and it is what the two tests below this one were written after the
/// fact to recover: `check_coverage` shipped for weeks with no declaration, and the coverage reply's
/// own block says how that was found — a key was added and NOTHING went red. A new surface gets its
/// row set on the way in, not after someone notices.
///
/// Like the coverage lane, this one has no facade-view block, so the `block` passed to
/// [`assert_shape`] is empty on purpose: no key can be covered by a field row, and every one of them
/// has to be declared in `_replyRootKeys.module_map` or this fails naming it.
#[test]
fn the_module_map_reply_declares_every_key_it_ships() {
    let dir = tmp_tree("module-map");
    fs::create_dir_all(dir.join("lib")).unwrap();
    // TWO modules with an import ACROSS them, so `edges` has a row and the floor below is about a map
    // rather than about an empty one. A one-directory fixture folds to a single box with no edges,
    // which would satisfy "every key is declared" while proving nothing about the rows.
    fs::write(
        dir.join("lib/util.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    fs::write(
        dir.join("api.ts"),
        "import { load } from './lib/util';\nexport const go = () => load();\n",
    )
    .unwrap();
    let out = zzop_summary::module_map(&[dir.display().to_string()], None, 1)
        .expect("the module map must succeed on a configured tree");

    // FLOOR: a real map, not an empty envelope. Both halves, because either one alone is satisfiable
    // by a reply that answered nothing.
    let v: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    assert!(
        v["modules"].as_array().is_some_and(|m| m.len() >= 2)
            && v["edges"].as_array().is_some_and(|e| !e.is_empty()),
        "the fixture produced no multi-module map with an edge, so the declaration check below would \
         be about an empty reply: {out}"
    );

    assert_shape(&out, &serde_json::json!({}), "module_map");
}

/// `check_coverage` — the aggregate-visibility reply, which had NO declaration at all until
/// 2026-09-14 (review ledger W2).
///
/// # Why this test is later than the other two, and what that cost
/// The registry declared `analyze_repo` and `cross_repo`. Every top-level key of the coverage reply
/// was undeclared, so a key added to or removed from this surface went past every guard in the repo —
/// the same blind spot `_replyRootKeys`' own `_doc` describes for nested keys, one whole surface over.
/// It was found the way such things are: a key was added here and NOTHING went red.
///
/// This lane has no facade-view block (there is no `queryCoverageView` row set), so the `block`
/// passed to [`assert_shape`] is empty on purpose — no key can be covered by a field row, and every
/// one of them has to be declared in `_replyRootKeys.check_coverage` or this fails naming it.
#[test]
fn the_coverage_reply_declares_every_key_it_ships() {
    let dir = tmp_tree("coverage");
    fs::write(
        dir.join("api.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    let out = zzop_summary::coverage_summary(&[dir.display().to_string()], None)
        .expect("coverage must succeed on a configured tree");

    // FLOOR: the reply has to be a real one. An empty object would satisfy "every key is declared"
    // while saying nothing, and the `always` half below would then be the only thing working.
    let keys = top_level_keys(&out);
    assert!(
        keys.len() >= 10,
        "the coverage reply carries {} top-level keys — that is not a real reply, and the declaration \
         check below would be vacuous: {keys:?}",
        keys.len()
    );

    assert_shape(&out, &serde_json::json!({}), "check_coverage");
}
