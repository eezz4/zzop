//! A run must not analyze its own cache output on the next run — measured as BEHAVIOUR, by running the
//! same tree twice and asserting the walked file count does not move.
//!
//! **The class of guard this file exists to replace**: *a guard that seals a protection by asserting a
//! constant exists, while leaving the wiring that consumes the constant unsealed.* Self-scan pollution had
//! two such guards before this file, both green while guarding nothing:
//! - `zzop_cache`'s `the_default_cache_dir_lives_under_the_tool_dir` compares one constant against another
//!   (`.zzop/cache` starts with `.zzop/`). Nothing downstream has to keep honoring either name for it to
//!   pass.
//! - `analyze_gitignore.rs`'s `the_default_cache_dir_is_not_rescanned_as_source_...` builds a
//!   `DispatchConfig::default()`, so it only ever proves that the DEFAULT skip list still contains the
//!   name. A caller that replaces that list wholesale (the config front-end does exactly this:
//!   `zzop-facade`'s `config::declared` assigns `dispatch.skip_dirs` from the declared vocabulary, and an
//!   undeclared `vocabulary.skipDirs` is an empty list by contract) leaves both guards green while the
//!   protection is gone.
//!
//! So the fixtures here declare an EMPTY skip list on purpose: that is the shape a config-file run has when
//! its author never wrote `vocabulary.skipDirs`, and it is the shape under which the file count was
//! observed to grow 3 -> 9 -> 21 -> 45 -> 93 -> 189 across six runs of a two-file project.
//!
//! **Two independent exclusions are under test here, and the second was added 2026-07-29:**
//! 1. this run's own `cacheDir`, pruned by RESOLVED DIRECTORY — covers a cache parked anywhere, follows a
//!    relocated one, and excludes nothing when it points outside the analyzed root.
//! 2. the RESERVED NAMESPACE `.zzop`, pruned by name wherever it appears and whatever the config says.
//!
//! (2) exists because (1) is narrower than "zzop never walks its own output" — it needs a directory to
//! name, so it was unarmed with caching off and after a `cacheDir` move. Adding it partially reverses the
//! 2026-07-28 judgement that what must be excluded is "not the NAME `.zzop` but the one `cacheDir` this run
//! writes to"; `a_zzop_dir_anywhere_is_excluded_even_when_it_is_not_this_runs_cache_dir` used to assert the
//! opposite and is inverted rather than deleted, so the trade stays visible in the test that paid for it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

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

/// A two-file project — the same shape the growth was measured on.
fn fixture() -> TempDir {
    let dir = TempDir::new("zzop-self-output");
    dir.write("src/a.ts", "export function a() { return 1; }\n");
    dir.write("src/b.ts", "export function b() { return 2; }\n");
    dir
}

/// `skip_dirs` EMPTY, deliberately: a config-file run that never declared `vocabulary.skipDirs` reaches the
/// engine exactly like this (the declared list is applied whole, empty included). Nothing about the walk's
/// self-exclusion may depend on a name being present in this list.
fn config(cache_dir: Option<PathBuf>) -> EngineConfig {
    EngineConfig {
        source_id: "self-output-fixture".to_string(),
        dispatch: DispatchConfig {
            glob_overrides: Vec::new(),
            skip_dirs: Vec::new(),
        },
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs: Vec::new(),
        cache_dir,
        ..EngineConfig::default()
    }
}

fn walked(out: &AnalyzeOutput) -> Vec<String> {
    let mut v: Vec<String> = out.ir.ir.loc.keys().cloned().collect();
    v.sort();
    v
}

