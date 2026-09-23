//! The join reply's native-analysis roster, pinned against a REAL `cross_summary` reply.
//!
//! # Why this file exists at all
//! `9272ee0` closed a blank on the per-tree `analyze` reply — a `cross-layer/*` analysis with nowhere
//! to report was byte-identical to one that ran clean — and left the identical blank open one lane
//! over. Measured on the nine-repo corpus before this pin: the `cross` reply carried `nativeAnalyses`
//! NOWHERE (not at the root, not on any of nine `sources[]` rows) while its own
//! `crossLayerFindings.byRule` held 18 keys against a 27-analysis roster, so nine analyses were silent
//! with no population standing beside them.
//!
//! # Why the assertions are DERIVATIONS, never literals
//! Not one number below is written down. `registered`, the population and the zero list all move when
//! a rule is added, and a test that pinned today's counts would fail on the next rule instead of on
//! the next defect. What is pinned is the SHAPE the roster must keep to be worth reading — a non-empty
//! proper subset, a zero list that is exactly the subtraction it claims to be, and no fired id outside
//! the population — plus a two-directional canary that proves the derivation reacts to the gate at all.
//!
//! # The floor, and why the canary is here rather than in the module
//! A collapsed derivation publishes an EMPTY population, which reads as "no analysis reports into
//! `crossLayerFindings`" — a confident falsehood that is byte-identical to a healthy build with no
//! cross-layer rules. `crates/summary/src/cross/native_analyses.rs` asserts that in `debug_assert`s,
//! which a release reply path cannot rely on, so the load-bearing check runs here: the same split
//! `zzop_engine::NativeAnalyses` makes with `rule_contracts::native_analyses`.

use std::fs;

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

/// A configured tree. `rules_override` replaces the template's own `"rules"` object WHOLE, so the
/// gate the canary exercises is spelled the way a user spells it and cannot drift from the template.
fn tmp_tree(name: &str, rules_override: Option<&str>) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-na-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut config = zzop_config::template::CONFIG_TEMPLATE_JSONC.to_string();
    if let Some(rules) = rules_override {
        // Replace the template's `"rules"` OBJECT, not a fixed spelling of it. It carried `{}` until
        // 2026-09-02, when three hygiene analyses became default-off there; a substitution pinned to
        // the empty spelling would have stopped matching and left this canary disabling nothing,
        // which is the failure the assertions below exist to catch. Both ends are asserted so a
        // reshaped template fails loudly here rather than quietly passing.
        let start = config
            .find("\"rules\": {")
            .expect("the config template no longer carries a `\"rules\": {` object to substitute");
        let end = config[start..]
            .find("},")
            .map(|i| start + i + 2)
            .expect("the template's `\"rules\"` object has no `},` terminator — reshaped how?");
        assert!(
            end - start < 400,
            "the `\"rules\"` object spans {} bytes, which is not an object this substitution should \
             be swallowing whole — the terminator search almost certainly ran past it",
            end - start
        );
        config.replace_range(start..end, rules);
    }
    fs::write(dir.join("zzop.config.jsonc"), config).unwrap();
    dir
}

