//! One self-cleaning temp directory for every module of this test target.
//!
//! # Why this is one file and not 78
//! It was 78: the same 33 lines, byte-identical, in 78 of this target's 86 modules — 2,574 lines
//! that `crates/test-support/` exists precisely to prevent (its own module doc records folding 33
//! hand-written `eprintln!` calls across six files for the same reason), and that nothing noticed
//! because no guard looks for an identical span repeated across files in one crate.
//!
//! ## The rationale that kept it, and why it argued the other way
//! `main.rs` defended the duplication: *"Every module carries its own `TempDir` copy whose
//! uniquifying counter is a per-module static; `std::process::id()` no longer separates two modules
//! using the same prefix."* That is an argument FOR folding. With 78 independent counters, two
//! modules that pick the same prefix each start at 0 and can collide; with one counter they cannot,
//! because `fetch_add` is unique for the life of the process. The doc even records that the hazard
//! already fired once ("the one cross-module prefix collision was renamed in the guard commit") —
//! a rename is a fix for one case, not for the shape.
//!
//! Measured before the fold: 514 distinct prefixes, 0 colliding across files. So nothing was broken;
//! what was true is that the design could only stay correct by hand.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A self-cleaning temp directory — a std-only `mkdtemp` equivalent.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(prefix: &str) -> Self {
        // ONE counter for the whole target, which is what makes the name unique rather than merely
        // unlikely: `nanos` can repeat across threads and `process::id()` is shared by every module.
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
