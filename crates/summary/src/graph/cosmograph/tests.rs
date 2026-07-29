use super::*;
use serde_json::Value;

/// The same shape `dep/tests.rs` uses — a 3-file cycle plus an unrelated leaf — so a reader comparing
/// the two formats is comparing them over identical input rather than two fixtures that might differ.
fn one_tree() -> Value {
    serde_json::json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "ir": { "dep": {
                    "src/a.ts": ["src/b.ts"],
                    "src/b.ts": ["src/c.ts"],
                    "src/c.ts": ["src/a.ts"],
                    "src/leaf.ts": []
                }},
                "findings": [{
                    "ruleId": "circular", "severity": "warning", "file": "src/a.ts", "line": 1,
                    "message": "m", "data": { "members": ["src/a.ts", "src/b.ts", "src/c.ts"] }
                }]
            }
        }]
    })
}

/// The same tree, plus the `ir.loc` map a real run carries. `src/b.ts` is deliberately ABSENT from it:
/// the omission case is the one that has to be provable, and a fixture where every file is measured
/// cannot prove it.
fn one_tree_with_loc() -> Value {
    let mut v = one_tree();
    v["trees"][0]["output"]["ir"]["loc"] = serde_json::json!({
        "src/a.ts": 42,
        "src/c.ts": 3,
        "src/leaf.ts": 7
    });
    v
}

/// The same tree again, now shaped like a run where git DID collect: a non-null `output.gitWindow` (the
/// engine's own "an analysis phase ran" signal — `AnalyzeOutputView::git_window`'s doc) plus the
/// per-file `output.nodes[]` history. `src/c.ts` is deliberately missing from `nodes[]`, because a file
/// git has no row for must not be handed someone else's zero.
fn one_tree_with_git() -> Value {
    let mut v = one_tree_with_loc();
    v["trees"][0]["output"]["gitWindow"] = serde_json::json!({ "recentDays": 30, "since": null });
    v["trees"][0]["output"]["nodes"] = serde_json::json!([
        { "path": "src/a.ts", "changeCount": 9, "churn": 120, "authorCount": 3,
          "lastModified": "2026-07-01T00:00:00Z" },
        { "path": "src/b.ts", "changeCount": 0, "churn": 0, "authorCount": 0, "lastModified": null },
        { "path": "src/leaf.ts", "changeCount": 2, "churn": 8, "authorCount": 1,
          "lastModified": "2025-01-02T00:00:00Z" }
    ]);
    v
}

/// The history axes a git-collecting run supplies, read off `one_tree_with_git`'s own `nodes[]` entry:
/// every field of it except `path`, which is the join key rather than an axis.
///
/// Derived from the FIXTURE INPUT, deliberately, and not from the emitter's output. The two omission
/// tests below assert that these names are ABSENT from output built without git — and if the name list
/// came from diffing the emitter's own git-run output against its no-git output, that assertion would
/// be true by construction and prove nothing. Reading the input shape keeps the two sides independent,
/// and means a fifth history field added to `nodes[]` is covered by both tests the moment it lands.
fn git_axes() -> Vec<String> {
    let tree = one_tree_with_git();
    tree["trees"][0]["output"]["nodes"][0]
        .as_object()
        .expect("the git fixture's nodes[] entries are objects")
        .keys()
        .filter(|k| k.as_str() != "path")
        .cloned()
        .collect()
}

fn rows(ndjson: &str) -> Vec<Value> {
    ndjson
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("every emitted line is valid JSON"))
        .collect()
}

#[test]
fn every_line_is_a_standalone_json_object() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, _) = nodes_ndjson(&u, None);
    let (links, _) = links_ndjson(&u, None);
    // The parse in `rows` IS the assertion — NDJSON's whole contract is that a consumer can read one
    // line at a time without a streaming parser.
    assert_eq!(rows(&nodes).len(), 4);
    assert_eq!(rows(&links).len(), 3);
}

/// The property that separates this lane from the mermaid one. `dep`'s cap would drop nodes here; this
/// lane must not, and the census must say so rather than leaving the reader to assume it.
#[test]
fn the_lane_is_uncapped_and_says_so() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, census) = nodes_ndjson(&u, None);
    assert_eq!(rows(&nodes).len(), u.nodes.len());
    assert_eq!(census.nodes_emitted, 4);
    assert_eq!(census.total_nodes, 4);
    assert!(census.render().contains("UNCAPPED"), "{}", census.render());
}

