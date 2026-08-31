//! Pins for the shaped analyze reply's `coverageGaps` object — the compact statement of WHICH of this
//! tree's principal filetypes the dependency graph (and therefore every unimported/unreachable-export
//! and blast-radius judgment computed over it) does not contain.
//!
//! Both directions are pinned, because only the pair says anything: a tree WITH a gap must name it, and
//! a fully covered tree must come back with an empty list AND a basis sentence saying what was crossed.
//! A field that only ever appears when something is wrong proves nothing on the run where it is absent.

use std::path::{Path, PathBuf};

use crate::output::FindingFilters;

fn no_filters() -> FindingFilters {
    FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    }
}

/// ~20 lines of real TypeScript per file, so the LINE share of a fixture is not decided by one-liners.
fn ts_body(extra: &str) -> String {
    let filler: String = (0..16)
        .map(|i| format!("// line {i} — filler so this file has a realistic line count\n"))
        .collect();
    format!("{extra}{filler}")
}

/// A fresh tree under the OS temp dir, with the `zzop.config.jsonc` every host lane requires. Caching is
/// opted out (`cacheDir: null`) so a re-run of the suite on one machine analyzes the same way it did the
/// first time.
fn tree(name: &str, files: &[(&str, String)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zzop-covgap-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (rel, body) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    }
    std::fs::write(
        dir.join("zzop.config.jsonc"),
        "{ \"trees\": [{ \"root\": \".\" }], \"cacheDir\": null }\n",
    )
    .unwrap();
    dir
}

fn analyze(dir: &Path) -> serde_json::Value {
    let out = super::analyze_summary(Some(dir.to_str().unwrap()), None, &no_filters())
        .expect("the fixture tree must analyze");
    serde_json::from_str(&out).expect("the summary must be valid JSON")
}

fn gaps(v: &serde_json::Value) -> &serde_json::Value {
    v.get("coverageGaps")
        .unwrap_or_else(|| panic!("the shaped analyze reply must carry `coverageGaps`: {v}"))
}

fn rows(v: &serde_json::Value) -> &Vec<serde_json::Value> {
    gaps(v)["extensions"]
        .as_array()
        .unwrap_or_else(|| panic!("`coverageGaps.extensions` must be an array: {v}"))
}

fn row_for<'a>(v: &'a serde_json::Value, ext: &str) -> Option<&'a serde_json::Value> {
    rows(v).iter().find(|r| r["ext"] == ext)
}

/// Two TypeScript files that import ONE ANOTHER, so `ts` always has a resolved edge and can never be
/// the row a fixture is asserting about.
fn resolving_ts_pair() -> Vec<(&'static str, String)> {
    vec![
        ("src/a.ts", ts_body("export const a = 1;\n")),
        (
            "src/b.ts",
            ts_body("import { a } from './a';\nexport const b = a + 1;\n"),
        ),
    ]
}

/// THE DIRECTUS SHAPE, in miniature: a principal filetype no parser claims. Its files are walked and
/// line-counted, contribute no symbol and no dep-graph edge, and every export-reachability judgment is
/// computed as if they did not exist — which is exactly what a reader holding only the findings cannot
/// see today.
#[test]
fn a_principal_filetype_with_no_structural_projection_is_named() {
    let mut files = resolving_ts_pair();
    for i in 0..10 {
        let body: String = (0..30)
            .map(|n| format!("<!-- component {i} line {n} -->\n"))
            .collect();
        files.push((
            Box::leak(format!("app/c{i}.vue").into_boxed_str()),
            format!("<template>\n{body}</template>\n"),
        ));
    }
    let v = analyze(&tree("novue", &files));
    let row = row_for(&v, "vue").unwrap_or_else(|| {
        panic!(
            "`vue` dominates this tree and parses to nothing: {}",
            gaps(&v)
        )
    });
    assert_eq!(row["files"], 10, "row must carry the walked count: {row}");
    assert_eq!(
        row["structural"], 0,
        "no parser claims .vue, so the structural count is 0: {row}"
    );
}

