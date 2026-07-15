//! End-to-end coverage for the "bring an adapter" per-extension disclosure
//! (`zzop_engine::analyze::diagnostics::unparsed_extension_warning`, wired into `analyze::assemble`): a
//! file whose extension `dispatch::dispatch` never routes to a `Language` (no native parser frontend) used
//! to vanish from every self-report — `degraded: false`, no `io`/symbols, its extension recorded nowhere.
//! This now surfaces as one `AnalyzeOutput::warnings` line per distinct extension, naming a count and a
//! path sample, EXCLUDING non-source extensions (docs/data/styles/images/fonts/media/binaries — see
//! `dispatch::NON_SOURCE_EXTENSIONS`) and extensionless files (README, Dockerfile — no reliable language
//! signal), and EXCLUDING any file an adapter overlay already covers WITH A REAL EXTRACTED FACT
//! (`envelope::overlay_file_carries_facts`) — a zero-fact overlay entry (every channel empty, `is_entry:
//! false`) does NOT exempt its file: see `zero_fact_overlay_does_not_suppress_the_unparsed_warning` below,
//! the G8 "unmask" regression guard.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{FileProjection, IoFacts, IoProvide, NormalizedEnvelope, NORMALIZED_AST_FORMAT};
use zzop_engine::{analyze_tree, EngineConfig};

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

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "unparsed-ext-fixture".to_string(),
        ..EngineConfig::default()
    }
}

/// The shared fixture: two `.sql` files, one `.py` file (all dispatch-`None`, real source signal), a
/// `README.md` (non-source extension), an `img.png` (non-source extension), and a native `src/x.ts` (has a
/// real parser). Only `.sql`/`.py` are expected to ever appear in the per-extension warning.
fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-unparsed-ext");
    dir.write("a.sql", "SELECT * FROM users;\n");
    dir.write("b.sql", "SELECT * FROM orders;\n");
    dir.write("c.py", "def handler():\n    return 1\n");
    dir.write("README.md", "# not real source\n");
    dir.write("img.png", "not a real png, just bytes\n");
    dir.write("src/x.ts", "export const x = 1;\n");
    dir
}

/// A minimal, all-empty `FileProjection` — same defaults `analyze_adapter_overlay.rs`'s own `projection()`
/// helper uses. ZERO-FACT by construction (every channel empty, `is_entry: false`) — deliberately used by
/// `zero_fact_overlay_does_not_suppress_the_unparsed_warning` below; every OTHER overlay-exclusion test
/// uses `projection_with_io` instead so it keeps testing real coverage, not the zero-fact case.
fn projection(path: &str) -> FileProjection {
    FileProjection {
        class_shape_fragments: Vec::new(),
        path: path.to_string(),
        loc: 1,
        symbols: Vec::new(),
        imports: zzop_core::ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
        is_entry: false,
        attributes: Vec::new(),
        loop_spans: Vec::new(),
    }
}

/// Same as `projection` but with one real `io` provide fact — a FACT-CARRYING projection
/// (`envelope::overlay_file_carries_facts` returns `true`), so an overlay built from this keeps exempting
/// its file from the unparsed-extension disclosure under the new (post-G8) rule.
fn projection_with_io(path: &str) -> FileProjection {
    let mut p = projection(path);
    p.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: format!("GET /{path}"),
        file: path.to_string(),
        line: 1,
        symbol: None,
    });
    p
}

fn overlay(parser: &str, files: Vec<FileProjection>) -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: parser.to_string(),
        source: "adapter".to_string(),
        files,
    }
}

