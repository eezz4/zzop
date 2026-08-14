//! `parsers.globOverrides` — what the DISCLOSURES say once the author has declared a route.
//!
//! The knob's own routing is pinned at the config seam (`crates/facade/tests/parser_glob_overrides.rs`,
//! and `dispatch::tests` for the matcher). What is pinned HERE is the engine-side reporting around a
//! declared route, because that is where the reports and the published examples can drift apart:
//!
//! * A declared route onto a NON-SOURCE extension (`dispatch::NON_SOURCE_EXTENSIONS`, e.g. `.txt`) is
//!   honored end to end. `site/reference.html`'s first example for this knob is exactly this case ("a
//!   `.txt` holding SQL"), and the non-source list is one file-walk shortcut away from silently
//!   swallowing it — the list gates the "bring an adapter" disclosure ONLY, never dispatch. The
//!   disclosure staying silent about that `.txt` is then correct rather than a gap: a parser DID claim
//!   the file, so there is no coverage hole to name.
//! * A declared route onto an EXTENSIONLESS file is honored too. The disclosure's own doc calls
//!   extensionless files "ambiguous by construction" and excludes them from the "no native parser"
//!   count; that is a statement about what zzop will GUESS, and a declared override is not a guess.
//! * The LANGUAGE census behind `uncovered_extension_warning` must count the files the run really
//!   parsed, overrides included. Keyed on the extension map alone (`dispatch_by_extension`), a route
//!   the author declared onto an unknown extension fell into a hole between two reports: the
//!   "no native parser" disclosure skips it (a parser DID claim it) and the language census skipped it
//!   too (its extension is not in the table) — so a tree that is 90% house-extension files, natively
//!   parsed, targeted by no DSL rule at all, was named by neither.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, DispatchConfig, EngineConfig, Language};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The published example, verbatim: a `.txt` that holds SQL DDL.
const DDL: &str = "CREATE TABLE users (\n  id BIGINT PRIMARY KEY\n);\n";

fn config(overrides: Vec<(String, Language)>) -> EngineConfig {
    EngineConfig {
        source_id: "glob-override-disclosure".to_string(),
        dispatch: DispatchConfig {
            glob_overrides: overrides,
            ..DispatchConfig::default()
        },
        ..EngineConfig::default()
    }
}

fn db_table_files(out: &zzop_engine::AnalyzeOutput) -> Vec<String> {
    out.ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            io.provides
                .iter()
                .filter(|p| p.kind == "db-table")
                .map(|p| format!("{}@{}", p.key, p.file))
                .collect()
        })
        .unwrap_or_default()
}

/// `site/reference.html`'s FIRST example for this knob. The extension is on the deliberately-silent
/// non-source list, which is why this needs a pin at all: that list must keep gating the disclosure
/// only, never the routing.
#[test]
fn a_declared_route_onto_a_non_source_extension_is_parsed_and_reported() {
    let dir = TempDir::new("zzop-engine-override-txt");
    dir.write("db/notes.txt", DDL);
    dir.write("src/app.ts", "export const noop = () => 1;\n");

    let before = analyze_tree(dir.path(), &config(Vec::new()));
    assert!(
        db_table_files(&before).is_empty(),
        ".txt is not SQL until the config says it is: {:?}",
        db_table_files(&before)
    );
    assert_eq!(
        before.coverage.parser_dispatched, 1,
        "only the .ts file is claimed by a parser before the override"
    );

    let after = analyze_tree(
        dir.path(),
        &config(vec![("db/*.txt".to_string(), Language::Sql)]),
    );
    assert!(
        db_table_files(&after).contains(&"table:users@db/notes.txt".to_string()),
        "the declared route must reach the SQL frontend and yield its db-table fact — a non-source \
         extension is not a reason to drop a route the author declared: {:?}",
        db_table_files(&after)
    );
    assert_eq!(
        after.coverage.parser_dispatched, 2,
        "the routed .txt is parsed source in the coverage census too"
    );
    // And the "bring an adapter" disclosure stays silent about it, which is the honest reading here:
    // a parser claimed the file, so there is no coverage gap to disclose.
    assert!(
        !after.warnings.iter().any(|w| w.contains("extension .txt")),
        "a routed file has a parser — the no-native-parser disclosure must not name it: {:?}",
        after.warnings
    );
}

