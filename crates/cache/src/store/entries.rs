//! Per-entry READ/WRITE — the four `get_*`/`put_*` accessors, the content-addressing hash, and the
//! path derivation they share. Split out of `store.rs` on 2026-08-08 as a PURE MOVE (no logic change)
//! along the seam that file already had: its sibling `lifecycle` owns opening, version-mismatch
//! wiping and eviction accounting — everything that happens ONCE per handle — while this owns what
//! happens once per FILE.

use std::fs;
use std::io;
use std::path::PathBuf;

use zzop_core::Finding;

use super::{
    write_atomic, AnalysisCache, FindingsEntry, IrEntry, FINDINGS_DIR, FORMAT_VERSION, IR_DIR,
};
use crate::hash::digest128;
use crate::ir_slice::FileIrSlice;
use crate::key::{CacheKey, IrKey};

impl AnalysisCache {
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

    pub(super) fn ir_path(&self, key: &IrKey) -> PathBuf {
        self.entry_path(IR_DIR, &key.digest_input())
    }

    pub(super) fn findings_path(&self, key: &CacheKey) -> PathBuf {
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

pub(super) fn to_io_err(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}