/// The determinism contract, measured: three consecutive runs over identical sources with an identical
/// config must walk an identical file set. Before the fix the second run swallowed the first run's cache
/// entries and the count climbed every run.
#[test]
fn repeated_runs_with_caching_on_do_not_grow_the_walked_file_count() {
    let dir = fixture();
    let cache_dir = dir.path().join(".zzop").join("cache");

    let run1 = analyze_tree(dir.path(), &config(Some(cache_dir.clone())));
    let run2 = analyze_tree(dir.path(), &config(Some(cache_dir.clone())));
    let run3 = analyze_tree(dir.path(), &config(Some(cache_dir)));

    assert_eq!(
        run1.coverage.files,
        2,
        "the fixture has exactly two source files: {:?}",
        walked(&run1)
    );
    assert_eq!(
        run2.coverage.files,
        run1.coverage.files,
        "run 2 walked the cache run 1 wrote: {:?} vs {:?}",
        walked(&run1),
        walked(&run2)
    );
    assert_eq!(
        run3.coverage.files,
        run1.coverage.files,
        "run 3 walked the cache runs 1-2 wrote: {:?} vs {:?}",
        walked(&run1),
        walked(&run3)
    );
    assert_eq!(
        walked(&run1),
        walked(&run3),
        "consecutive runs over identical sources must walk an identical file set"
    );
}

/// A cache directory the author pointed OUTSIDE the analyzed tree has nothing to exclude — the walk must
/// not gain a phantom exclusion, and must not panic on a path that shares no prefix with `root`.
#[test]
fn a_cache_dir_outside_the_analyzed_root_excludes_nothing() {
    let dir = fixture();
    let elsewhere = TempDir::new("zzop-self-output-external");
    let cache_dir = elsewhere.path().join("cache");

    let run1 = analyze_tree(dir.path(), &config(Some(cache_dir.clone())));
    let run2 = analyze_tree(dir.path(), &config(Some(cache_dir)));

    assert!(
        elsewhere.path().join("cache").join("ir").is_dir(),
        "the fixture is only meaningful if caching actually ran and wrote out of tree"
    );
    assert_eq!(run1.coverage.files, 2, "{:?}", walked(&run1));
    assert_eq!(run2.coverage.files, 2, "{:?}", walked(&run2));
    assert_eq!(walked(&run1), walked(&run2));
}

/// `.zzop` is a RESERVED NAMESPACE, excluded wherever it appears and whatever the config says — the
/// 2026-07-29 decision that partially reverses 2026-07-28's "exclude the run's own `cacheDir`, never the
/// name". This test asserted the OPPOSITE until that day (it required `vendor/.zzop/other.ts` to be
/// analyzed) and is inverted here rather than deleted, because the behaviour it pinned is exactly what was
/// traded away.
///
/// What was traded: analyzing SOMEBODY ELSE'S `.zzop` output on purpose. That is now impossible without a
/// new opt-in knob, and the decision took that cost knowingly — the name cannot mean two things, and it
/// means "zzop's own derived output" everywhere else in this codebase (`DEFAULT_CACHE_DIR`, the on-disk
/// `.zzop/` vs `zzop/` split). Same standing `git` gives `.git`.
#[test]
fn a_zzop_dir_anywhere_is_excluded_even_when_it_is_not_this_runs_cache_dir() {
    let dir = fixture();
    dir.write(
        "vendor/.zzop/other.ts",
        "export function other() { return 3; }\n",
    );
    let cache_dir = dir.path().join(".zzop").join("cache");

    let run1 = analyze_tree(dir.path(), &config(Some(cache_dir.clone())));
    let run2 = analyze_tree(dir.path(), &config(Some(cache_dir)));

    assert!(
        !walked(&run1).contains(&"vendor/.zzop/other.ts".to_string()),
        ".zzop is zzop-owned wherever it sits, not only at this run's cacheDir: {:?}",
        walked(&run1)
    );
    assert_eq!(run1.coverage.files, 2, "{:?}", walked(&run1));
    assert_eq!(
        run2.coverage.files,
        run1.coverage.files,
        "{:?} vs {:?}",
        walked(&run1),
        walked(&run2)
    );
}