/// Two trees whose join has both sides: a consume with no provider, and a provide nobody calls.
fn two_trees(tag: &str, rules_override: Option<&str>) -> Vec<String> {
    let fe = tmp_tree(&format!("{tag}-fe"), rules_override);
    fs::write(
        fe.join("api.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    let be = tmp_tree(&format!("{tag}-be"), None);
    fs::write(
        be.join("server.ts"),
        "import express from 'express';\nconst app = express();\napp.get('/api/orders', (_req, res) => res.json([]));\n",
    )
    .unwrap();
    vec![fe.display().to_string(), be.display().to_string()]
}

fn cross(paths: &[String]) -> serde_json::Value {
    let out = zzop_summary::cross_summary(paths, None, &default_filters())
        .expect("cross must succeed on two configured trees");
    serde_json::from_str(&out).expect("a reply is a JSON object")
}

fn ids(v: &serde_json::Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .unwrap_or_else(|| {
            panic!(
                "`{key}` must be an ARRAY even when empty — a dropped empty list is \
             the exact shape that made 'not analyzed' and 'analyzed and clean' the same bytes: {v}"
            )
        })
        .iter()
        .map(|i| i.as_str().expect("a roster entry is a rule id").to_string())
        .collect()
}

/// The whole contract of the roster, in one real reply: present, complete, self-consistent with the
/// findings map beside it, and legended.
#[test]
fn the_join_reply_carries_a_roster_that_agrees_with_its_own_findings_map() {
    let v = cross(&two_trees("plain", None));
    let roster = v
        .get("nativeAnalyses")
        .unwrap_or_else(|| panic!("the join reply dropped nativeAnalyses: {v}"));

    let registered = roster["registered"]
        .as_u64()
        .unwrap_or_else(|| panic!("the denominator must be a number: {roster}"));
    let population = ids(roster, "reportedInCrossLayerFindings");
    let zero = ids(roster, "zeroInCrossLayerFindings");
    ids(roster, "disabled");

    // THE FLOOR. An empty population would claim nothing reports into `crossLayerFindings` while that
    // very map sits beside it; a population equal to the registry would mean the cross-layer
    // registration and the whole native registry were read as one thing.
    assert!(
        !population.is_empty(),
        "the cross-layer population derived to EMPTY — the reply then says no analysis reports into \
         its own findings channel: {roster}"
    );
    assert!(
        (population.len() as u64) < registered,
        "the population ({}) must be a PROPER subset of the {registered} registered analyses: {roster}",
        population.len()
    );

    // The zero list must be exactly the subtraction it advertises. Derived from the reply's OWN
    // findings map, so this cannot pass by both sides being wrong in the same direction.
    let fired: Vec<String> = v["crossLayerFindings"]["byRule"]
        .as_object()
        .expect("a shaped findings view carries byRule")
        .keys()
        .cloned()
        .collect();
    let outside: Vec<&String> = fired.iter().filter(|f| !population.contains(f)).collect();
    assert!(
        outside.is_empty(),
        "crossLayerFindings keys the roster does not contain {outside:?} — the roster and the findings \
         disagree about what ran, which is the confusion this object exists to end: {roster}"
    );
    let expected_zero: Vec<&String> = population.iter().filter(|p| !fired.contains(p)).collect();
    assert_eq!(
        zero.iter().collect::<Vec<&String>>(),
        expected_zero,
        "zeroInCrossLayerFindings must be `reportedInCrossLayerFindings` minus the keys of \
         crossLayerFindings.byRule, and nothing else: {roster}"
    );
    assert!(
        !zero.is_empty(),
        "the whole point is that a zero becomes a ROW: on a two-file fixture most cross-layer \
         analyses find nothing, so an empty list here means the subtraction never ran: {roster}"
    );
}

/// 🔴 The legend forks by LANE while the key name does not — the one thing a copied field would have
/// got wrong. In the per-tree reply `reportedInCrossLayerFindings` means "not evaluable here, run the
/// join"; carrying that sentence into the reply that IS the join tells the reader to run what they
/// just ran.
#[test]
fn the_join_legend_does_not_tell_the_reader_to_run_the_join() {
    let v = cross(&two_trees("legend", None));
    let meaning = v
        .get("nativeAnalysesMeaning")
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("the roster must not arrive without its legend: {v}"));

    let mut keys: Vec<&String> = meaning.keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "disabled",
            "everythingElse",
            "registered",
            "reportedInCrossLayerFindings",
            "zeroInCrossLayerFindings",
        ],
        "every key of the roster needs a sentence, and so does the residual class that has no key: {v}"
    );

    let reported = meaning["reportedInCrossLayerFindings"].as_str().unwrap();
    assert!(
        !reported.contains("run the CROSS-LAYER JOIN"),
        "the per-tree lane's imperative must not travel into the join reply: {reported}"
    );
    assert!(
        reported.contains("THIS reply carries"),
        "the join legend has to say that these verdicts are IN this reply, which is the whole \
         inversion: {reported}"
    );

    // The residual class inverts too: on a per-tree reply everything else RAN here; on a join reply
    // everything else judges one tree and reports somewhere this reply does not carry.
    let residual = meaning["everythingElse"].as_str().unwrap();
    assert!(
        residual.contains("sources[].findingCount"),
        "the residual sentence must name the only trace of per-tree findings this reply has: {residual}"
    );

    // The two LANE-INVARIANT sentences are read from `zzop_facade`, not restated — pinned by identity
    // so a copy made here would be caught rather than left to drift.
    assert_eq!(
        meaning["registered"].as_str().unwrap(),
        zzop_facade::NATIVE_ANALYSES_REGISTERED_MEANING,
        "the two replies must not disagree about their own denominator"
    );
    assert!(
        meaning["disabled"]
            .as_str()
            .unwrap()
            .starts_with(zzop_facade::NATIVE_ANALYSES_DISABLED_MEANING),
        "the join legend composes the shared sentence and appends its own clause; it never rewrites it"
    );
}