/// The other silence the disclosure declares on purpose: extensionless files, "ambiguous by
/// construction". That is about GUESSING a language, so a declared route must still be honored.
#[test]
fn a_declared_route_onto_an_extensionless_file_is_parsed() {
    let dir = TempDir::new("zzop-engine-override-noext");
    dir.write("db/schema", DDL);
    dir.write("src/app.ts", "export const noop = () => 1;\n");

    let out = analyze_tree(
        dir.path(),
        &config(vec![("db/schema".to_string(), Language::Sql)]),
    );
    assert!(
        db_table_files(&out).contains(&"table:users@db/schema".to_string()),
        "an extensionless file the author routed explicitly must be parsed — the disclosure's \
         'ambiguous by construction' silence is about guessing, not about refusing a declaration: {:?}",
        db_table_files(&out)
    );
}

/// A one-rule pack targeting `.ts` and nothing else — same fixture shape (and same reason for being a
/// fixture rather than the shipped packs) as `analyze_uncovered_language.rs`, whose report this
/// exercises from the routing side.
fn ts_only_pack() -> RulePackDef {
    serde_json::from_str(
        r#"{
          "id": "ts-only-probe",
          "schema_version": 1,
          "rules": [{
            "id": "ts-todo",
            "severity": "warning",
            "message": "A TODO comment.",
            "matcher": { "type": "line-scan", "file_pattern": "(?i)\\.tsx?$", "line_pattern": "TODO" }
          }]
        }"#,
    )
    .expect("the fixture pack must parse")
}

const UNCOVERED_HEAD: &str = "NO loaded DSL rule targets";

/// A tree that is 90% house-extension, routed to a real frontend by declaration. Every file is parsed,
/// no DSL rule targets any of them, and before the census consulted the overrides NOTHING said so:
/// the "no native parser" disclosure skips a routed file (correctly — it has one), and the language
/// census skipped it too (its extension is not in the extension map).
#[test]
fn the_language_census_names_an_extension_only_a_declared_route_parses() {
    let dir = TempDir::new("zzop-engine-override-census");
    for i in 0..9 {
        dir.write(
            &format!("src/m{i}.houseml"),
            "export function run() { return 1; }\n",
        );
    }
    dir.write("web/index.ts", "export function run() { return 1; }\n");

    let mut cfg = config(vec![("**/*.houseml".to_string(), Language::TypeScript)]);
    cfg.packs = vec![ts_only_pack()];
    let out = analyze_tree(dir.path(), &cfg);

    let hit = out
        .warnings
        .iter()
        .find(|w| w.starts_with(UNCOVERED_HEAD))
        .unwrap_or_else(|| {
            panic!(
                "a routed extension that no loaded rule targets must reach the language census: {:?}",
                out.warnings
            )
        });
    assert!(
        hit.contains(".houseml (9 file(s), 90% of this tree)"),
        "the report must name the routed extension with its count and share: {hit}"
    );
}

/// INVALIDATION for the test above, and the boundary it must not cross: WITHOUT the override the same
/// tree's `.houseml` files have no parser at all, so the language census must NOT name them — the
/// "no native parser" disclosure owns that case, with its own counts. The two reports stay disjoint.
#[test]
fn without_the_route_the_same_extension_belongs_to_the_no_parser_disclosure_instead() {
    let dir = TempDir::new("zzop-engine-override-census-off");
    for i in 0..9 {
        dir.write(
            &format!("src/m{i}.houseml"),
            "export function run() { return 1; }\n",
        );
    }
    dir.write("web/index.ts", "export function run() { return 1; }\n");

    let mut cfg = config(Vec::new());
    cfg.packs = vec![ts_only_pack()];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.starts_with(UNCOVERED_HEAD) && w.contains(".houseml")),
        "an unparsed extension has no DSL coverage to lose — the language census must not claim it: {:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("9 file(s) with extension .houseml have no native parser")),
        "the no-native-parser disclosure owns the unrouted case: {:?}",
        out.warnings
    );
}
