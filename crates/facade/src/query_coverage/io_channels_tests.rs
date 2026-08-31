//! Tests for the `ioChannels` cell, driven through the PUBLIC reply (`query_coverage_json`) rather
//! than the helper — the defect these close is a reader of the shipped JSON being unable to tell an
//! empty channel from a full one, so the assertion has to stand where that reader stands.
//!
//! Every emission here is paired: one fixture where the row MUST appear and one where it MUST NOT.
//! A "0 was reported" test with no silence twin proves only that the code can print.

use serde_json::{json, Value};

fn reply(trees: Value) -> Value {
    let out = crate::query_coverage_json(&json!({ "trees": trees }).to_string())
        .expect("coverage query must succeed");
    serde_json::from_str(&out).expect("reply is JSON")
}

/// A tree in the shape gogs (`d460e50`) was measured in: Go files that all project structure, a
/// filled `db-table` provide channel, and an EMPTY http provide channel. `joinContributionZero` is
/// stated as the census's own rule computes it (`files > 0 && provides == 0 && keyed == 0`) — false
/// here, which is exactly the masking this cell exists to undo.
fn db_full_http_empty_go_tree() -> Value {
    json!({
        "sourceId": "gogs",
        "output": {
            "ir": {
                "loc": { "internal/db/user.go": 100, "internal/route/repo.go": 80,
                         "internal/conf/conf.go": 40 },
                "symbols": [ { "file": "internal/db/user.go", "name": "User" },
                             { "file": "internal/route/repo.go", "name": "Repo" },
                             { "file": "internal/conf/conf.go", "name": "Conf" } ],
                "dep": {},
                "io": {
                    "provides": [ { "kind": "db-table", "key": "user",
                                    "file": "internal/db/user.go", "line": 1 } ],
                    "consumes": []
                }
            },
            "degraded": [],
            "coverage": { "files": 3, "parserDispatched": 3, "symbols": 3,
                          "resolvedImportEdges": 0, "ioProvides": 1, "ioConsumesKeyed": 0,
                          "ioConsumesUnresolved": 0, "degraded": 0,
                          "joinContributionZero": false }
        }
    })
}

/// The same tree with its http provide channel FILLED — the silence twin for every
/// `io.provides`-on-`go` assertion below.
fn http_extracting_go_tree() -> Value {
    let mut t = db_full_http_empty_go_tree();
    t["output"]["ir"]["io"]["provides"] = json!([
        { "kind": "db-table", "key": "user", "file": "internal/db/user.go", "line": 1 },
        { "kind": "http", "key": "POST /repo", "file": "internal/route/repo.go", "line": 12 }
    ]);
    t["output"]["ir"]["io"]["consumes"] = json!([
        { "kind": "http", "key": "GET /hook", "file": "internal/route/repo.go", "line": 40 }
    ]);
    t
}

/// A tree whose only structural files are in a language NO recognizer in this build claims — the
/// second silence twin: capability absent, so no channel of this tree may be called blind.
fn no_recognizer_tree() -> Value {
    json!({
        "sourceId": "rb",
        "output": {
            "ir": {
                "loc": { "app/models/user.rb": 30, "README.md": 10 },
                "symbols": [ { "file": "app/models/user.rb", "name": "User" } ],
                "dep": {},
                "io": { "provides": [], "consumes": [] }
            },
            "degraded": [],
            "coverage": { "files": 2, "parserDispatched": 1, "symbols": 1,
                          "resolvedImportEdges": 0, "ioProvides": 0, "ioConsumesKeyed": 0,
                          "ioConsumesUnresolved": 0, "degraded": 0,
                          "joinContributionZero": true }
        }
    })
}

/// The shape immich (`server/src/schema`) and probe3 were measured in: `.ts` files that all project
/// structure, `db-table` filled on the CONSUME side, and the `db-table` PROVIDE side at zero. This is
/// the fixture the recognizer-attribution defect lives on — the row it produces is about provides,
/// while two of the three recognizers it used to name (`raw sql`, `prisma client`) only ever build
/// consumes.
fn db_consuming_ts_tree() -> Value {
    json!({
        "sourceId": "ts-consume",
        "output": {
            "ir": {
                "loc": { "src/a.ts": 40, "src/b.ts": 40, "src/c.ts": 40 },
                "symbols": [ { "file": "src/a.ts", "name": "a" },
                             { "file": "src/b.ts", "name": "b" },
                             { "file": "src/c.ts", "name": "c" } ],
                "dep": {},
                "io": {
                    "provides": [],
                    "consumes": [ { "kind": "db-table", "key": "table:widgets",
                                    "file": "src/a.ts", "line": 3 } ]
                }
            },
            "degraded": [],
            "coverage": { "files": 3, "parserDispatched": 3, "symbols": 3,
                          "resolvedImportEdges": 0, "ioProvides": 0, "ioConsumesKeyed": 1,
                          "ioConsumesUnresolved": 0, "degraded": 0,
                          "joinContributionZero": false }
        }
    })
}

fn channels(tree: Value) -> Value {
    reply(json!([tree]))["trees"][0]["ioChannels"].clone()
}