/// 🔴 A zero row that is NOT a measured zero, and the legend that has to say so.
///
/// `cross-layer/all-consumes-unjoined` fires once per tree whose internal http consumes all failed to
/// join, and the orchestrator then DROPS the per-call `cross-layer/ambiguous-consume` and
/// `cross-layer/unprovided-mutation-call` findings for that tree. Both ids therefore key nothing in
/// `crossLayerFindings.byRule` and land in `zeroInCrossLayerFindings` — where, until 2026-08-31, the
/// legend called them MEASURED zeros and pointed the reader at `buckets` to check, which showed a
/// populated input channel and completed the wrong conclusion. Measured on the nine-repo dogfood join:
/// 226 findings with the aggregate on, 314 with it off, the difference being 76 + 17 of exactly these
/// two ids.
///
/// The fixture builds the shape rather than asserting the counts: a front end with enough unjoinable
/// calls to trip the aggregate's floor, against a back end that serves routes so the run has something
/// to join against at all. What is pinned is that the roster's legend NAMES the replaced ids — not that
/// this particular corpus produces this particular number.
#[test]
fn a_replaced_analysis_is_not_reported_as_a_measured_zero() {
    let fe = tmp_tree("subsumed-fe", None);
    // Distinct paths, and more of them than `MIN_UNJOINED_CONSUMES`: the aggregate folds a TREE, and a
    // tree with two unjoined calls is one the per-call findings still read better for.
    //
    // The two sides share no path vocabulary at all, and that is load-bearing: a near-miss, a prefix
    // drift or a method mismatch would DIAGNOSE these call sites, and `all_consumes_unjoined`'s
    // `diagnosed` precondition then excludes them from its floor — a tree whose calls each break in
    // their own way is not a tree with one unresolved base.
    fs::write(
        fe.join("api.ts"),
        "export const a = () => fetch('/alpha');\n\
         export const b = () => fetch('/beta');\n\
         export const c = () => fetch('/gamma');\n\
         export const d = () => fetch('/delta');\n",
    )
    .unwrap();
    let be = tmp_tree("subsumed-be", None);
    fs::write(
        be.join("server.ts"),
        "import express from 'express';\nconst app = express();\n\
         app.get('/inventory/restock', (_req, res) => res.json([]));\n\
         app.get('/inventory/audit', (_req, res) => res.json([]));\n",
    )
    .unwrap();
    let v = cross(&[fe.display().to_string(), be.display().to_string()]);

    let fired: Vec<String> = v["crossLayerFindings"]["byRule"]
        .as_object()
        .expect("a shaped findings view carries byRule")
        .keys()
        .cloned()
        .collect();
    // The premise. Without it the assertion below would pass for the wrong reason — the ids would be
    // absent from the zero list rather than correctly labelled in it.
    if !fired
        .iter()
        .any(|f| f == "cross-layer/all-consumes-unjoined")
    {
        panic!(
            "the fixture must trip the subsuming aggregate, or this test proves nothing about the \
             case it exists for. Fired: {fired:?}"
        );
    }
    let zero = ids(&v["nativeAnalyses"], "zeroInCrossLayerFindings");
    let legend = v["nativeAnalysesMeaning"]["zeroInCrossLayerFindings"]
        .as_str()
        .expect("the zero row must carry its legend");

    // Every id the fired aggregate DECLARED it replaces, read from the finding itself rather than from
    // a list this test keeps: the declaration is the channel, and a test with its own copy of the list
    // would go green against a rule that stopped declaring anything.
    let declared: Vec<String> = v["crossLayerFindings"]["shown"]
        .as_array()
        .expect("the shaped view carries the findings it shows")
        .iter()
        .filter_map(|f| f["data"]["replaces"].as_array())
        .flatten()
        .filter_map(|r| r.as_str())
        .map(str::to_string)
        .collect();
    assert!(
        !declared.is_empty(),
        "the aggregate fired but declared no `data.replaces` — the roster then has no way to tell a \
         replaced id from a measured zero, which is the whole defect: {}",
        v["crossLayerFindings"]["shown"]
    );
    for id in declared.iter().filter(|d| zero.contains(d)) {
        assert!(
            legend.contains(id.as_str()),
            "`{id}` is in zeroInCrossLayerFindings AND was declared replaced, and the legend does not \
             name it — so the reply calls a dropped finding set a zero: {legend}"
        );
    }
    assert!(
        !legend.contains("These are MEASURED zeros"),
        "the unconditional claim is the defect itself; a zero row has three causes and only one of \
         them is clean: {legend}"
    );
}