/// Bulk body for a filetype no parser claims, in a shape that carries LINES as well as files — the
/// share filter reads both, so a fixture of one-liners cannot exercise it.
fn bulky(ext: &str, i: usize) -> String {
    let body: String = (0..30)
        .map(|n| format!("  <!-- {ext} document {i}, line {n} -->\n"))
        .collect();
    format!("<root>\n{body}</root>\n")
}

/// THE MALL SHAPE, in miniature, and the regression this row exists for: a DATA/CONFIG filetype that
/// dominates the tree and that no parser read. On macrozheng/mall those files were 114 `.xml` MyBatis
/// mappers — 906 SQL statements, 744 `${}` substitution sites, essentially every SQL statement the
/// project has — and this list came back EMPTY, because the filter was asking "should we ask for an
/// XML parser" (no) instead of "could this have cost facts" (emphatically yes).
///
/// On regression — reinstating `is_non_source_extension` as the gate here, or reclassifying the
/// data/config group as `NoFactsToLose` — this test goes red where a user would otherwise see `[]`.
#[test]
fn a_data_config_filetype_that_dominates_the_tree_is_named_and_labelled() {
    let mut files = resolving_ts_pair();
    for i in 0..10 {
        files.push((
            Box::leak(format!("mapper/m{i}.xml").into_boxed_str()),
            bulky("xml", i),
        ));
    }
    let v = analyze(&tree("mybatis", &files));
    let row = row_for(&v, "xml").unwrap_or_else(|| {
        panic!(
            "`.xml` is 10 of 12 files and most of the lines here, and nothing read it: {}",
            gaps(&v)
        )
    });
    assert_eq!(row["files"], 10, "row must carry the walked count: {row}");
    assert_eq!(row["structural"], 0, "no parser claims .xml: {row}");
    assert_eq!(
        row["kind"], "data-config",
        "the row must say WHICH remedy applies — a data/config row is a place to look, not a \
         verdict, and a reader cannot tell a mapper directory from a locale bundle without it: {row}"
    );
    let meaning = gaps(&v)["meaning"].as_str().unwrap_or_default();
    assert!(
        meaning.contains("data-config") && meaning.contains("source"),
        "the kind vocabulary must ride inside the object that uses it: {meaning:?}"
    );
}

/// The SUPPRESSION control, and the reason this change is a narrowing rather than a revert: a filetype
/// with nothing a parser could ever project stays out of the list at the very same proportions that
/// put `.xml` in it. Without this leg, "name the data files" degrades into "name every file", which is
/// the noise wall the original filter was added to prevent.
#[test]
fn a_filetype_with_no_extractable_facts_is_not_a_row_at_the_same_proportions() {
    let mut files = resolving_ts_pair();
    for i in 0..10 {
        files.push((
            Box::leak(format!("docs/d{i}.md").into_boxed_str()),
            bulky("md", i),
        ));
    }
    let v = analyze(&tree("prose", &files));
    assert!(
        row_for(&v, "md").is_none(),
        "`.md` holds no symbol, import or io fact to lose — at the exact proportions that make \
         `.xml` a row: {}",
        gaps(&v)
    );
}

/// Every row must carry the closed vocabulary, and only the two tokens the filter can produce. A row
/// labelled `no-facts-to-lose` would mean the filter and the labeller disagree about the same
/// extension.
#[test]
fn every_row_carries_one_of_the_two_reportable_kinds() {
    let mut files = resolving_ts_pair();
    for i in 0..10 {
        files.push((
            Box::leak(format!("mapper/m{i}.xml").into_boxed_str()),
            bulky("xml", i),
        ));
        files.push((
            Box::leak(format!("app/c{i}.vue").into_boxed_str()),
            bulky("vue", i),
        ));
    }
    let v = analyze(&tree("kinds", &files));
    assert!(
        rows(&v).len() >= 2,
        "this fixture must produce a source row AND a data-config row: {}",
        gaps(&v)
    );
    let kinds: Vec<&str> = rows(&v).iter().filter_map(|r| r["kind"].as_str()).collect();
    assert_eq!(
        kinds.len(),
        rows(&v).len(),
        "every row must carry a kind field: {}",
        gaps(&v)
    );
    for kind in &kinds {
        assert!(
            *kind == "source" || *kind == "data-config",
            "a row this filter emitted cannot be {kind:?} — that token names what it excludes"
        );
    }
    assert!(
        kinds.contains(&"source"),
        "`.vue` is a source row: {kinds:?}"
    );
    assert!(
        kinds.contains(&"data-config"),
        "`.xml` is a data-config row: {kinds:?}"
    );
}

