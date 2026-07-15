//! Test-only helper shared by `mapper.rs`'s, `workspaces.rs`'s, and `lib.rs`'s unit tests: a
//! self-cleaning temp directory, the same pattern `crates/facade/src/lib.rs`'s own test module
//! uses. Kept as a standalone module (rather than duplicated in each test module) since all three
//! files' tests need it. Declared `#[cfg(test)]` at the `mod` site in `lib.rs`, so this file only
//! compiles for tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(prefix: &str) -> Self {
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

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `content` to `rel` (a path relative to this temp dir), creating any parent
    /// directories it needs.
    pub fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }

    /// Creates the directory `rel` (relative to this temp dir), parents included — for tests that
    /// need a directory with no files in it (e.g. a workspace-glob match without a `package.json`).
    pub fn mkdir(&self, rel: &str) {
        fs::create_dir_all(self.0.join(rel)).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
