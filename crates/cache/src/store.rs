//! `AnalysisCache` — the on-disk store. Layout, format, and atomicity are documented inline; see the
//! crate doc for the design this implements.
//!
//! ## Storage layout
//!
//! ```text
//! <root>/
//!   schema_version        plain UTF-8 text: the schema version string passed to `open`
//!   ir/<digest>.json      one IrEntry per (content_hash, parser_fingerprint, scope) triple seen
//!   findings/<digest>.json one FindingsEntry per (content_hash, parser_fingerprint, scope, ruleset_fingerprint)
//! ```
//!
//! `<digest>` is `hash::digest128` of the relevant key fields joined with a NUL separator — it exists
//! only to shard entries into filenames; it is never trusted on its own (see `hash.rs` and the read-path
//! key comparison below).
//!
//! ## Format
//!
//! Each entry file is JSON with a leading `format_version` field (spec: "lead with a format-version
//! marker" — so a future switch to a binary format like bincode can coexist with, or cleanly reject,
//! entries written by this version). The full cache key is duplicated inside the entry;
//! every read compares it against the requested key by exact string equality and treats a mismatch (or
//! any deserialization failure) as a miss rather than an error — see `hash.rs` for why this matters even
//! though digest collisions are already astronomically unlikely.
//!
//! ## Concurrency / atomicity
//!
//! Writers (this crate expects concurrent same-process writers via `rayon`, one per file) write to a
//! uniquely-named temp file sibling to the target path, then `fs::rename` it into place. On POSIX this is
//! a well-known atomic-replace idiom. On Windows, `std::fs::rename` also replaces an existing destination
//! (it is implemented via `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`), but unlike POSIX it can fail
//! with a sharing violation if some other process/handle holds the destination open without
//! `FILE_SHARE_DELETE`. Because every write for a given target path carries identical bytes (the path is
//! a deterministic function of the cache key, which is itself a deterministic function of file content +
//! fingerprints), losing that race to a concurrent writer that already produced the same file is treated
//! as success rather than propagated as an error (see `write_atomic`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use zzop_core::Finding;

use crate::hash::digest128;
use crate::ir_slice::FileIrSlice;
use crate::key::CacheKey;

const SCHEMA_VERSION_FILE: &str = "schema_version";
const IR_DIR: &str = "ir";
const FINDINGS_DIR: &str = "findings";
/// Leading format-version marker stored in every entry (see module doc). Bump when the JSON shape of
/// `IrEntry`/`FindingsEntry` changes in a way old readers cannot tolerate; a mismatch is treated as a
/// miss, not a crash (see `get_ir`/`get_findings`).
const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct IrEntry {
    format_version: u32,
    content_hash: String,
    parser_fingerprint: String,
    scope: String,
    ir: FileIrSlice,
}

#[derive(Serialize, Deserialize)]
struct FindingsEntry {
    format_version: u32,
    content_hash: String,
    parser_fingerprint: String,
    scope: String,
    ruleset_fingerprint: String,
    findings: Vec<Finding>,
}

/// The file-level analysis cache: per-file Common IR slices and per-file rule findings, stored as
/// separate on-disk entries. See the crate doc for the layout and format.
pub struct AnalysisCache {
    root: PathBuf,
}

impl AnalysisCache {
    /// Opens (creating if absent) the cache directory at `dir`. `schema_version` identifies the Common IR
    /// / entry-format contract this caller speaks; if the directory's stored schema version differs (or
    /// there isn't one yet), every existing entry is wiped before the new version is recorded — a bulk
    /// invalidation for "the IR contract changed", not a per-entry decision.
    pub fn open(dir: &Path, schema_version: &str) -> io::Result<AnalysisCache> {
        fs::create_dir_all(dir)?;
        let version_path = dir.join(SCHEMA_VERSION_FILE);
        let existing = fs::read_to_string(&version_path).ok();
        if existing.as_deref() != Some(schema_version) {
            wipe_entries(dir)?;
            write_atomic(&version_path, schema_version.as_bytes())?;
        }
        fs::create_dir_all(dir.join(IR_DIR))?;
        fs::create_dir_all(dir.join(FINDINGS_DIR))?;
        Ok(AnalysisCache {
            root: dir.to_path_buf(),
        })
    }

    /// Content-addressing hash of raw file bytes — the `content_hash` half of a `CacheKey`. Not
    /// cryptographic; see `hash.rs` for the collision tradeoff.
    pub fn content_hash(bytes: &[u8]) -> String {
        digest128(bytes)
    }