fn rows(v: &Value, key: &str) -> Vec<Value> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("ioChannels.{key} must be an array, got {v}"))
        .clone()
}

/// MECHANISM 3, the whole point: a kind-agnostic zero test cannot tell channels apart, so one full
/// channel vouches for another empty one. The census on this fixture says `joinContributionZero:
/// false` and `joinVisibility` says the tree contributed joinable io — both TRUE of the tree and
/// both silent about its http channel. `extracted` must carry a row for EVERY io kind this build's
/// rules read, present at zero, so the http zero is byte-visible beside the db-table one.
#[test]
fn every_read_io_kind_keeps_a_row_so_a_full_channel_cannot_vouch_for_an_empty_one() {
    let tree = db_full_http_empty_go_tree();
    let view = reply(json!([tree.clone()]))["trees"][0].clone();
    // The masking, pinned: the pre-existing signals both read "contributed".
    assert_eq!(view["census"]["joinContributionZero"], json!(false));
    assert_eq!(view["joinVisibility"]["provides"], json!(1));

    let ch = channels(tree);
    let extracted = rows(&ch, "extracted");
    let kinds: Vec<&str> = extracted
        .iter()
        .map(|r| r["kind"].as_str().expect("kind is a string"))
        .collect();
    for want in zzop_core::RULE_READ_IO_KINDS {
        assert!(
            kinds.contains(want),
            "no `extracted` row for read io kind {want:?}: {kinds:?}"
        );
    }
    let http = extracted
        .iter()
        .find(|r| r["kind"] == json!("http"))
        .expect("http row");
    assert_eq!(http["provides"], json!(0), "{http}");
    let db = extracted
        .iter()
        .find(|r| r["kind"] == json!("db-table"))
        .expect("db-table row");
    assert_eq!(db["provides"], json!(1), "{db}");
}

