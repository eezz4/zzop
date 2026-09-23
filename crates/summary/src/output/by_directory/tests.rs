//! Unit seals for the FOLD itself. The wire behaviour — the key rides every reply, its legend is
//! folded and reachable — belongs to `crates/summary/src/output/tests.rs` and
//! `crates/summary/tests/legend_fold.rs`; what belongs here is arithmetic and ordering, plus the two
//! properties the 2026-09-11 ruling turns on.

use super::*;

fn finding(file: &str) -> Value {
    json!({ "ruleId": "x", "severity": "warning", "file": file, "line": 1 })
}

/// The block, for the fixtures that HAVE one. `by_directory` returns `None` when there is no
/// distribution to report (see its own doc), so tests about the ROWS say so by unwrapping here rather
/// than each handling an absence it is not about.
fn built(findings: &[Value]) -> Value {
    by_directory(findings).expect("this fixture has a distribution to report")
}

fn dirs(v: &Value) -> Vec<(String, u64, f64)> {
    v["directories"]
        .as_array()
        .expect("directories is an array")
        .iter()
        .map(|r| {
            (
                r["dir"].as_str().unwrap().to_string(),
                r["findings"].as_u64().unwrap(),
                r["sharePct"].as_f64().unwrap(),
            )
        })
        .collect()
}

/// THE SHAPE the ruling asked for: name, count, share, and no verdict word anywhere on the wire. The
/// second half is the load-bearing one — a row that shipped a `kind`/`likelyNoise`/`recommended` field
/// would be the guess the measurement refused, and it would be refused in the rows rather than in the
/// prose.
#[test]
fn a_row_carries_a_name_a_count_and_a_share_and_nothing_that_judges() {
    let out = built(&[
        finding("docs_src/a.py"),
        finding("docs_src/b.py"),
        finding("m.py"),
    ]);
    assert_eq!(
        dirs(&out),
        vec![
            ("docs_src/".to_string(), 2, 66.7),
            ("(root)".to_string(), 1, 33.3),
        ],
        "{out}"
    );
    let row = &out["directories"][0];
    let keys: Vec<&String> = row.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        vec!["dir", "findings", "sharePct"],
        "a row is three facts; anything else here is the judgement the 2026-09-11 ruling moved to the \
         reader: {row}"
    );
}

/// The measured corpus shape, in miniature: a tree whose findings pile into one directory says so, and
/// says it with a share a reader can compare against the table in the module doc. 484 of 511 is the
/// fastapi row, reproduced by `zzop analyze corpus/frameworks/fastapi --limit 1000`.
#[test]
fn the_fastapi_shape_reports_its_own_share() {
    let mut findings: Vec<Value> = (0..484)
        .map(|i| finding(&format!("docs_src/f{i}.py")))
        .collect();
    findings.extend((0..19).map(|i| finding(&format!("tests/t{i}.py"))));
    findings.extend((0..4).map(|i| finding(&format!("scripts/s{i}.py"))));
    findings.extend((0..2).map(|i| finding(&format!("fastapi/p{i}.py"))));
    findings.extend((0..2).map(|i| finding(&format!("docs/d{i}.md"))));
    let out = built(&findings);
    assert_eq!(findings.len(), 511, "the fixture is the measured total");
    assert_eq!(dirs(&out)[0], ("docs_src/".to_string(), 484, 94.7), "{out}");
    assert!(
        out["basis"].as_str().unwrap().contains("511 finding(s)")
            && out["basis"]
                .as_str()
                .unwrap()
                .contains("across 5 top-level"),
        "basis must name the population and the directory count: {out}"
    );
}

/// INVERSION — the same shaped fold over the typeorm row, whose top segment is the PRODUCT. Pinned
/// beside the one above because the pair is the whole argument for facts-only: two identical-looking
/// outputs that mean opposite things, which is why neither carries a label.
#[test]
fn the_typeorm_shape_is_indistinguishable_from_the_fastapi_one_on_the_wire() {
    let mut findings: Vec<Value> = (0..25).map(|i| finding(&format!("src/f{i}.ts"))).collect();
    findings.extend((0..16).map(|i| finding(&format!("test/t{i}.ts"))));
    findings.push(finding("packages/p.ts"));
    findings.push(finding("rollup.config.js"));
    let out = built(&findings);
    assert_eq!(dirs(&out)[0], ("src/".to_string(), 25, 58.1), "{out}");
    let rendered = serde_json::to_string(&out["directories"]).unwrap();
    assert!(
        !rendered.contains("noise") && !rendered.contains("product"),
        "the rows must be readable only as a distribution: {rendered}"
    );
}

/// The SHORTLIST bar and the sentence that keeps it honest. A cut that did not disclose itself would
/// be the silent filter every other capped list in this module announces.
#[test]
fn a_cut_list_says_how_much_it_left_out() {
    let findings: Vec<Value> = (0..MAX_ROWS + 3)
        .flat_map(|i| (0..(MAX_ROWS + 3 - i)).map(move |_| finding(&format!("d{i}/x.ts"))))
        .collect();
    let out = built(&findings);
    assert_eq!(out["directories"].as_array().unwrap().len(), MAX_ROWS);
    let basis = out["basis"].as_str().unwrap();
    assert!(
        basis.contains(&format!("across {} top-level", MAX_ROWS + 3))
            && basis.contains(&format!("the {MAX_ROWS} row(s) below")),
        "the cut must name both the full directory count and the shown one: {basis}"
    );
    let covered: f64 = dirs(&out).iter().map(|(_, _, s)| s).sum();
    assert!(
        covered < 100.0,
        "this fixture's tail is outside the rows, so the shown shares cannot sum to the whole: {out}"
    );
}

