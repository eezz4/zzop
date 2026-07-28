//! `AnalysisCache` — the on-disk store. Layout, format, and atomicity are documented inline; see the
//! crate doc for the design this implements.
//!
//! ## Storage layout
//!
//! ```text
//! <root>/
//!   schema_version        plain UTF-8 text: the schema version string passed to `open`
//!   ir/<digest>.json      one IrEntry per (content_hash, parser_fingerprint, scope, vocabulary_fingerprint)
//!   findings/<digest>.json one FindingsEntry per (… the same four, plus ruleset_fingerprint)
//! ```
//!
//! ## No eviction, by design — and what that costs when a key gains a field
//!
//! Nothing in this crate deletes an entry except [`AnalysisCache::open`]'s schema-version wipe. Entries
//! are pure derived state, immutable once written, and addressed by a digest of their own key, so a key
//! that stops being asked for simply stops being read: it is never stale, only orphaned. That is the
//! deliberate trade — a background GC would have to decide "still reachable?" from outside the run that
//! knows, and getting that wrong deletes a live entry. Editing a source file already orphans its old
//! `content_hash` entry, so the directory has always grown monotonically and `docs/ARCHITECTURE.md`
//! documents it as safe to delete at any time.
//!
//! The consequence for key CHANGES: adding a field to `CacheKey`/`IrKey` orphans EVERY existing entry at
//! once (every digest moves), so the reclamation lever is the one that already exists — a
//! `CACHE_SCHEMA_VERSION` bump, whose bulk wipe turns "the old generation lives on disk forever" into
//! "one cold run". A key change needs no bump for CORRECTNESS (see
//! `scripts/check-parser-fingerprint-bump.sh`'s note on `key.rs`: every key mutation degrades to a MISS,
//! never a stale hit); it needs one for HOUSEKEEPING.
//!
//! `<digest>` is `hash::digest128` of `CacheKey::digest_input`/`IrKey::digest_input` — the key type's own
//! field list, NUL-joined, generated from the declaration (see `key.rs`) rather than hand-listed here, so
//! a new key field cannot silently skip the digest. It exists only to shard entries into filenames; it is
//! never trusted on its own (see `hash.rs` and the read-path key comparison below).
//!
//! ## Format
//!
//! Each entry file is JSON with a leading `format_version` field (spec: "lead with a format-version
//! marker" — so a future switch to a binary format like bincode can coexist with, or cleanly reject,
//! entries written by this version). The cache key is duplicated inside the entry — the KEY VALUE
//! itself, `#[serde(flatten)]`ed so the stored JSON keeps the same flat field layout it always had;
//! every read compares stored-key against requested-key with `PartialEq` (never a hand-written field
//! chain, for the same reason the digest is generated) and treats a mismatch — or any deserialization
//! failure — as a miss rather than an error. See `hash.rs` for why this matters even though digest
//! collisions are already astronomically unlikely.
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
use crate::key::{CacheKey, IrKey};

const SCHEMA_VERSION_FILE: &str = "schema_version";
const IR_DIR: &str = "ir";
const FINDINGS_DIR: &str = "findings";
/// Leading format-version marker stored in every entry (see module doc). Bump when the JSON shape of
/// `IrEntry`/`FindingsEntry` changes in a way old readers cannot tolerate; a mismatch is treated as a
/// miss, not a crash (see `get_ir`/`get_findings`).
const FORMAT_VERSION: u32 = 1;

/// `key` is `#[serde(flatten)]`ed, so the stored JSON keeps the flat shape earlier versions wrote
/// (`format_version`, then the key's own fields, then `ir` — same names, same order) — this entry gained
/// a structural guarantee, not a new on-disk format, so no `CACHE_SCHEMA_VERSION` bump is owed. Pinned by
/// `stored_entries_keep_the_flat_on_disk_key_layout`.
#[derive(Serialize, Deserialize)]
struct IrEntry {
    format_version: u32,
    #[serde(flatten)]
    key: IrKey,
    ir: FileIrSlice,
}