/// MECHANISMS 1+2, the derivable half: the empty channel is named by the LANGUAGE this build has an
/// extractor for, never by recognizing a framework by name — so a framework no hand-kept vocabulary
/// lists (gogs' macaron is the measured one) still produces the row.
#[test]
fn a_capable_language_that_extracted_no_route_is_named_with_its_file_count() {
    let ch = channels(db_full_http_empty_go_tree());
    let zero = rows(&ch, "zeroExtraction");
    let go = zero
        .iter()
        .find(|r| r["ext"] == json!("go") && r["channel"] == json!("io.provides"))
        .unwrap_or_else(|| panic!("no io.provides row for `go`: {zero:#?}"));
    assert_eq!(go["structuralFiles"], json!(3), "{go}");
    assert_eq!(go["extracted"], json!(0), "{go}");
    // The recognizers are quoted from the build's own capability table, never restated here.
    let names: Vec<&str> = go["recognizers"]
        .as_array()
        .expect("recognizers array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(names.contains(&"gin"), "{go}");
    assert!(names.contains(&"net/http"), "{go}");
    // The channel this tree DID fill must not be called blind on the same extension.
    assert!(
        !zero
            .iter()
            .any(|r| r["ext"] == json!("go") && r["channel"] == json!("io.provides:db-table")),
        "the filled db-table channel was reported blind: {zero:#?}"
    );
}

/// The principal-share floor, pinned in BOTH directions in one assertion: a capable extension that is
/// a rounding error of what this run read gets no row, while the principal one beside it — blind on the
/// same channel, in the same tree — still does. Measured 2026-08-20 before the floor existed: zzop's own
/// tree produced 25 rows, and directus produced one for a 2-file `.cjs`; the row that carries the
/// finding (gogs `io.provides`/`go`, 301 files, ~300 unextracted routes) was buried among them. The
/// after-counts live in one place, `io_channels.rs`'s `zero_extraction` doc, rather than being restated
/// here — that doc went stale once already by carrying a copy nobody re-measured.
#[test]
fn a_capable_extension_below_the_principal_floor_gets_no_row_while_the_principal_one_still_does() {
    let mut tree = db_full_http_empty_go_tree();
    // One `.cjs` file against three `.go` ones — 25% would clear a tenth, so make it a real minority.
    let loc = tree["output"]["ir"]["loc"]
        .as_object_mut()
        .expect("loc map");
    for i in 0..40 {
        loc.insert(format!("internal/route/gen{i}.go"), json!(10));
    }
    loc.insert("scripts/tool.cjs".to_string(), json!(10));
    let ch = channels(tree);
    let zero = rows(&ch, "zeroExtraction");
    assert!(
        zero.iter()
            .any(|r| r["ext"] == json!("go") && r["channel"] == json!("io.provides")),
        "the principal filetype must keep its row: {zero:#?}"
    );
    assert!(
        !zero.iter().any(|r| r["ext"] == json!("cjs")),
        "a filetype under the principal floor must not produce a row: {zero:#?}"
    );
}

/// SILENCE TWIN 1 — extraction succeeded, so neither http channel may be named. Without this the
/// test above only proves the code can print a row.
#[test]
fn a_language_that_filled_its_channel_is_never_named_blind() {
    let ch = channels(http_extracting_go_tree());
    let zero = rows(&ch, "zeroExtraction");
    assert!(
        !zero.iter().any(|r| r["ext"] == json!("go")),
        "a tree that extracted routes and calls was still reported blind: {zero:#?}"
    );
}

/// SILENCE TWIN 2 — no recognizer exists for `.rb` in this build, so no row may claim its channels
/// went empty. "This build cannot see Ruby at all" is `frameworkRecognizers`' answer, and stating it
/// here as a per-run blindness would double-report it as a tree defect.
#[test]
fn an_extension_with_no_recognizer_is_never_named_blind() {
    let ch = channels(no_recognizer_tree());
    assert!(
        rows(&ch, "zeroExtraction").is_empty(),
        "a language this build has no recognizer for was reported blind: {ch:#}"
    );
    // ...and the kind rows are still all present: "measured 0" is the answer for every channel.
    assert_eq!(
        rows(&ch, "extracted").len(),
        zzop_core::RULE_READ_IO_KINDS.len()
    );
}

/// A row names the recognizers that fill THAT channel on THAT extension — not every recognizer that
/// happens to run on the extension and every recognizer that happens to touch the kind. Measured
/// 2026-08-26 with a constructed differential over one factor (which db idiom the `.ts` file holds):
/// `@Entity("widgets")` yields 2 `db-table` provides and the row disappears, while a raw `CREATE
/// TABLE` string yields 0 of both and `prisma.user.findMany()` yields 2 db-table CONSUMES and 0
/// provides — yet the row named `raw sql` and `prisma client` in all of them. Two recognizers that
/// only ever construct `IoConsume` were being offered as evidence that a PROVIDE channel could have
/// been filled, which inverts what a reader takes from the zero: "this build has a `.ts` raw-SQL
/// table recognizer and got 0" reads as "this tree declares no tables in TypeScript", when the truth
/// is "this build does not read `CREATE TABLE` inside `.ts` at all".
#[test]
fn a_zero_row_names_only_recognizers_that_fill_that_channel_on_that_extension() {
    let ch = channels(db_consuming_ts_tree());
    let zero = rows(&ch, "zeroExtraction");
    let db = zero
        .iter()
        .find(|r| {
            r["channel"] == json!(zzop_core::recognizer::channel::DB_PROVIDES)
                && r["ext"] == json!("ts")
        })
        .unwrap_or_else(|| panic!("the db-table provide row must still be reported: {zero:#?}"));
    let names: Vec<&str> = db["recognizers"]
        .as_array()
        .expect("recognizers array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        names,
        vec!["typeorm"],
        "the db-table PROVIDE row on `ts` must name only the recognizer that builds db-table \
         provides there; `raw sql` and `prisma client` build only consumes: {db:#?}"
    );
}

/// SILENCE TWIN 3 for the assertion above — the same tree with a db-table PROVIDE present gets no
/// row at all, so the test above cannot pass by the cross going quiet.
#[test]
fn a_tree_that_declared_a_table_in_typescript_gets_no_db_provide_row() {
    let mut tree = db_consuming_ts_tree();
    tree["output"]["ir"]["io"]["provides"] = json!([
        { "kind": "db-table", "key": "table:widgets", "file": "src/b.ts", "line": 2 }
    ]);
    let ch = channels(tree);
    let zero = rows(&ch, "zeroExtraction");
    assert!(
        !zero
            .iter()
            .any(|r| r["channel"] == json!(zzop_core::recognizer::channel::DB_PROVIDES)),
        "a tree whose TypeScript declared a table was still reported blind on it: {zero:#?}"
    );
}

/// Determinism is a shipped contract: both arrays must be byte-identical across repeated builds of
/// the same input, and `zeroExtraction` must be sorted on its own key pair rather than on whatever
/// order the tree's files happened to arrive in.
#[test]
fn rows_are_order_stable() {
    let a = channels(db_full_http_empty_go_tree());
    let b = channels(db_full_http_empty_go_tree());
    assert_eq!(a, b);
    let zero = rows(&a, "zeroExtraction");
    let keys: Vec<(String, String)> = zero
        .iter()
        .map(|r| {
            (
                r["channel"].as_str().unwrap_or_default().to_string(),
                r["ext"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "zeroExtraction is not sorted: {zero:#?}");
}

/// The cell must say what it is, in the reply, or a reader has a number with no membership rule —
/// the same self-describing-reply discipline `dispatchMeaning`/`blindSpotMeaning` carry. In
/// particular it must state that a zero row is a COVERAGE FACT (a CLI that serves nothing reads 0
/// legitimately) and that the population is the tree, not a recognized-framework list.
#[test]
fn the_cell_ships_its_own_membership_rule() {
    let ch = channels(db_full_http_empty_go_tree());
    let m = ch["meaning"].as_str().expect("ioChannels.meaning");
    assert!(
        m.contains("RULE_READ_IO_KINDS") || m.contains("read io kind"),
        "{m}"
    );
    assert!(m.contains("COVERAGE FACT"), "{m}");
    assert!(m.contains("frameworkRecognizers"), "{m}");
}