#[test]
fn a_node_carries_every_axis_a_viewer_can_style_by() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, _) = nodes_ndjson(&u, None);
    let a = rows(&nodes)
        .into_iter()
        .find(|r| r["id"] == "src/a.ts")
        .expect("src/a.ts is a node");
    assert_eq!(a["source"], "web");
    assert_eq!(a["path"], "src/a.ts");
    // A basename, not the path — this is the one zzop surface whose reader is a person looking at a
    // screen, and a full path on every node is unreadable at graph scale.
    assert_eq!(a["label"], "a.ts");
    assert_eq!(a["folder"], "src");
    assert_eq!(a["inCycle"], true);
    // a -> b, and c -> a.
    assert_eq!(a["fanIn"], 1);
    assert_eq!(a["fanOut"], 1);
    assert_eq!(a["degree"], 2);
}

/// SIZE is the axis a reader of a thousand-node graph reaches for first, and the run already measured
/// it (`ir.loc`). Without it every node is the same dot and the picture carries only topology.
#[test]
fn loc_rides_the_row_when_the_run_measured_it() {
    let u = super::super::dep::collect(&one_tree_with_loc());
    let (nodes, _) = nodes_ndjson(&u, None);
    let rows = rows(&nodes);
    let a = rows
        .iter()
        .find(|r| r["id"] == "src/a.ts")
        .expect("src/a.ts is a node");
    assert_eq!(a["loc"], 42);
    let leaf = rows
        .iter()
        .find(|r| r["id"] == "src/leaf.ts")
        .expect("leaf is a node");
    assert_eq!(leaf["loc"], 7);
}

/// The rule that keeps this table honest: an axis the run did not measure is ABSENT, never `0`. `0`
/// would say "this file has no lines" / "this file never changed" in the same bytes as "nobody looked",
/// and a viewer styling by it would draw the second as the first. Asserting `is_none()` on the KEY (not
/// `is_null()` on the value) is the point — it is what proves the field was omitted rather than filled.
#[test]
fn an_unmeasured_axis_is_absent_from_the_row_rather_than_zero() {
    let u = super::super::dep::collect(&one_tree_with_loc());
    let (nodes, _) = nodes_ndjson(&u, None);
    let rows = rows(&nodes);
    let b = rows
        .iter()
        .find(|r| r["id"] == "src/b.ts")
        .expect("src/b.ts is a node even though nothing measured its size");
    assert!(
        b.get("loc").is_none(),
        "an unmeasured file must carry NO loc key, not loc: 0 — got {b}"
    );
    // The measured siblings still ride, so this is an omission, not an emitter that gave up.
    let a = rows
        .iter()
        .find(|r| r["id"] == "src/a.ts")
        .expect("src/a.ts is a node");
    assert_eq!(a["loc"], 42);
}

/// The other half of the standard pair: size=LOC, colour=how often it changes. The run already computed
/// this per file when git collection was on, so a viewer should not have to run git itself.
#[test]
fn git_history_rides_the_row_when_the_run_collected_it() {
    let u = super::super::dep::collect(&one_tree_with_git());
    let (nodes, _) = nodes_ndjson(&u, None);
    let rows = rows(&nodes);
    let a = rows
        .iter()
        .find(|r| r["id"] == "src/a.ts")
        .expect("src/a.ts is a node");
    assert_eq!(a["changeCount"], 9);
    assert_eq!(a["churn"], 120);
    assert_eq!(a["authorCount"], 3);
    assert_eq!(a["lastModified"], "2026-07-01T00:00:00Z");
    // ... and the structural axes are untouched by the addition.
    assert_eq!(a["loc"], 42);
    assert_eq!(a["degree"], 2);
}

