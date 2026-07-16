//! Test-only helpers shared by the `pipeline` submodules' `#[cfg(test)]` mods.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — no `tempfile` crate dependency in
/// this workspace; mirrors `rules/native/rules-schema/src/usage.rs`'s own test-local `TempDir`).
/// Created fresh per test and removed on drop.
pub(super) struct TempDir(PathBuf);

impl TempDir {
    pub(super) fn new(prefix: &str) -> Self {
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

    pub(super) fn path(&self) -> &Path {
        &self.0
    }

    pub(super) fn write(&self, rel: &str, content: &str) {
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