#[test]
fn unparsed_source_extensions_warn_exactly_once_each_non_source_and_native_extensions_stay_silent()
{
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());

    let py_line = out
        .warnings
        .iter()
        .find(|w| w.contains("extension .py"))
        .unwrap_or_else(|| panic!("expected a .py warning, got: {:?}", out.warnings));
    assert!(
        py_line.starts_with("1 file(s) with extension .py"),
        "{py_line}"
    );
    assert!(py_line.contains("c.py"), "{py_line}");

    let sql_line = out
        .warnings
        .iter()
        .find(|w| w.contains("extension .sql"))
        .unwrap_or_else(|| panic!("expected a .sql warning, got: {:?}", out.warnings));
    assert!(
        sql_line.starts_with("2 file(s) with extension .sql"),
        "{sql_line}"
    );
    assert!(sql_line.contains("a.sql"), "{sql_line}");
    assert!(sql_line.contains("b.sql"), "{sql_line}");

    // Exactly one line per extension.
    assert_eq!(
        out.warnings
            .iter()
            .filter(|w| w.contains("extension .py"))
            .count(),
        1
    );
    assert_eq!(
        out.warnings
            .iter()
            .filter(|w| w.contains("extension .sql"))
            .count(),
        1
    );

    // BTreeMap key order: "py" sorts before "sql".
    let py_idx = out
        .warnings
        .iter()
        .position(|w| w.contains("extension .py"))
        .unwrap();
    let sql_idx = out
        .warnings
        .iter()
        .position(|w| w.contains("extension .sql"))
        .unwrap();
    assert!(
        py_idx < sql_idx,
        "expected .py before .sql: {:?}",
        out.warnings
    );

    // Non-source extensions and natively-dispatched extensions never appear.
    assert!(
        !out.warnings.iter().any(|w| w.contains("extension .md")),
        "{:?}",
        out.warnings
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains("extension .png")),
        "{:?}",
        out.warnings
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains("extension .ts")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn adapter_overlay_coverage_excludes_a_file_from_its_extension_count() {
    let dir = fixture_tree();

    let mut cfg = config();
    // The overlay's own `path` matches the native artifact's `rel` exactly, so this hits the
    // merge-onto-existing-artifact branch of `apply_adapter_overlays` — `a.sql` is now "covered" by an
    // adapter, and must drop out of the `.sql` count entirely. Uses `projection_with_io` (a real fact),
    // not the zero-fact `projection` — see `zero_fact_overlay_does_not_suppress_the_unparsed_warning`
    // below for the case where the overlay carries no facts at all.
    cfg.adapter_overlays = vec![overlay("sql-adapter/1", vec![projection_with_io("a.sql")])];
    let out = analyze_tree(dir.path(), &cfg);

    let sql_line = out
        .warnings
        .iter()
        .find(|w| w.contains("extension .sql"))
        .unwrap_or_else(|| panic!("expected a .sql warning, got: {:?}", out.warnings));
    assert!(
        sql_line.starts_with("1 file(s) with extension .sql"),
        "{sql_line}"
    );
    assert!(!sql_line.contains("a.sql"), "{sql_line}");
    assert!(sql_line.contains("b.sql"), "{sql_line}");
}

#[test]
fn adapter_overlay_covering_the_only_file_of_an_extension_makes_that_line_disappear() {
    let dir = TempDir::new("zzop-engine-unparsed-ext-single");
    dir.write("only.sql", "SELECT 1;\n");
    dir.write("src/x.ts", "export const x = 1;\n");

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay(
        "sql-adapter/1",
        vec![projection_with_io("only.sql")],
    )];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.warnings.iter().any(|w| w.contains("extension .sql")),
        "the overlay covers the only .sql file, so the line must disappear entirely: {:?}",
        out.warnings
    );
}

#[test]
fn adapter_overlay_covering_a_path_with_no_native_file_is_excluded_via_the_synthetic_branch() {
    // Distinct from the two overlay tests above, which both name an EXISTING native artifact's own `rel`
    // (the merge-onto-existing branch of `apply_adapter_overlays`). Here the overlay's `path` matches no
    // file on disk at all, so `apply_adapter_overlays` pushes a brand-new SYNTHETIC `FileArtifact` for it
    // instead — that artifact is still `dispatch(...) == None` with an unparsed, non-non-source extension,
    // so without the overlay-exclusion check it would ALSO count toward the `.sql` total.
    let dir = TempDir::new("zzop-engine-unparsed-ext-synthetic");
    // A real, on-disk, unparsed .sql file — NOT covered by the overlay.
    dir.write("native.sql", "SELECT 1;\n");
    dir.write("src/x.ts", "export const x = 1;\n");

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay(
        "sql-adapter/1",
        vec![projection_with_io("external/legacy.sql")],
    )];
    let out = analyze_tree(dir.path(), &cfg);

    let sql_line = out
        .warnings
        .iter()
        .find(|w| w.contains("extension .sql"))
        .unwrap_or_else(|| panic!("expected a .sql warning, got: {:?}", out.warnings));
    assert!(
        sql_line.starts_with("1 file(s) with extension .sql"),
        "the synthetic overlay-covered file must not be counted: {sql_line}"
    );
    assert!(sql_line.contains("native.sql"), "{sql_line}");
    assert!(!sql_line.contains("legacy.sql"), "{sql_line}");

    // G8a — the declared path matched no file in this tree at all, so it landed via the synthetic branch;
    // that must self-report as a synthetic-entry warning too (independent of the fact-carrying rule above
    // — a path can be a typo AND still carry a real fact, as this one does).
    let synthetic_line = out
        .warnings
        .iter()
        .find(|w| w.contains("added as synthetic entries"))
        .unwrap_or_else(|| {
            panic!(
                "expected a synthetic-entry warning, got: {:?}",
                out.warnings
            )
        });
    assert!(
        synthetic_line.starts_with("adapter overlay \"adapter\" (parser sql-adapter/1): 1 of 1 declared file(s) matched no file in this tree"),
        "{synthetic_line}"
    );
    assert!(
        synthetic_line.contains("external/legacy.sql"),
        "{synthetic_line}"
    );
}