/// DISARM CASE 1 — caching OFF (`cacheDir: null`). There is no run-owned directory to prune, so the
/// `cacheDir`-based exclusion has nothing to match and a `.zzop` left by an EARLIER cached run gets walked
/// as source. Measured on a one-file tree the day this was found: default cacheDir gave `files=2` twice,
/// then switching to `cacheDir: null` gave `files=7` — five cache artifacts read as source.
#[test]
fn a_run_with_caching_off_still_excludes_a_zzop_left_by_an_earlier_run() {
    let dir = fixture();
    let cache_dir = dir.path().join(".zzop").join("cache");

    let cached = analyze_tree(dir.path(), &config(Some(cache_dir)));
    assert_eq!(cached.coverage.files, 2, "{:?}", walked(&cached));
    assert!(
        dir.path().join(".zzop").join("cache").is_dir(),
        "the fixture is only meaningful if the first run actually wrote a cache"
    );

    let uncached = analyze_tree(dir.path(), &config(None));
    assert_eq!(
        uncached.coverage.files,
        2,
        "caching off must not turn the previous run's cache into source: {:?}",
        walked(&uncached)
    );
    assert_eq!(walked(&cached), walked(&uncached));
}

/// DISARM CASE 2 — `cacheDir` MOVED from A to B. The run prunes B, which it writes; A's leftovers are not
/// this run's output and so were walked as source. Both directories are `.zzop`, and the reserved
/// namespace covers both.
#[test]
fn moving_the_cache_dir_does_not_turn_the_old_ones_leftovers_into_source() {
    let dir = fixture();
    let dir_a = dir.path().join(".zzop").join("cache");
    let dir_b = dir.path().join(".zzop").join("cache-b");

    let run_a = analyze_tree(dir.path(), &config(Some(dir_a)));
    assert_eq!(run_a.coverage.files, 2, "{:?}", walked(&run_a));

    let run_b = analyze_tree(dir.path(), &config(Some(dir_b)));
    assert_eq!(
        run_b.coverage.files,
        2,
        "the previous cacheDir's leftovers are still zzop output: {:?}",
        walked(&run_b)
    );
    assert_eq!(walked(&run_a), walked(&run_b));
}

/// The user-authored sibling `zzop/` (no dot — custom rule packs, adapter overlays) is NOT reserved: that
/// is source a human wrote and wants analyzed. The reservation is exactly one name, and this pins the
/// boundary so a future "prune anything starting with zzop" never quietly eats it.
#[test]
fn the_dotless_zzop_dir_is_user_source_and_stays_analyzed() {
    let dir = fixture();
    dir.write("zzop/packs/custom.ts", "export const pack = 1;\n");
    let out = analyze_tree(dir.path(), &config(None));
    assert!(
        walked(&out).contains(&"zzop/packs/custom.ts".to_string()),
        "`zzop/` without the dot is user-authored source: {:?}",
        walked(&out)
    );
}

/// The relative-vs-absolute form of the same directory must exclude the same thing: the engine receives
/// whatever the front-end resolved, and a relative `cacheDir` is resolved against the process CWD, not
/// against `root` — so the comparison has to be made on canonicalized paths rather than on strings.
#[test]
fn a_cache_dir_given_in_a_non_canonical_form_still_excludes_itself() {
    let dir = fixture();
    // `<root>/src/../.zzop/cache` — the same directory as `<root>/.zzop/cache`, spelled with a `..`.
    let cache_dir = dir
        .path()
        .join("src")
        .join("..")
        .join(".zzop")
        .join("cache");

    let run1 = analyze_tree(dir.path(), &config(Some(cache_dir.clone())));
    let run2 = analyze_tree(dir.path(), &config(Some(cache_dir)));

    assert_eq!(run1.coverage.files, 2, "{:?}", walked(&run1));
    assert_eq!(
        run2.coverage.files,
        run1.coverage.files,
        "a `..`-spelled cache dir is the same directory and must be excluded too: {:?} vs {:?}",
        walked(&run1),
        walked(&run2)
    );
}