/// The gate is the RUN, not the field. `output.gitWindow` is the engine's own "git collection ran"
/// signal; without it every `nodes[].changeCount` on the wire is a hardcoded `0` (`build_one` defaults
/// the whole git block when there is no history), so copying those into a styling column would paint a
/// repo that was never measured as a repo that never changes.
#[test]
fn no_git_run_means_no_git_columns_at_all() {
    let u = super::super::dep::collect(&one_tree_with_loc());
    let (nodes, _) = nodes_ndjson(&u, None);
    let axes = git_axes();
    assert!(!axes.is_empty(), "the git fixture must supply history axes");
    for r in rows(&nodes) {
        for axis in &axes {
            assert!(
                r.get(axis).is_none(),
                "git did not run, so no row may carry {axis} — got {r}"
            );
        }
    }
}

/// Within a run that DID collect, a file git has no row for is still unmeasured, and `lastModified:
/// null` is not a date. Both omit rather than default.
#[test]
fn a_file_git_has_no_row_for_carries_no_git_columns() {
    let u = super::super::dep::collect(&one_tree_with_git());
    let (nodes, _) = nodes_ndjson(&u, None);
    let rows = rows(&nodes);
    let c = rows
        .iter()
        .find(|r| r["id"] == "src/c.ts")
        .expect("src/c.ts is a node even though nodes[] has no row for it");
    let axes = git_axes();
    assert!(!axes.is_empty(), "the git fixture must supply history axes");
    for axis in &axes {
        assert!(
            c.get(axis).is_none(),
            "nodes[] has no row for src/c.ts, so no {axis} may be invented — got {c}"
        );
    }
    // b.ts HAS a row, all zeroes — that is a real measurement and rides. Its null date does not.
    let b = rows
        .iter()
        .find(|r| r["id"] == "src/b.ts")
        .expect("src/b.ts is a node");
    assert_eq!(b["changeCount"], 0);
    assert!(
        b.get("lastModified").is_none(),
        "a null date is not a date — omit it: {b}"
    );
}

/// Omitting an unmeasured axis is honest but SILENT: the viewer simply has one fewer column to offer,
/// and a reader wondering where "colour by churn" went has nothing to read. The census is this lane's
/// honesty channel (stderr, never a row), so the coverage of the measured axes belongs in it — and only
/// in the NODES census, since the links table has no node axes to report on.
#[test]
fn the_census_says_which_measured_axes_actually_rode() {
    let (_, with_git) = nodes_ndjson(&super::super::dep::collect(&one_tree_with_git()), None);
    let line = with_git.render();
    assert!(line.contains("loc on 3 of 4"), "{line}");
    assert!(line.contains("git history on 3 of 4"), "{line}");

    let (_, no_git) = nodes_ndjson(&super::super::dep::collect(&one_tree()), None);
    let line = no_git.render();
    assert!(
        line.contains("loc on 0 of 4") && line.contains("git history on 0 of 4"),
        "a run that measured nothing must SAY so rather than leaving the missing columns to inference: {line}"
    );

    // The links census is about edges; claiming node-axis coverage there would be describing a table
    // the reader is not looking at.
    let (_, links) = links_ndjson(&super::super::dep::collect(&one_tree_with_git()), None);
    assert!(!links.render().contains("loc on"), "{}", links.render());
}

/// Cycle membership is the highest-severity structural fact this domain carries. The mermaid lane draws
/// it as a thick arrow; a viewer has no arrow styles, so it has to survive as a COLUMN or it is lost.
#[test]
fn cycle_membership_survives_into_both_tables() {
    let u = super::super::dep::collect(&one_tree());
    let (nodes, _) = nodes_ndjson(&u, None);
    let leaf = rows(&nodes)
        .into_iter()
        .find(|r| r["id"] == "src/leaf.ts")
        .expect("leaf is a node");
    assert_eq!(leaf["inCycle"], false);

    let (links, _) = links_ndjson(&u, None);
    assert!(
        rows(&links).iter().all(|r| r["inCycle"] == true),
        "all three edges are inside the 3-file cycle: {links}"
    );
}

