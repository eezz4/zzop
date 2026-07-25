//! Unit coverage for the manifest projection and the diff's two honesty gates. Both functions the
//! `zzop manifest`/`zzop diff` subcommands call are pure (JSON in, JSON out), so every contract below
//! is pinned against literal engine output with no filesystem and no analysis run; the end-to-end
//! "the real binary produces one from a real tree" lane lives in `packages/cli-bin/tests/cli.rs`.

use serde_json::json;

use super::diff_manifests_json;

/// One fake tree entry. The engine's IR block is ATTACHED by name rather than spelled as a quoted
/// `json!` key, deliberately: `crates/engine/tests/rule_contracts/surface_parity.rs` reads a quoted
/// `ir` key literal anywhere under `crates/summary/src` as proof that the shaped MCP reply forwards
/// that field, which the registry marks `omit`. The proxy is right about replies and wrong about a
/// fixture — this mocks engine INPUT, it emits no reply — so spelling the key here would fail a true
/// contract with a false example (this very sentence tripped it once). Both stay true this way.
fn tree(source_id: &str, degraded: u64, provides: serde_json::Value) -> serde_json::Value {
    let mut entry = json!({
        "sourceId": source_id,
        "output": { "coverage": { "joinContributionZero": false, "degraded": degraded } },
    });
    entry["output"]["ir"] = json!({ "io": { "provides": provides, "consumes": [] } });
    entry
}

/// A minimal `analyzeTrees` output: `fe` calls two routes, `be` provides one of them (the other is
/// unprovided), plus a third provide nobody consumes.
fn engine_output() -> serde_json::Value {
    json!({
        "trees": [
            tree("fe", 0, json!([])),
            tree("be", 0, json!([
                { "kind": "http", "key": "GET /api/users", "file": "src/a.ts", "line": 3 },
                { "kind": "http", "key": "GET /api/legacy", "file": "src/a.ts", "line": 9 },
                // Same identity, different site — the manifest must dedupe it away, which is
                // what makes a pure refactor produce an EMPTY diff.
                { "kind": "http", "key": "GET /api/users", "file": "src/b.ts", "line": 1 },
            ])),
        ],
        "crossLayer": {
            "edges": [{
                "kind": "http", "key": "GET /api/users",
                "from": { "source": "fe", "file": "src/x.ts", "line": 2 },
                "to": { "source": "be", "file": "src/a.ts", "line": 3 },
            }],
            "unconsumedProvides": [
                { "source": "be", "kind": "http", "key": "GET /api/legacy", "file": "src/a.ts", "line": 9 },
            ],
            "unprovidedConsumes": [
                { "source": "fe", "kind": "http", "key": "GET /api/gone", "file": "src/x.ts", "line": 7 },
            ],
            "unresolvedConsumes": [
                { "source": "fe", "kind": "http", "key": null, "raw": "url(x)", "file": "src/x.ts", "line": 8 },
            ],
            "externalConsumes": [],
            "ambiguousConsumes": [],
        },
    })
}

fn manifest() -> serde_json::Value {
    serde_json::from_str(&super::project(&engine_output())).expect("manifest is valid JSON")
}

/// Seals the whole identity contract in one place: what a manifest carries (kind/key/source only,
/// deduped, sorted) and what it must NEVER carry (file/line — the diff noise a single refactor
/// generates — or an absolute `root`, which differs between a laptop and CI and would make the two
/// machines that most need to compare unable to).
#[test]
fn the_manifest_carries_deduped_identity_and_never_a_path_or_a_line() {
    let m = manifest();
    assert_eq!(
        m["provides"],
        json!([
            { "kind": "http", "key": "GET /api/legacy", "source": "be" },
            { "kind": "http", "key": "GET /api/users", "source": "be" },
        ]),
        "sorted, and the twice-declared route collapses to one identity"
    );
    assert_eq!(
        m["edges"],
        json!([{ "kind": "http", "key": "GET /api/users", "from": "fe", "to": "be" }])
    );
    assert_eq!(
        m["buckets"],
        json!([
            { "bucket": "unconsumedProvides", "kind": "http", "key": "GET /api/legacy", "source": "be" },
            { "bucket": "unprovidedConsumes", "kind": "http", "key": "GET /api/gone", "source": "fe" },
            // An unresolved consume has no key — its `raw` source text IS its identity (same
            // fallback `output::bucket_keys` uses), never a guess and never dropped silently.
            { "bucket": "unresolvedConsumes", "kind": "http", "key": "url(x)", "source": "fe" },
        ]),
        "every non-edge bucket membership, sorted by the row's own serialized text (bucket first) — \
         NOT by engine bucket order, so a bucket reorder in the engine cannot churn a manifest"
    );
    let text = super::project(&engine_output());
    for banned in ["src/a.ts", "src/x.ts", "\"line\"", "\"root\"", "\"file\""] {
        assert!(
            !text.contains(banned),
            "manifest must not carry {banned}: {text}"
        );
    }
}

