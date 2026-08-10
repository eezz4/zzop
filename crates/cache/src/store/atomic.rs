//! Durable single-file replacement — the one primitive every write path in [`super`] goes through.
//!
//! Split out of `store.rs` unchanged (2026-08-06). It is the only code in this module that knows
//! nothing about cache keys, entries or digests: both functions take a `&Path` and produce a file,
//! and would read the same if this crate cached something else entirely. The concurrency contract
//! below is theirs alone, which is why it moved with them.
//!
//! ## Concurrency / atomicity
//!
//! Writers (this crate expects concurrent same-process writers via `rayon`, one per file) write to a
//! uniquely-named temp file sibling to the target path, then `fs::rename` it into place. On POSIX this is
//! a well-known atomic-replace idiom. On Windows, `std::fs::rename` also replaces an existing destination
//! (it is implemented via `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`), but unlike POSIX it can fail
//! with a sharing violation if some other process/handle holds the destination open without
//! `FILE_SHARE_DELETE`. Losing that race to a concurrent writer that already produced the file is
//! therefore treated as success rather than propagated as an error (see [`write_atomic`]).
//!
//! What makes that safe is that both racers wrote an EQUIVALENT entry, not an identical one. The target
//! path is a deterministic function of the cache key, which is itself a deterministic function of file
//! content + fingerprints, so two writers landing on the same path derived their entry from the same
//! inputs and it deserializes to the same value either way; combined with rename's atomicity (a reader
//! sees one complete entry, never a blend) whichever writer wins is a correct entry for that key.
//!
//! It does NOT mean the two writers produced identical BYTES, and this doc claimed that until 2026-07-29.
//! Stored entries carry map fields typed `std::collections::HashMap`, whose serde_json object-key order
//! follows a per-instance randomized hash seed — `grep -n 'HashMap' crates/cache/src/ir_slice.rs` names
//! them (`const_map_fragment` is the standing example). Nothing here reads a stored entry byte-wise, so
//! the difference has never had a consequence; it is corrected because the benign-race argument above was
//! resting on it, and an argument resting on a false premise cannot be re-checked by whoever comes next.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Writes `bytes` to `path` via a temp-file-then-rename, so concurrent readers of `path` (this crate's
/// own `get_ir`/`get_findings`, or an external tool) never observe a partially-written file. See the
/// module doc for the Windows rename caveat this function absorbs.
pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = temp_sibling(path);
    fs::write(&tmp_path, bytes)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Every writer for this exact `path` derived its entry from the same cache key, so the
            // entries are equivalent even where their bytes differ (see module doc) — a concurrent
            // writer finishing first and leaving `path` in place is a benign race, not a failure.
            // Clean up our now-redundant temp file and report success; only propagate the error if
            // `path` genuinely never got written (a real I/O problem).
            let _ = fs::remove_file(&tmp_path);
            if path.exists() {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// A sibling path of `path` guaranteed unique within this process (pid + monotonic counter + wall-clock
/// nanos) so concurrent `rayon` writers targeting the same eventual `path` never step on each other's
/// temp file.
fn temp_sibling(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut tmp = path.to_path_buf();
    tmp.set_extension(format!("tmp-{pid}-{nanos}-{n}"));
    tmp
}