/// Same flattened-key shape as [`IrEntry`], over the full five-field key.
#[derive(Serialize, Deserialize)]
struct FindingsEntry {
    format_version: u32,
    #[serde(flatten)]
    key: CacheKey,
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

    /// Looks up a file's cached Common IR slice by `(content_hash, parser_fingerprint, scope,
    /// vocabulary_fingerprint)` —
    /// ruleset-independent, per the spec's IR/findings split, but NOT scope-independent: `scope`
    /// disambiguates "which file" (see `CacheKey::scope`'s doc) since a `FileIrSlice`'s `symbols`/`io`
    /// embed their own originating path. Returns `None` on a miss, a stored-key mismatch (see module
    /// doc), or any I/O / deserialization failure — this method never panics or errors on a corrupted or
    /// missing entry, it simply reports "not cached".
    pub fn get_ir(&self, key: &CacheKey) -> Option<FileIrSlice> {
        let ir_key = IrKey::from(key);
        let bytes = fs::read(self.ir_path(&ir_key)).ok()?;
        let entry: IrEntry = serde_json::from_slice(&bytes).ok()?;
        if entry.format_version != FORMAT_VERSION || entry.key != ir_key {
            return None;
        }
        Some(entry.ir)
    }

    /// Stores `ir` under `(content_hash, parser_fingerprint, scope, vocabulary_fingerprint)`, independent of
    /// `key.ruleset_fingerprint` — a later `put_ir` for the same content + parser + scope but a different
    /// ruleset overwrites the same entry (harmlessly: the IR itself does not vary with the ruleset).
    pub fn put_ir(&self, key: &CacheKey, ir: &FileIrSlice) -> io::Result<()> {
        let entry = IrEntry {
            format_version: FORMAT_VERSION,
            key: IrKey::from(key),
            ir: ir.clone(),
        };
        let bytes = serde_json::to_vec(&entry).map_err(to_io_err)?;
        write_atomic(&self.ir_path(&entry.key), &bytes)
    }

    /// Looks up a file's cached per-file rule findings by the full `(content_hash, parser_fingerprint,
    /// scope, vocabulary_fingerprint, ruleset_fingerprint)` key. Same never-panics-on-corruption
    /// contract as `get_ir`.
    pub fn get_findings(&self, key: &CacheKey) -> Option<Vec<Finding>> {
        let bytes = fs::read(self.findings_path(key)).ok()?;
        let entry: FindingsEntry = serde_json::from_slice(&bytes).ok()?;
        if entry.format_version != FORMAT_VERSION || entry.key != *key {
            return None;
        }
        Some(entry.findings)
    }

    /// Stores `findings` under the full five-field key.
    pub fn put_findings(&self, key: &CacheKey, findings: &[Finding]) -> io::Result<()> {
        let entry = FindingsEntry {
            format_version: FORMAT_VERSION,
            key: key.clone(),
            findings: findings.to_vec(),
        };
        let bytes = serde_json::to_vec(&entry).map_err(to_io_err)?;
        write_atomic(&self.findings_path(key), &bytes)
    }

    fn ir_path(&self, key: &IrKey) -> PathBuf {
        self.entry_path(IR_DIR, &key.digest_input())
    }

    fn findings_path(&self, key: &CacheKey) -> PathBuf {
        self.entry_path(FINDINGS_DIR, &key.digest_input())
    }

    /// The one place a key digest becomes a filename. Takes the key's OWN `digest_input` (see `key.rs`)
    /// rather than a field list, so neither path function can drift from the key it shards.
    fn entry_path(&self, sub_dir: &str, digest_input: &str) -> PathBuf {
        self.root
            .join(sub_dir)
            .join(format!("{}.json", digest128(digest_input.as_bytes())))
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