/// The other direction: with nothing replaced, the legend must SAY nothing was, rather than leaving the
/// reader to wonder whether the run was checked. A caveat that only ever appears is unfalsifiable.
#[test]
fn a_run_with_no_replacement_says_so_rather_than_staying_silent() {
    let v = cross(&two_trees("no-replacement", None));
    let legend = v["nativeAnalysesMeaning"]["zeroInCrossLayerFindings"]
        .as_str()
        .expect("the zero row must carry its legend");
    assert!(
        legend.contains("Nothing in this run declared a replacement"),
        "a measurement has to report its negative result too: {legend}"
    );
}

/// 🔴 CANARY, both directions: an id the config switches OFF in exactly ONE tree must leave the
/// population and appear under `disabled`, and every other id must stay put.
///
/// It proves two things a structural assertion cannot. First, that the derivation reacts to the gate
/// at all — a roster hard-wired to the registration would pass every test above unchanged. Second,
/// that the union is EXCLUDE-ONLY across trees the way `merge_config::union_configs` gates the join:
/// only the frontend tree disables the rule here, and the join must still drop it, because a
/// cross-layer verdict spans trees and no single tree owns it.
#[test]
fn disabling_one_cross_layer_id_in_one_tree_moves_it_out_of_the_population() {
    const DISABLED_ID: &str = "cross-layer/unconsumed-endpoint";

    let base = cross(&two_trees("canary-base", None));
    let base_roster = &base["nativeAnalyses"];
    let base_population = ids(base_roster, "reportedInCrossLayerFindings");
    assert!(
        base_population.iter().any(|id| id == DISABLED_ID),
        "the canary's subject must be IN the population before it is switched off, or the test below \
         proves nothing: {base_roster}"
    );

    let gated = cross(&two_trees(
        "canary-off",
        Some(&format!("\"rules\": {{ \"{DISABLED_ID}\": \"off\" }},")),
    ));
    let gated_roster = &gated["nativeAnalyses"];
    let gated_population = ids(gated_roster, "reportedInCrossLayerFindings");
    let gated_disabled = ids(gated_roster, "disabled");

    assert!(
        !gated_population.iter().any(|id| id == DISABLED_ID),
        "a disabled analysis must not stay in a list whose implicit promise is 'these report here' — \
         that promise is false for one the config switched off: {gated_roster}"
    );
    assert!(
        gated_disabled.iter().any(|id| id == DISABLED_ID),
        "and it must not vanish either: `disabled` is the honest home, and the two lists are disjoint \
         by construction: {gated_roster}"
    );
    assert_eq!(
        gated_population.len() + 1,
        base_population.len(),
        "exactly ONE id may move — a gate that takes more is not the gate the config asked for.\n\
         before: {base_population:?}\nafter:  {gated_population:?}"
    );
    assert_eq!(
        base_roster["registered"], gated_roster["registered"],
        "the denominator is a BUILD fact and must not move with a config"
    );
}
