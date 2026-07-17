//! Host-boundary path absolutization. `zzop-config`'s mapper contract says the CALLER hands it an
//! absolute root (`crates/config/src/mapper/paths.rs`): its resolution is purely lexical, so a
//! relative argument like `.` would survive to `normalize_lexically`, which collapses `CurDir`
//! into an EMPTY path — the engine then rejects `root: ""` with a baffling "missing required
//! field" error. This module is the one seam every incoming filesystem path argument (tree roots
//! AND `--config`/`configPath` files) crosses before it reaches `zzop_config`, for the MCP tools
//! and the CLI subcommands alike — the handlers are shared, so absolutizing here fixes both.
//!
//! `std::path::absolute`, deliberately NOT `fs::canonicalize`: it resolves against the process cwd
//! without touching the filesystem — no `\\?\` UNC prefixes on Windows, no existence requirement
//! (existence stays the handlers' explicit `exists()` check / the engine's load-time problem,
//! exactly as before).

use std::path::{Path, PathBuf};

/// Absolutize a CLI/tool path argument against the process cwd; an already-absolute path passes
/// through (lexically normalized, `.` components dropped). Falls back to the raw path when
/// `std::path::absolute` errors (its only cases: an empty path, or an unobtainable cwd) — the
/// downstream existence check then reports the argument the caller actually passed.
pub(crate) fn absolutize(raw: &str) -> PathBuf {
    std::path::absolute(Path::new(raw)).unwrap_or_else(|_| PathBuf::from(raw))
}

/// Serializes every test in THIS file that READS OR MUTATES the process cwd (`set_current_dir` is
/// process-global and the test harness runs threads in parallel — a cwd-mutating test racing a
/// cwd-reading one misresolves relative paths intermittently). None of this crate's tests currently
/// mutate cwd, but the read-side tests below still lock it defensively; the `zzop-mcp` host owns an
/// independent lock of its own for its one cwd-mutating end-to-end test (a separate test binary/
/// process — a static here could not serialize against it anyway).
#[cfg(test)]
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::{absolutize, CWD_LOCK};

    #[test]
    fn dot_becomes_the_cwd_not_an_empty_path() {
        let _cwd_guard = CWD_LOCK.lock().unwrap();
        // The original bug: `.` is all CurDir components, which the config mapper's lexical
        // normalization collapses to "" — absolutized first, it is the cwd itself.
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(absolutize("."), cwd);
        assert!(absolutize(".").file_name().is_some(), "must not be empty");
    }

    #[test]
    fn a_relative_path_joins_onto_the_cwd() {
        let _cwd_guard = CWD_LOCK.lock().unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(absolutize("sub/tree"), cwd.join("sub").join("tree"));
    }

    #[test]
    fn an_absolute_path_passes_through_unchanged() {
        let abs = std::env::current_dir().unwrap().join("some-tree");
        assert_eq!(absolutize(abs.to_str().unwrap()), abs);
    }
}