/// An edge pointing at a node the table does not contain is a dangling reference in the viewer. The
/// mermaid lane states this rule in its note; here it has to actually hold across two files.
#[test]
fn scope_drops_edges_whose_other_end_it_dropped() {
    let tree = serde_json::json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "ir": { "dep": { "keep/a.ts": ["drop/b.ts"], "keep/c.ts": ["keep/a.ts"] }},
                "findings": []
            }
        }]
    });
    let u = super::super::dep::collect(&tree);
    let (nodes, census) = nodes_ndjson(&u, Some("keep/"));
    let ids: Vec<String> = rows(&nodes)
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec!["keep/a.ts", "keep/c.ts"]);

    let (links, _) = links_ndjson(&u, Some("keep/"));
    let links = rows(&links);
    assert_eq!(links.len(), 1, "only the wholly-in-scope edge survives");
    assert_eq!(links[0]["source"], "keep/c.ts");
    assert_eq!(links[0]["target"], "keep/a.ts");
    // The drop has to be REPORTED, not merely correct.
    assert!(
        census.render().contains("--scope dropped"),
        "{}",
        census.render()
    );
}

/// Byte-stability is the same contract every other zzop surface carries; a viewer that reloads a file
/// must not see a different graph because a `HashMap` iterated differently.
#[test]
fn output_is_byte_stable_across_runs() {
    let u = super::super::dep::collect(&one_tree());
    for _ in 0..3 {
        let (n, _) = nodes_ndjson(&u, None);
        let (l, _) = links_ndjson(&u, None);
        let (n2, _) = nodes_ndjson(&super::super::dep::collect(&one_tree()), None);
        let (l2, _) = links_ndjson(&super::super::dep::collect(&one_tree()), None);
        assert_eq!(n, n2);
        assert_eq!(l, l2);
    }
}

/// A path containing a quote or a comma is exactly the case a hand-rolled CSV writer gets wrong; the
/// reason this lane is NDJSON is that `serde_json` owns the escaping. Pinned so a future "let's emit CSV
/// instead" arrives with this test in front of it.
#[test]
fn punctuation_in_a_path_cannot_corrupt_a_row() {
    let tree = serde_json::json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "ir": { "dep": { "src/we\"ird, name.ts": ["src/b.ts"] }},
                "findings": []
            }
        }]
    });
    let u = super::super::dep::collect(&tree);
    let (nodes, _) = nodes_ndjson(&u, None);
    let weird = rows(&nodes)
        .into_iter()
        .find(|r| r["label"] == "we\"ird, name.ts")
        .expect("the awkward name round-trips through one line");
    assert_eq!(weird["folder"], "src");
}

// -------------------------------------------------------------------------------------------------
// Published-column parity — the columns this lane EMITS vs. the two documents that publish the schema
// -------------------------------------------------------------------------------------------------
//
// This lane's column list lives in THREE places: the `json!` blocks in `cosmograph.rs` (the truth), the
// `zzop graph` lane's `emits` prose in `docs/contracts/surface-parity.json`, and `site/usage.html`'s
// command table. Nothing compared them, and the predictable happened — measured 2026-07-29,
// `site/usage.html` had drifted to a seven-column node list, silently dropping `source` and `path`,
// which the emitter has written on every row since this format landed. A reader mapping columns in a
// viewer would have gone looking for a `path` column the published schema said did not exist.
//
// The truth side is DERIVED BY RUNNING THE EMITTER, not by reading its source: the three fixtures above
// already differ in exactly the axis each conditional column depends on, so subtracting their emitted
// key sets separates the classes the prose has to get right on its own terms.

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The top-level key set of every emitted row, one entry per row.
fn row_column_sets(ndjson: &str) -> Vec<BTreeSet<String>> {
    rows(ndjson)
        .iter()
        .map(|row| {
            row.as_object()
                .expect("every emitted row is a JSON object")
                .keys()
                .cloned()
                .collect()
        })
        .collect()
}

/// The union of every emitted row's keys — every column this run could put on the wire.
fn columns(ndjson: &str) -> BTreeSet<String> {
    row_column_sets(ndjson).into_iter().flatten().collect()
}

fn node_columns(tree: &Value) -> BTreeSet<String> {
    let u = super::super::dep::collect(tree);
    columns(&nodes_ndjson(&u, None).0)
}

