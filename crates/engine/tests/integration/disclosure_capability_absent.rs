//! The `capability-absent-vs-empty` blindness class, measured instead of asserted.
//!
//! 🔴 That class is labelled `asserted` — the strongest label the registry has — and until now nothing
//! ran it. `disclosure/tests.rs` says so in its own words: its two checks are "the sliver of the gap
//! this file can close from inside the registry", and "neither measures FIRING; that needs a fixture
//! tree shaped like the class, one per class, and is not built". The backlog carried the same gap with
//! a sharper edge: an `asserted` class that has drifted is worse than a `partial` one, because a
//! disclosure registry is the canonical answer to *what this tool cannot see* — a stale row there
//! claims sight the tool does not have.
//!
//! This is that fixture, for that one class. It does not close the general question (are the OTHER
//! `asserted` labels still true), and the general question is not closeable by one test: each class
//! needs a run shaped like its own claim.
//!
//! Since 2026-09-11 every `asserted` row has one. The registry row for this class names this file and
//! this test in a `FIRING PIN:` line, and
//! `disclosure::tests::every_asserted_row_names_a_firing_pin_that_exists_and_names_it_back` refuses a
//! pin whose target does not exist, does not hold the named `fn`, or does not name its class id back —
//! which is why `capability-absent-vs-empty` is spelled out above rather than only described.
//!
//! ## What the class claims, and therefore what is checked
//! Three capabilities emit a self-report when they did NOT run, so `0 findings` can never be confused
//! with `never ran`. The run below withholds all three at once, which is the shape that would expose a
//! channel that only appears when something else is present:
//!
//! * **git history** — a warning naming the OMITTED option, and distinct from the collection-failed
//!   warning, so "never asked" and "asked, failed" are told apart by which string is present.
//! * **DSL rule packs** — `packsLoaded` present, plus a no-packs warning.
//! * **native analyses** — `nativeAnalyses` present unconditionally, which is the half added on
//!   2026-08-28 after having no channel at all.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_capability_the_class_names_still_reports_itself_when_it_did_not_run() {
    let dir = TempDir::new("zzop-capability-absent");
    fs::write(
        dir.path().join("a.ts"),
        "export function a(): number { return 1; }\n",
    )
    .unwrap();

    // All three capabilities withheld in one run: no git options, no packs.
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        git: None,
        packs: Vec::new(),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);

    // Non-vacuous: the tree really was analyzed, so an empty reply cannot pass this by accident.
    assert!(
        out.nodes.iter().any(|n| n.path == "a.ts"),
        "fixture file must have been analyzed: {:?}",
        out.nodes.iter().map(|n| &n.path).collect::<Vec<_>>()
    );

    // 1. git — the OMITTED-option string, not the collection-failed one. Both halves are checked
    //    because the class's whole claim is that a consumer can tell the two apart.
    let git_absent = out
        .warnings
        .iter()
        .any(|w| w.contains("git history not requested"));
    assert!(
        git_absent,
        "git was never requested and the run said nothing about it — `0 findings` and `never ran` \
         are now indistinguishable, which is exactly what this class asserts cannot happen. \
         warnings: {:?}",
        out.warnings
    );

    // 2. DSL packs — the roster is present even at zero, plus a warning that says so.
    assert!(
        out.packs_loaded.is_empty(),
        "the fixture must load no packs, or this half of the test is measuring the wrong run"
    );
    let packs_absent = out
        .warnings
        .iter()
        .any(|w| w.contains("pack") || w.contains("packs"));
    assert!(
        packs_absent,
        "no DSL pack was loaded and no warning said so. warnings: {:?}",
        out.warnings
    );

    // 3. native analyses — present unconditionally. This is the half that had NO channel until
    //    2026-08-28, which is how the class's own enumeration went stale in the first place.
    assert!(
        out.native_analyses.registered > 0,
        "the native roster must ship its count unconditionally — an absent roster is the silence \
         this class was rewritten to cover: {:?}",
        out.native_analyses
    );
}