/// THE GOGS-`js` SHAPE: an extension that DID parse — it has symbols, it declared imports — and still
/// contributes zero resolved edges. Nothing in the analyze reply carries this today at any grain:
/// `coverage.declaredImportsByExt` holds the declared side and `resolvedImportEdges` the tree-wide
/// resolved side, and neither says which extension lost.
#[test]
fn a_parsed_extension_whose_imports_never_resolved_is_named() {
    let files: Vec<(&str, String)> = (0..6)
        .map(|i| {
            (
                Box::leak(format!("src/m{i}.ts").into_boxed_str()) as &str,
                ts_body("import lodash from 'lodash';\nexport const use = lodash;\n"),
            )
        })
        .collect();
    let v = analyze(&tree("unresolved", &files));
    let row = row_for(&v, "ts").unwrap_or_else(|| {
        panic!(
            "every .ts file here declares an import that resolves to nothing: {}",
            gaps(&v)
        )
    });
    assert_eq!(row["files"], 6, "row must carry the walked count: {row}");
    assert_eq!(
        row["structural"], 6,
        "these files DID parse — that is what separates this row from the no-parser kind: {row}"
    );
}

/// The zero-proof half, and the reason the field is present-always rather than present-when-wrong: on a
/// tree whose dependency graph really does contain its code, the reply must still say so — an EMPTY
/// list plus a basis sentence naming what was crossed. Without this pin, "no gap" and "never looked"
/// are the same bytes.
#[test]
fn a_fully_covered_tree_reports_an_empty_list_and_still_says_what_was_crossed() {
    let v = analyze(&tree("clean", &resolving_ts_pair()));
    assert!(
        rows(&v).is_empty(),
        "this tree's only source extension resolves its own imports: {}",
        gaps(&v)
    );
    let basis = gaps(&v)["basis"].as_str().unwrap_or_else(|| {
        panic!(
            "an empty list without a basis reads as 'never crossed': {}",
            gaps(&v)
        )
    });
    assert!(
        basis.contains("extension(s) crossed") && basis.contains("resolved import edge"),
        "the basis must state the cross and the graph population it measured: {basis:?}"
    );
    assert!(
        !gaps(&v)["meaning"].as_str().unwrap_or_default().is_empty(),
        "the vocabulary rides inside the object it describes: {}",
        gaps(&v)
    );
}

/// An asset-heavy tree is the false-positive case this filter exists to survive: images are a large
/// share of the FILE count and a tiny share of the LINE count, so a filter reading either share alone
/// would open every reply with a row about `.png`. Measured on gogs, where `.png` is 33.9% of files and
/// 6.6% of lines.
#[test]
fn an_asset_extension_that_dominates_the_file_count_is_not_a_row() {
    // gogs' proportions, amplified: the images are most of the FILES and a rounding error of the
    // lines, because the source files are where the lines are.
    let long: String = (0..400)
        .map(|i| format!("// module line {i}, the kind of bulk real source carries\n"))
        .collect();
    let mut files = vec![
        ("src/a.ts", format!("export const a = 1;\n{long}")),
        (
            "src/b.ts",
            format!("import {{ a }} from './a';\nexport const b = a + 1;\n{long}"),
        ),
    ];
    for i in 0..20 {
        files.push((
            Box::leak(format!("public/img{i}.png").into_boxed_str()),
            "binary-ish\n".to_string(),
        ));
    }
    let v = analyze(&tree("assets", &files));
    assert!(
        row_for(&v, "png").is_none(),
        "`.png` is 20 of 23 files and a rounding error of the lines: {}",
        gaps(&v)
    );
}