    /// Looks up a file's cached Common IR slice by `(content_hash, parser_fingerprint, scope)` —
    /// ruleset-independent, per the spec's IR/findings split, but NOT scope-independent: `scope`
    /// disambiguates "which file" (see `CacheKey::scope`'s doc) since a `FileIrSlice`'s `symbols`/`io`
    /// embed their own originating path. Returns `None` on a miss, a stored-key mismatch (see module
    /// doc), or any I/O / deserialization failure — this method never panics or errors on a corrupted or
    /// missing entry, it simply reports "not cached".
    pub fn get_ir(&self, key: &CacheKey) -> Option<FileIrSlice> {
        let path = self.ir_path(key);
        let bytes = fs::read(path).ok()?;
        let entry: IrEntry = serde_json::from_slice(&bytes).ok()?;
        if entry.format_version != FORMAT_VERSION
            || entry.content_hash != key.content_hash
            || entry.parser_fingerprint != key.parser_fingerprint
            || entry.scope != key.scope
        {
            return None;
        }
        Some(entry.ir)
    }

    /// Stores `ir` under `(content_hash, parser_fingerprint, scope)`, independent of
    /// `key.ruleset_fingerprint` — a later `put_ir` for the same content + parser + scope but a different
    /// ruleset overwrites the same entry (harmlessly: the IR itself does not vary with the ruleset).
    pub fn put_ir(&self, key: &CacheKey, ir: &FileIrSlice) -> io::Result<()> {
        let entry = IrEntry {
            format_version: FORMAT_VERSION,
            content_hash: key.content_hash.clone(),
            parser_fingerprint: key.parser_fingerprint.clone(),
            scope: key.scope.clone(),
            ir: ir.clone(),
        };
        let bytes = serde_json::to_vec(&entry).map_err(to_io_err)?;
        write_atomic(&self.ir_path(key), &bytes)
    }

    /// Looks up a file's cached per-file rule findings by the full `(content_hash, parser_fingerprint,
    /// scope, ruleset_fingerprint)` quadruple. Same never-panics-on-corruption contract as `get_ir`.
    pub fn get_findings(&self, key: &CacheKey) -> Option<Vec<Finding>> {
        let path = self.findings_path(key);
        let bytes = fs::read(path).ok()?;
        let entry: FindingsEntry = serde_json::from_slice(&bytes).ok()?;
        if entry.format_version != FORMAT_VERSION
            || entry.content_hash != key.content_hash
            || entry.parser_fingerprint != key.parser_fingerprint
            || entry.scope != key.scope
            || entry.ruleset_fingerprint != key.ruleset_fingerprint
        {
            return None;
        }
        Some(entry.findings)
    }

    /// Stores `findings` under the full four-field key.
    pub fn put_findings(&self, key: &CacheKey, findings: &[Finding]) -> io::Result<()> {
        let entry = FindingsEntry {
            format_version: FORMAT_VERSION,
            content_hash: key.content_hash.clone(),
            parser_fingerprint: key.parser_fingerprint.clone(),
            scope: key.scope.clone(),
            ruleset_fingerprint: key.ruleset_fingerprint.clone(),
            findings: findings.to_vec(),
        };
        let bytes = serde_json::to_vec(&entry).map_err(to_io_err)?;
        write_atomic(&self.findings_path(key), &bytes)
    }

    fn ir_path(&self, key: &CacheKey) -> PathBuf {
        let combined = format!(
            "{}\0{}\0{}",
            key.content_hash, key.parser_fingerprint, key.scope
        );
        self.root
            .join(IR_DIR)
            .join(format!("{}.json", digest128(combined.as_bytes())))
    }

    fn findings_path(&self, key: &CacheKey) -> PathBuf {
        let combined = format!(
            "{}\0{}\0{}\0{}",
            key.content_hash, key.parser_fingerprint, key.scope, key.ruleset_fingerprint
        );
        self.root
            .join(FINDINGS_DIR)
            .join(format!("{}.json", digest128(combined.as_bytes())))
    }
}

fn wipe_entries(dir: &Path) -> io::Result<()> {
    for sub in [IR_DIR, FINDINGS_DIR] {
        match fs::remove_dir_all(dir.join(sub)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn to_io_err(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

/// Writes `bytes` to `path` via a temp-file-then-rename, so concurrent readers of `path` (this crate's
/// own `get_ir`/`get_findings`, or an external tool) never observe a partially-written file. See the
/// module doc for the Windows rename caveat this function absorbs.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = temp_sibling(path);
    fs::write(&tmp_path, bytes)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Every writer for this exact `path` carries identical bytes (see module doc), so a
            // concurrent writer finishing first and leaving `path` in place is a benign race, not a
            // failure — clean up our now-redundant temp file and report success. Only propagate the
            // error if `path` genuinely never got written (a real I/O problem).
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

#[cfg(test)]
mod tests;
