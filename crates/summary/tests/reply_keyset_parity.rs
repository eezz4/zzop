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