/// The SEAL on this module's own extension bucketing. `zzop coverage`'s per-extension table is the
/// surface that already answers "which extension is which", and its grain is pinned byte-for-byte
/// against the engine census's. This crate cannot call either one's private `ext_of`, so the relation is
/// sealed the way this crate seals every other cross-crate literal (see `Cargo.toml`'s dev-dependency
/// notes): every row emitted here must appear in the coverage query's table with the SAME `files` and
/// `structural` counts — over a tree carrying the three names that break a naive split (no dot at all,
/// an upper-case extension, a double extension).
#[test]
fn every_row_matches_the_coverage_querys_own_table_for_that_extension() {
    let mut files = resolving_ts_pair();
    files.push(("Makefile", "all:\n\techo hi\n".to_string()));
    files.push(("scripts/tool.MJS", ts_body("import x from 'pkg';\n")));
    files.push(("dist/bundle.tar.gz", "not really an archive\n".to_string()));
    for i in 0..10 {
        let body: String = (0..30)
            .map(|n| format!("<!-- component {i} line {n} -->\n"))
            .collect();
        files.push((
            Box::leak(format!("app/c{i}.vue").into_boxed_str()),
            format!("<template>\n{body}</template>\n"),
        ));
    }
    let dir = tree("seal", &files);
    let v = analyze(&dir);
    let coverage: serde_json::Value = serde_json::from_str(
        &crate::coverage_summary(&[dir.to_str().unwrap().to_string()], None)
            .expect("the coverage query must run over the same tree"),
    )
    .expect("the coverage reply must be valid JSON");
    let table = coverage["trees"][0]["extensions"]
        .as_array()
        .expect("the coverage query must carry a per-extension table");
    assert!(
        !rows(&v).is_empty(),
        "the seal proves nothing over an empty row set: {}",
        gaps(&v)
    );
    for row in rows(&v) {
        let ext = row["ext"].as_str().expect("every row names an extension");
        let theirs = table
            .iter()
            .find(|r| r["ext"] == row["ext"])
            .unwrap_or_else(|| panic!("`{ext}` is not an extension the coverage query recognizes — the two bucketings have drifted: {table:?}"));
        assert_eq!(
            row["files"], theirs["files"],
            "`{ext}` file count disagrees with the coverage query"
        );
        assert_eq!(
            row["structural"], theirs["structural"],
            "`{ext}` structural count disagrees with the coverage query"
        );
        assert_eq!(
            theirs["inDepGraph"], 0,
            "`{ext}` is a row here, so the coverage query must agree it contributes no resolved edge"
        );
    }
}

/// Determinism is a shipped contract: the row order is the extension order, ascending, and two runs of
/// the same tree serialize the same bytes.
#[test]
fn rows_are_extension_sorted_and_byte_stable_across_runs() {
    let mut files = resolving_ts_pair();
    for i in 0..10 {
        let body: String = (0..30)
            .map(|n| format!("<!-- component {i} line {n} -->\n"))
            .collect();
        files.push((
            Box::leak(format!("app/c{i}.vue").into_boxed_str()),
            format!("<template>\n{body}</template>\n"),
        ));
        files.push((
            Box::leak(format!("app/s{i}.svelte").into_boxed_str()),
            format!("<script>\n{body}</script>\n"),
        ));
    }
    let dir = tree("order", &files);
    let first = analyze(&dir);
    let second = analyze(&dir);
    let names: Vec<&str> = rows(&first)
        .iter()
        .filter_map(|r| r["ext"].as_str())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "rows must be extension-ascending: {names:?}");
    assert!(
        names.len() >= 2,
        "this fixture must produce >= 2 rows: {names:?}"
    );
    assert_eq!(
        gaps(&first).to_string(),
        gaps(&second).to_string(),
        "two runs of one tree must serialize identical bytes"
    );
}

/// Bulk body for a JSON manifest, in a shape that carries real lines.
fn json_object(lines: usize) -> String {
    let body: String = (0..lines)
        .map(|n| format!("    \"key{n}\": \"value {n}\",\n"))
        .collect();
    format!("{{\n{body}    \"last\": true\n}}\n")
}