/// Ties break by NAME, so one tree serializes to the same bytes twice. Every other ordering in this
/// module is a total order for the same reason.
#[test]
fn equal_counts_break_by_name() {
    let out = built(&[finding("zed/a.ts"), finding("alpha/b.ts")]);
    assert_eq!(
        dirs(&out)
            .iter()
            .map(|(d, ..)| d.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha/", "zed/"],
        "{out}"
    );
}

/// The EMPTY case is the reason `basis` exists at all: with no rows, the reader must still be able to
/// tell a fold that ran from a question nobody asked.
/// NOTHING TO DISTRIBUTE means the block is ABSENT — the contract `testPaths`/`buildPaths` already
/// keep, adopted here on 2026-09-15 (ledger V243) so one `findings` object stops carrying three
/// emission doctrines into a freeze.
///
/// Two shapes, because they are wrong in different ways. A run with no findings has nothing to say and
/// the `total: 0` beside it already says it. A one-directory run restates that total as a single 100%
/// row — 📏 1,848 bytes of a 19,700-byte reply, 1,595 of them the note explaining how to read a
/// distribution that has nothing to distribute.
#[test]
fn a_fold_with_no_distribution_to_report_is_absent_rather_than_a_single_hundred_percent_row() {
    assert!(
        by_directory(&[]).is_none(),
        "a run with no findings has nothing to distribute"
    );
    assert!(
        by_directory(&[finding("src/a.ts"), finding("src/b.ts")]).is_none(),
        "one directory is the total restated, not a distribution"
    );
    // FLOOR: two directories must still report, or the gate has eaten the channel. Note this fixture
    // is 80/20 — a SHARE threshold would have hidden it, and this gate reads no share at all.
    let two = built(&[
        finding("src/a.ts"),
        finding("src/b.ts"),
        finding("src/c.ts"),
        finding("src/d.ts"),
        finding("docs/e.ts"),
    ]);
    assert_eq!(dirs(&two).len(), 2, "{two}");
}

/// The `MEASURED empty` sentence survives, and THIS is the case it was written for: findings exist and
/// none carries a path. The rows are empty and that is a measurement rather than an unasked question,
/// so the gate above spares it — suppressing it would delete the one thing this channel says when it
/// has no rows at all.
#[test]
fn findings_with_no_path_at_all_still_report_that_the_fold_was_computed() {
    let out = by_directory(&[
        json!({ "ruleId": "x", "severity": "warning" }),
        json!({ "ruleId": "x", "severity": "warning", "file": "  " }),
    ])
    .expect("pathless findings are still something to disclose");
    assert!(out["directories"].as_array().unwrap().is_empty());
    let basis = out["basis"].as_str().unwrap();
    assert!(
        basis.contains("MEASURED empty"),
        "an empty distribution must say it was computed: {basis}"
    );
}

/// A finding with no path is not silently dropped — it leaves the rows (there is nothing to place it
/// under) and enters `basis`, so the row shares and the reply's `total` can be reconciled.
#[test]
fn a_pathless_finding_is_counted_in_the_basis_rather_than_vanishing() {
    // ONE directory, but two findings carry no path — so there IS something to disclose and the gate
    // spares it. That asymmetry is in the gate's own doc; this is where it is pinned.
    let out = by_directory(&[
        finding("src/a.ts"),
        json!({ "ruleId": "x", "severity": "warning" }),
        json!({ "ruleId": "x", "severity": "warning", "file": "  " }),
    ])
    .expect("unplaced findings are something to say");
    assert_eq!(dirs(&out), vec![("src/".to_string(), 1, 100.0)], "{out}");
    assert!(
        out["basis"]
            .as_str()
            .unwrap()
            .contains("2 finding(s) carry no file path"),
        "{out}"
    );
}

/// The legend rides FOLDED, resolved through the same list the contract document renders from — the
/// property `legend_fold.rs` proves end to end, pinned here so a broken key spelling fails in the
/// module that owns it.
#[test]
fn the_meaning_is_the_folded_note_and_not_the_full_text() {
    let out = built(&[finding("src/a.ts"), finding("docs/b.ts")]);
    let meaning = out["meaning"].as_str().expect("meaning is a string");
    assert_eq!(meaning, crate::output::legends::by_directory_note());
    assert!(
        !meaning.contains("THAT REFUSAL IS A MEASUREMENT"),
        "the full text must not ride the wire: {meaning}"
    );
    assert!(
        meaning.contains("`exclude`") && meaning.contains("`vocabulary.skipDirs`"),
        "the remedy clause names BOTH keys or it is advice to do the wrong one: {meaning}"
    );
}

/// `./`-prefixed paths fold to the same bucket as their bare form. Cheap, and the alternative is two
/// rows for one directory in any tree whose engine output ever carries the prefix.
#[test]
fn a_dot_slash_prefix_does_not_make_a_second_bucket() {
    // A third finding in a SECOND directory, so the block exists and the fold is visible as a row.
    // Without it the two `src/` spellings collapse to one directory and the block is correctly absent
    // — which would prove the same thing while showing the reader nothing.
    let out = built(&[
        finding("./src/a.ts"),
        finding("src/b.ts"),
        finding("docs/c.ts"),
    ]);
    assert_eq!(
        dirs(&out),
        vec![
            ("src/".to_string(), 2, 66.7),
            ("docs/".to_string(), 1, 33.3),
        ],
        "{out}"
    );
}