#[test]
fn zero_fact_overlay_does_not_suppress_the_unparsed_warning() {
    // The G8 "unmask" regression guard: an overlay whose entry carries NO extracted facts at all (every
    // channel empty, `is_entry: false`) used to still count as "adapter coverage" and suppress the
    // per-extension unparsed-extension disclosure for its file. It must no longer do either — the covered
    // `.sql` file keeps its unparsed-extension warning, AND the overlay itself gets a zero-fact warning.
    let dir = fixture_tree();

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("sql-adapter/1", vec![projection("a.sql")])];
    let out = analyze_tree(dir.path(), &cfg);

    // The unmask: a.sql is NOT exempted, so both .sql files (a.sql and b.sql) still count.
    let sql_line = out
        .warnings
        .iter()
        .find(|w| w.contains("extension .sql"))
        .unwrap_or_else(|| panic!("expected a .sql warning, got: {:?}", out.warnings));
    assert!(
        sql_line.starts_with("2 file(s) with extension .sql"),
        "zero-fact overlay coverage must not exempt a.sql from the disclosure: {sql_line}"
    );
    assert!(sql_line.contains("a.sql"), "{sql_line}");
    assert!(sql_line.contains("b.sql"), "{sql_line}");

    // The G8b zero-fact self-report on the overlay itself.
    let zero_fact_line = out
        .warnings
        .iter()
        .find(|w| w.contains("carries any extracted facts"))
        .unwrap_or_else(|| {
            panic!(
                "expected a zero-fact overlay warning, got: {:?}",
                out.warnings
            )
        });
    assert!(
        zero_fact_line.starts_with(
            "adapter overlay \"adapter\" (parser sql-adapter/1): none of its 1 file entry carries any extracted facts"
        ),
        "{zero_fact_line}"
    );
}

#[test]
fn oversized_unparsed_extension_file_appears_in_both_degraded_and_the_extension_warning() {
    // `dispatch::dispatch` runs purely off the path/extension, independent of file size — an OVERSIZED
    // file of an unparsed extension short-circuits to `degraded: true` in
    // `pipeline::compute_fresh_artifact`'s oversized branch BEFORE dispatch is even consulted for language
    // selection, but `dispatch(...)` itself still returns `None` for it. The two disclosures are
    // orthogonal: `degraded` states a SIZE fact, the per-extension warning states a COVERAGE fact — this
    // file legitimately belongs in both, not either/or.
    let dir = TempDir::new("zzop-engine-unparsed-ext-oversized");
    let big_sql = "SELECT * FROM users;\n".repeat(50);
    dir.write("big.sql", &big_sql);
    dir.write("src/x.ts", "export const x = 1;\n");

    let mut cfg = config();
    cfg.size_cap = 100; // big.sql is well over this; src/x.ts is not.
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        out.degraded.contains(&"big.sql".to_string()),
        "expected big.sql to be size-cap-degraded: {:?}",
        out.degraded
    );
    let sql_line = out
        .warnings
        .iter()
        .find(|w| w.contains("extension .sql"))
        .unwrap_or_else(|| {
            panic!(
                "expected a .sql warning even for the oversized file, got: {:?}",
                out.warnings
            )
        });
    assert!(
        sql_line.starts_with("1 file(s) with extension .sql"),
        "{sql_line}"
    );
    assert!(sql_line.contains("big.sql"), "{sql_line}");
}