/// The T1-tier pin behind `super::RELATIONS` (see that const's own doc): the producer writes literal
/// keys and the reader iterates the const, so this equality is what keeps them one vocabulary — a
/// relation added to the manifest and not to the const (or the reverse) fails here rather than
/// shipping as a relation nobody diffs.
#[test]
fn the_manifests_top_level_keys_are_exactly_the_shared_relation_vocabulary() {
    let m = manifest();
    let keys: Vec<&str> = m
        .as_object()
        .expect("root object")
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected: Vec<&str> = super::RELATIONS.to_vec();
    expected.extend(["sources", "tool"]);
    expected.sort_unstable();
    assert_eq!(keys, expected);
}

#[test]
fn the_same_analysis_projects_byte_identically() {
    // Determinism is the product's named property; a manifest that reordered between runs would
    // make every diff a lie. Tree order must not leak either.
    let mut reversed = engine_output();
    let trees = reversed["trees"].as_array().unwrap().clone();
    reversed["trees"] = json!([trees[1], trees[0]]);
    assert_eq!(super::project(&engine_output()), super::project(&reversed));
}

/// A source row's ORDER must follow its identity, never its coverage. Caught live: sorting source
/// rows by serialized text put `degraded`/`joinContributionZero` (which serialize alphabetically
/// ahead of `sourceId`) in charge, so one tree's coverage moving re-ordered the whole array — pure
/// churn in the git diff of the very file this feature asks users to commit.
#[test]
fn source_rows_are_ordered_by_identity_not_by_their_coverage_numbers() {
    let mut degraded = engine_output();
    degraded["trees"][1]["output"]["coverage"]["degraded"] = json!(9);
    degraded["trees"][1]["output"]["coverage"]["joinContributionZero"] = json!(true);
    let ids = |v: &serde_json::Value| -> Vec<String> {
        serde_json::from_str::<serde_json::Value>(&super::project(v)).unwrap()["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["sourceId"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(ids(&engine_output()), ["be", "fe"]);
    assert_eq!(ids(&degraded), ["be", "fe"], "coverage must not re-order");
}

/// Honesty gate 1, forward direction: same build diffs fine and a pure refactor is EMPTY.
#[test]
fn an_identical_pair_diffs_to_nothing() {
    let m = super::project(&engine_output());
    let d: serde_json::Value = serde_json::from_str(&diff_manifests_json(&m, &m, false).unwrap())
        .expect("diff is valid JSON");
    assert_eq!(d["transitions"], json!([]));
    for relation in ["provides", "edges", "buckets"] {
        assert_eq!(d[relation]["added"], json!([]), "{relation}");
        assert_eq!(d[relation]["removed"], json!([]), "{relation}");
    }
    assert!(
        d.get("toolDrift").is_none(),
        "same build, nothing to disclose"
    );
}

/// Honesty gate 1, REVERSE direction (the one that matters): two builds must not silently compare.
#[test]
fn a_cross_build_diff_is_refused_and_only_discloses_when_forced() {
    let a = super::project(&engine_output());
    // Same structural content, different producing build — the ONLY difference, so anything the diff
    // reports below is attributable to the tool rather than to the analyzed code.
    let mut other = manifest();
    other["tool"] = json!("zzop/0.0.1-other zzop-parser-typescript=deadbeef");
    let b = other.to_string();
    let err = diff_manifests_json(&a, &b, false)
        .expect_err("different zzop builds must not diff silently");
    assert!(err.contains("different zzop builds"), "{err}");
    assert!(
        err.contains("--allow-tool-drift"),
        "the escape hatch is named: {err}"
    );
    let forced: serde_json::Value =
        serde_json::from_str(&diff_manifests_json(&a, &b, true).unwrap()).unwrap();
    assert!(
        forced["toolDrift"]["warning"]
            .as_str()
            .unwrap()
            .contains("unattributable"),
        "forcing it must DISCLOSE, not go quiet: {forced}"
    );
}

/// Honesty gate 2: a removal from a tree that got less visible is a blindness suspect, not a
/// deletion. Reverse-verified in the same test — the identical removal with coverage HELD is not
/// tagged, so the flag proves the coverage fact rather than firing on every removal.
#[test]
fn a_removal_under_a_coverage_drop_is_tagged_a_blindness_suspect() {
    let a = super::project(&engine_output());
    let mut degraded = engine_output();
    degraded["trees"][1]["output"]["coverage"]["degraded"] = json!(4);
    degraded["trees"][1]["output"]["ir"]["io"]["provides"] = json!([]);
    degraded["crossLayer"]["edges"] = json!([]);
    degraded["crossLayer"]["unconsumedProvides"] = json!([]);
    degraded["crossLayer"]["unprovidedConsumes"] = json!([
        { "source": "fe", "kind": "http", "key": "GET /api/gone", "file": "src/x.ts", "line": 7 },
        { "source": "fe", "kind": "http", "key": "GET /api/users", "file": "src/x.ts", "line": 2 },
    ]);
    let b = super::project(&degraded);
    let d: serde_json::Value =
        serde_json::from_str(&diff_manifests_json(&a, &b, false).unwrap()).unwrap();

    assert_eq!(
        d["sources"]["coverageDropped"],
        json!([{
            "sourceId": "be",
            "joinContributionZero": { "a": false, "b": false },
            "degraded": { "a": 0, "b": 4 },
        }])
    );
    let removed = d["provides"]["removed"].as_array().unwrap();
    assert!(
        removed.iter().all(|r| r["blindnessSuspect"] == json!(true)),
        "be's provides vanished WHILE be lost coverage — never reported as plain deletions: {removed:?}"
    );

    // The 1st-rank signal, and gate 2 riding it: the route moved edges -> unprovidedConsumes.
    assert_eq!(
        d["transitions"],
        json!([{
            "kind": "http", "key": "GET /api/users",
            "from": ["edges"], "to": ["unprovidedConsumes"],
            "blindnessSuspect": true,
        }])
    );

    // Reverse: same structural change, coverage HELD -> the flag must be absent everywhere.
    let mut held = degraded.clone();
    held["trees"][1]["output"]["coverage"]["degraded"] = json!(0);
    let held = super::project(&held);
    let d2: serde_json::Value =
        serde_json::from_str(&diff_manifests_json(&a, &held, false).unwrap()).unwrap();
    assert_eq!(d2["sources"]["coverageDropped"], json!([]));
    assert!(
        !diff_manifests_json(&a, &held, false)
            .unwrap()
            .contains("blindnessSuspect"),
        "the tag must track the coverage fact, not merely the removal: {d2}"
    );
}

/// A source that VANISHED between runs explains its own rows disappearing — the same gate-2 reading
/// as a coverage drop, and the reason `sources.removed` is reported at all.
#[test]
fn a_vanished_source_makes_its_removals_suspects_too() {
    let a = super::project(&engine_output());
    let mut dropped = engine_output();
    dropped["trees"] = json!([dropped["trees"][0]]);
    dropped["crossLayer"]["edges"] = json!([]);
    dropped["crossLayer"]["unconsumedProvides"] = json!([]);
    let b = super::project(&dropped);
    let d: serde_json::Value =
        serde_json::from_str(&diff_manifests_json(&a, &b, false).unwrap()).unwrap();
    assert_eq!(d["sources"]["removed"], json!(["be"]));
    assert_eq!(d["edges"]["removed"][0]["blindnessSuspect"], json!(true));
}

/// The argument-mix-up lane: an `analyze`/`cross` reply passed to `diff` must be a NAMED error, never
/// two empty relation sets read as "nothing changed" (a §0 silent-wrong shape).
#[test]
fn a_non_manifest_argument_is_a_named_error_naming_which_side() {
    let m = super::project(&engine_output());
    let err = diff_manifests_json("{\"findings\":{}}", &m, false).expect_err("not a manifest");
    assert!(err.contains("the first manifest"), "{err}");
    let err = diff_manifests_json(&m, "{\"tool\":\"x\"}", false).expect_err("not a manifest");
    assert!(err.contains("the second manifest"), "{err}");
    assert!(err.contains("sources"), "names the missing relation: {err}");
}
