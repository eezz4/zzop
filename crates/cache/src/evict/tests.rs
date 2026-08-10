use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use super::*;

/// Writes `count` files of `size` bytes each, oldest first, with mtimes far enough apart that the
/// filesystem's timestamp granularity cannot reorder them. Returns them in write order.
fn seed(dir: &PathBuf, count: usize, size: usize) -> Vec<PathBuf> {
    fs::create_dir_all(dir).unwrap();
    let mut out = Vec::new();
    for i in 0..count {
        let p = dir.join(format!("e{i:04}.json"));
        fs::write(&p, vec![b'x'; size]).unwrap();
        // Windows' FAT-era granularity is 2s on some volumes; NTFS is 100ns. 20ms is ample for NTFS
        // and keeps the suite fast. The assertions below never depend on a specific ORDER within a
        // tick, only on "the oldest go first".
        std::thread::sleep(Duration::from_millis(20));
        out.push(p);
    }
    out
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "zzop-evict-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_cache_under_budget_is_left_completely_alone() {
    let root = scratch("under");
    let dir = root.join("ir");
    let files = seed(&dir, 5, 100);
    let deleted = evict_to_cap(&entry_dirs(&root, &["ir"]), 10_000);
    assert_eq!(deleted, 0);
    for f in &files {
        assert!(f.exists(), "{f:?} must survive");
    }
}

#[test]
fn going_over_budget_deletes_oldest_written_first_and_stops_at_the_target() {
    let root = scratch("over");
    let dir = root.join("ir");
    // 10 files x 100 bytes = 1000. Cap 500 -> target is 3/4 of 500 = 375, so it must delete until
    // 375 or less remains: 7 deletions (300 left), not 5 (500 left, which merely meets the cap).
    let files = seed(&dir, 10, 100);
    let deleted = evict_to_cap(&entry_dirs(&root, &["ir"]), 500);
    assert_eq!(
        deleted, 7,
        "must reclaim to the target, not merely to the cap"
    );
    for f in files.iter().take(7) {
        assert!(!f.exists(), "oldest {f:?} must be gone");
    }
    for f in files.iter().skip(7) {
        assert!(f.exists(), "newest {f:?} must survive");
    }
}

#[test]
fn both_entry_kinds_share_one_budget() {
    let root = scratch("both");
    // The cap is on the CACHE, not per directory — an `ir` entry and a `findings` entry cost the same
    // disk, so they compete in one pool rather than each getting the full budget.
    seed(&root.join("ir"), 5, 100);
    seed(&root.join("findings"), 5, 100);
    let deleted = evict_to_cap(&entry_dirs(&root, &["ir", "findings"]), 500);
    assert!(
        deleted > 0,
        "1000 bytes across two dirs must exceed a 500 cap"
    );
    let remaining: u64 = ["ir", "findings"]
        .iter()
        .flat_map(|s| fs::read_dir(root.join(s)).unwrap())
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    assert!(
        remaining <= 375,
        "must be at or under the target, got {remaining}"
    );
}

#[test]
fn a_missing_directory_is_not_an_error() {
    // First run on a fresh machine: neither directory exists yet. Housekeeping must never be the thing
    // that fails an analysis.
    let root = scratch("missing");
    assert_eq!(evict_to_cap(&entry_dirs(&root, &["ir", "findings"]), 1), 0);
}

#[test]
fn a_subdirectory_is_never_deleted() {
    // Only files are entries. Anything else in there belongs to someone else and is left alone even
    // when the cache is over budget.
    let root = scratch("subdir");
    let dir = root.join("ir");
    seed(&dir, 10, 100);
    fs::create_dir_all(dir.join("not-an-entry")).unwrap();
    evict_to_cap(&entry_dirs(&root, &["ir"]), 100);
    assert!(
        dir.join("not-an-entry").is_dir(),
        "a directory must survive"
    );
}