/// CONCENTRATION IS NOT A VERDICT — the regression pin for a leg tried and reverted on 2026-08-20.
/// A largest-file exclusion was added to the line leg to kill the `package-lock.json` shape
/// (`corpus/oss/be-express` `.json`: 13 files, 10,229 lines, the lock alone 96.8%). The adversarial
/// pass then measured what it ALSO killed: two trees with the same unread payload — 10 MyBatis
/// mappers, ~2,990 lines of interpolated SQL, 20 `.java` files beside them — answered differently
/// purely on whether the XML sat in one file or ten, while `zzop coverage` kept reporting the
/// concentrated one. Two surfaces disagreeing about a whole row, on an INFERENCE, in the erasing
/// direction.
///
/// This fixture holds that pair. `.json` is 12 files whose lines are bought by ONE lockfile; `.xml`
/// is 10 files holding the same lines evenly. BOTH must be rows: this cell reports what nothing
/// read, and how a filetype distributes its lines is not evidence about that. The `kind` label is
/// what makes the lockfile row cheap to dismiss — an erased row is not.
///
/// Non-vacuity first: the coverage table is queried for `.json` itself, so a row is proven to be a
/// judgement about 12 WALKED files rather than an empty walk.
#[test]
fn a_concentrated_population_and_a_spread_one_get_the_same_answer() {
    let mut files = resolving_ts_pair();
    for i in 0..10 {
        files.push((
            Box::leak(format!("mapper/m{i}.xml").into_boxed_str()),
            bulky("xml", i),
        ));
    }
    // One generated artifact holding ~95% of the extension's lines...
    files.push(("package-lock.json", json_object(1000)));
    // ...beside the build manifests a JS/TS tree always carries, which hold almost none of them.
    for rel in [
        "package.json",
        "tsconfig.json",
        "tsconfig.spec.json",
        "tsconfig.app.json",
        ".eslintrc.json",
        "nx.json",
        "project.json",
        "e2e/project.json",
        "e2e/tsconfig.json",
        ".vscode/extensions.json",
        "jest.json",
    ] {
        files.push((rel, json_object(4)));
    }
    let dir = tree("lockfile", &files);
    let v = analyze(&dir);

    let coverage: serde_json::Value = serde_json::from_str(
        &crate::coverage_summary(&[dir.to_str().unwrap().to_string()], None)
            .expect("the coverage query must run over the same tree"),
    )
    .expect("the coverage reply must be valid JSON");
    let table = coverage["trees"][0]["extensions"]
        .as_array()
        .expect("the coverage query must carry a per-extension table");
    let json_entry = table
        .iter()
        .find(|r| r["ext"] == "json")
        .unwrap_or_else(|| panic!("the fixture's 12 .json files must be walked: {table:?}"));
    assert_eq!(
        json_entry["files"], 12,
        "the absence asserted below has to be a judgement about 12 walked files, not an empty walk"
    );
    assert_eq!(
        json_entry["structural"], 0,
        "no parser claims .json, so this extension is eligible on every leg but the line one"
    );
    assert!(
        json_entry["files"].as_u64() > table
            .iter()
            .find(|r| r["ext"] == "xml")
            .and_then(|r| r["files"].as_u64()),
        "the .json file share must EXCEED .xml's, or this fixture cannot tell the line leg from the \
         file leg: {table:?}"
    );

    let json = row_for(&v, "json").unwrap_or_else(|| {
        panic!(
            "a lockfile-concentrated population must STILL be a row — dropping it was the reverted \
             leg, whose cost was erasing a MyBatis tree whose mappers sit in one fat file: {}",
            gaps(&v)
        )
    });
    assert_eq!(
        json["kind"], "data-config",
        "the label is what makes this row cheap: {json}"
    );
    let xml = row_for(&v, "xml").unwrap_or_else(|| {
        panic!(
            "the mall row must be present for the SAME reason the json row is — both are \
             filetypes nothing read, and line distribution is not evidence about that: {}",
            gaps(&v)
        )
    });
    assert_eq!(xml["files"], 10, "row must carry the walked count: {xml}");
    assert_eq!(xml["kind"], "data-config", "{xml}");
    let meaning = gaps(&v)["meaning"].as_str().unwrap_or_default();
    assert!(
        !meaning.contains("single largest file"),
        "the reverted leg must not be described on the wire as if it still ran: {meaning:?}"
    );
}