/// The four column classes, each derived by DIFFERENCE between two fixtures that differ only in the
/// measurement that gates it:
///   - STRUCTURAL: what a run measuring nothing emits. Read off the graph, so every row has them.
///   - loc-gated:  what `ir.loc` adds.
///   - git-gated:  what a non-null `gitWindow` plus `nodes[]` adds on top.
///   - links:      the other table entirely.
///
/// Subtraction rather than four hand-sorted lists: the classification is then a PROPERTY OF THE
/// EMITTER, so a column that changes which class it belongs to (a structural one becoming conditional,
/// or the reverse) moves here without anyone deciding it did. Getting that wrong is the failure mode
/// that matters most for this lane — its whole doctrine is that an unmeasured axis is an ABSENT key and
/// never a `0`, so a document promising an always-present column that is really conditional tells a
/// reader their viewer is broken when it is behaving exactly as designed.
fn emitted_column_classes() -> [(&'static str, BTreeSet<String>); 4] {
    let plain = node_columns(&one_tree());
    let with_loc = node_columns(&one_tree_with_loc());
    let with_git = node_columns(&one_tree_with_git());
    let links = {
        let u = super::super::dep::collect(&one_tree_with_git());
        columns(&links_ndjson(&u, None).0)
    };

    // The fixtures must actually nest, or the subtractions below describe nothing. `one_tree_with_loc`
    // is `one_tree` plus `ir.loc`, and `one_tree_with_git` is that plus git — assert the emitted key
    // sets reflect it rather than trusting the fixture names.
    assert!(
        plain.is_subset(&with_loc) && with_loc.is_subset(&with_git),
        "the three node fixtures no longer nest (plain {plain:?} / +loc {with_loc:?} / +git \
         {with_git:?}) — the class subtractions below are only meaningful while each fixture adds a \
         measurement to the previous one"
    );

    // Cross-check on the STRUCTURAL definition, from the other direction: "what a run measuring nothing
    // emits" must equal "what EVERY row of the fully-measured run carries". The git fixture is built so
    // this can fail — `src/b.ts` is absent from its `loc` map and `src/c.ts` from its `nodes[]` — so a
    // conditional column would have to ride a row that never measured it to slip through both.
    let always: BTreeSet<String> = {
        let u = super::super::dep::collect(&one_tree_with_git());
        let per_row = row_column_sets(&nodes_ndjson(&u, None).0);
        assert!(!per_row.is_empty(), "the git fixture must emit rows");
        per_row
            .into_iter()
            .reduce(|a, b| a.intersection(&b).cloned().collect())
            .expect("non-empty")
    };
    assert_eq!(
        plain, always,
        "the two independent derivations of the STRUCTURAL column set disagree: a run that measured \
         nothing emits {plain:?}, while the keys present on EVERY row of the fully-measured run are \
         {always:?}. One of them is not what the prose should publish as always-present."
    );

    let loc_gated: BTreeSet<String> = with_loc.difference(&plain).cloned().collect();
    let git_gated: BTreeSet<String> = with_git.difference(&with_loc).cloned().collect();
    for (label, set) in [("loc-gated", &loc_gated), ("git-gated", &git_gated)] {
        assert!(
            !set.is_empty(),
            "the {label} column class came out EMPTY — either the emitter stopped gating that axis (it \
             now writes a default, which this lane's doctrine forbids) or the fixture stopped supplying \
             the measurement. Either way the prose check below would stop asserting anything about it."
        );
    }

    [
        ("STRUCTURAL node", plain),
        ("loc-gated node", loc_gated),
        ("git-gated node", git_gated),
        ("links-table", links),
    ]
}

/// Every maximal `/`-joined run of code tokens on a page, as a SET each.
///
/// A run — backtick-delimited names joined by slashes in the registry's prose, and the
/// `<code>a</code>/<code>b</code>` form in HTML — is how
/// both documents spell a column list, and it is the smallest anchor that makes the check exact in BOTH
/// directions (a column added to the emitter and a column the emitter no longer writes both break it).
/// A looser matcher — does the page mention `path` anywhere? — would have passed the very drift that
/// prompted this: `path` occurs a dozen times in both documents as ordinary English and inside other
/// identifiers, which is exactly why nobody noticed the column list had lost it.
///
/// Comparing SETS rather than one rendered string leaves each document free to order its list for a
/// reader: the emitted byte order is alphabetical (`serde_json`'s `Map` is a `BTreeMap` in this build),
/// which is not the order either page reads best in.
fn code_token_runs(text: &str, open: &str, close: &str) -> Vec<BTreeSet<String>> {
    let (o, c) = (regex::escape(open), regex::escape(close));
    let run_re = regex::Regex::new(&format!(
        "(?:{o}[A-Za-z][A-Za-z0-9]*{c}/)+{o}[A-Za-z][A-Za-z0-9]*{c}"
    ))
    .expect("static shape, escaped delimiters");
    let token_re = regex::Regex::new(&format!("{o}([A-Za-z][A-Za-z0-9]*){c}"))
        .expect("static shape, escaped delimiters");
    run_re
        .find_iter(text)
        .map(|run| {
            token_re
                .captures_iter(run.as_str())
                .map(|cap| cap[1].to_string())
                .collect()
        })
        .collect()
}

/// Whether `text` publishes exactly `cols` as one column list. A single-column class has no `/` to run
/// on, so it is asserted as a lone delimited token instead — the same claim, minus a separator.
fn publishes(text: &str, open: &str, close: &str, cols: &BTreeSet<String>) -> bool {
    match cols.iter().next() {
        Some(only) if cols.len() == 1 => text.contains(&format!("{open}{only}{close}")),
        _ => code_token_runs(text, open, close).contains(cols),
    }
}

fn rendered(cols: &BTreeSet<String>, open: &str, close: &str) -> String {
    cols.iter()
        .map(|col| format!("{open}{col}{close}"))
        .collect::<Vec<_>>()
        .join("/")
}

/// The `zzop graph` lane's `emits` prose, located by the `sources` array that names THIS lane's emitter
/// rather than by the lane's display name — a lane can be renamed, but the entry claiming to implement
/// `cosmograph.rs` is the one whose prose describes these columns.
fn graph_lane_emits() -> String {
    const EMITTER: &str = "crates/summary/src/graph/cosmograph.rs";
    let text = read_repo_file("docs/contracts/surface-parity.json");
    let registry: Value =
        serde_json::from_str(&text).expect("docs/contracts/surface-parity.json must be valid JSON");
    let lanes = registry["_cliOnlyLanes"]
        .as_object()
        .expect("surface-parity.json must carry a `_cliOnlyLanes` object");
    for (lane, entry) in lanes {
        let names_emitter = entry
            .get("sources")
            .and_then(|v| v.as_array())
            .is_some_and(|sources| sources.iter().any(|p| p.as_str() == Some(EMITTER)));
        if names_emitter {
            return entry["emits"]
                .as_str()
                .unwrap_or_else(|| panic!("_cliOnlyLanes[{lane:?}] must carry an `emits` string"))
                .to_string();
        }
    }
    panic!(
        "no `_cliOnlyLanes` entry in docs/contracts/surface-parity.json declares {EMITTER} among its \
         `sources` — this lane's published schema has no owner there, so nothing documents the columns \
         it puts on the wire. Add the path to the owning lane's `sources` array."
    );
}

/// The drift guard: every column class this lane emits must be published, as a column list, by BOTH
/// documents that describe the schema.
///
/// Scope, stated honestly. This proves each document names the right SET of columns per class. It does
/// not read the surrounding sentence, so a page could still attach a correct list to a wrong claim
/// ("these ride on every row", over the git-gated set). Pinning that would mean pinning wording, which
/// this repo does not do; what it does instead is give each class its own list, so the classes are at
/// least separately visible and separately wrong.
#[test]
fn both_published_schemas_name_exactly_the_columns_this_lane_emits() {
    let registry_emits = graph_lane_emits();
    let usage = read_repo_file("site/usage.html");
    let sites: [(&str, &str, &str, &str); 2] = [
        (
            "docs/contracts/surface-parity.json (the `zzop graph` lane's `emits`)",
            &registry_emits,
            "`",
            "`",
        ),
        ("site/usage.html", &usage, "<code>", "</code>"),
    ];

    let mut offenders = Vec::new();
    for (label, cols) in emitted_column_classes() {
        for (site, text, open, close) in sites {
            if !publishes(text, open, close, &cols) {
                offenders.push(format!(
                    "{site} does not publish the {label} column list. `zzop graph --format \
                     cosmograph-*` emits exactly {cols:?} for that class; the document must name them \
                     as one `/`-joined run of code tokens: {}",
                    rendered(&cols, open, close)
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the cosmograph column schema and the documents that publish it disagree: {offenders:#?}"
    );
}
