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
mod tests {
    use super::*;
    use zzop_core::Severity;

    /// A fresh, unique scratch directory under the OS temp dir — no `tempfile` crate dependency (this
    /// crate's dependency budget is `zzop-core` + `serde` + `serde_json` only), so tests roll their own via
    /// the same pid+counter+nanos uniqueness scheme as `temp_sibling`.
    fn scratch_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "zzop-cache-test-{tag}-{}-{nanos}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn key(content: &str, parser: &str, ruleset: &str) -> CacheKey {
        scoped_key(content, parser, "a.ts", ruleset)
    }

    fn scoped_key(content: &str, parser: &str, scope: &str, ruleset: &str) -> CacheKey {
        CacheKey {
            content_hash: AnalysisCache::content_hash(content.as_bytes()),
            parser_fingerprint: parser.to_string(),
            scope: scope.to_string(),
            ruleset_fingerprint: ruleset.to_string(),
        }
    }

    fn sample_ir(loc: u32) -> FileIrSlice {
        FileIrSlice {
            symbols: vec![zzop_core::SourceSymbol {
                id: "a.ts#foo".to_string(),
                file: "a.ts".to_string(),
                name: "foo".to_string(),
                kind: zzop_core::SourceSymbolKind::Function,
                line: 1,
                exported: true,
                is_default: false,
                body_start: Some(1),
                body_end: Some(3),
                write_sites: Vec::new(),
            }],
            imports: Some(zzop_core::ImportMap::new()),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            loc,
            degraded: false,
            io: None,
            used_names: Vec::new(),
            minified_or_generated: false,
            const_map_fragment: std::collections::HashMap::new(),
            trpc_router_fragments: Vec::new(),
            router_mount_fragments: Vec::new(),
            wrapper_def_fragments: Vec::new(),
            wrapper_call_fragments: Vec::new(),
            controller_prefix_route_fragments: Vec::new(),
            query_call_sites: Vec::new(),
            store_bound_models: Vec::new(),
            field_usage_tokens: Vec::new(),
        }
    }

    fn sample_findings() -> Vec<Finding> {
        vec![Finding {
            rule_id: "pack/rule".to_string(),
            severity: Severity::Warning,
            file: "a.ts".to_string(),
            line: 1,
            message: "example finding".to_string(),
            data: None,
        }]
    }

    // Compares two serde-serializable values structurally by round-tripping through `serde_json::Value` —
    // sidesteps the fact that `SourceSymbol` (and therefore `FileIrSlice`) does not derive `PartialEq`.
    fn json_eq<T: Serialize>(a: &T, b: &T) -> bool {
        serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
    }

    #[test]
    fn roundtrip_ir_put_get() {
        let dir = scratch_dir("roundtrip-ir");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k = key("content", "ts+swc1+logic1", "pack@1");
        let ir = sample_ir(10);

        assert!(cache.get_ir(&k).is_none());
        cache.put_ir(&k, &ir).unwrap();
        let got = cache.get_ir(&k).expect("expected IR hit after put");
        assert!(json_eq(&got, &ir));
    }

    #[test]
    fn roundtrip_preserves_the_minified_flag() {
        // `FileIrSlice::minified_or_generated` (added alongside `zzop-cache-v6`) must survive a put/get round
        // trip exactly like every other field `roundtrip_ir_put_get` already covers in aggregate — this test
        // isolates just that one field so a future regression that quietly serializes/deserializes it wrong
        // (e.g. an errant `#[serde(skip)]`) fails here specifically, not just as an unexplained diff in the
        // broader `json_eq` comparison above.
        let dir = scratch_dir("roundtrip-minified-flag");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k = key("content", "ts+swc1+logic1", "pack@1");
        let mut ir = sample_ir(10);
        ir.minified_or_generated = true;

        cache.put_ir(&k, &ir).unwrap();
        let got = cache.get_ir(&k).expect("expected IR hit after put");
        assert!(
            got.minified_or_generated,
            "the minified_or_generated flag must round-trip as true"
        );
    }

    #[test]
    fn roundtrip_findings_put_get() {
        let dir = scratch_dir("roundtrip-findings");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k = key("content", "ts+swc1+logic1", "pack@1");
        let findings = sample_findings();

        assert!(cache.get_findings(&k).is_none());
        cache.put_findings(&k, &findings).unwrap();
        let got = cache
            .get_findings(&k)
            .expect("expected findings hit after put");
        assert!(json_eq(&got, &findings));
    }

    #[test]
    fn miss_on_content_change() {
        let dir = scratch_dir("miss-content");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k1 = key("content-a", "parser1", "ruleset1");
        let k2 = key("content-b", "parser1", "ruleset1");
        cache.put_ir(&k1, &sample_ir(1)).unwrap();
        cache.put_findings(&k1, &sample_findings()).unwrap();

        assert!(cache.get_ir(&k2).is_none());
        assert!(cache.get_findings(&k2).is_none());
        // original key is unaffected
        assert!(cache.get_ir(&k1).is_some());
        assert!(cache.get_findings(&k1).is_some());
    }

    #[test]
    fn miss_on_parser_fingerprint_change() {
        let dir = scratch_dir("miss-parser");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k1 = key("content", "parser-v1", "ruleset1");
        let k2 = key("content", "parser-v2", "ruleset1");
        cache.put_ir(&k1, &sample_ir(1)).unwrap();

        assert!(cache.get_ir(&k2).is_none());
        assert!(cache.get_ir(&k1).is_some());
    }

    #[test]
    fn ir_preserved_when_only_ruleset_changes_but_findings_are_not() {
        let dir = scratch_dir("ruleset-split");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k1 = key("content", "parser1", "ruleset-a");
        let k2 = key("content", "parser1", "ruleset-b");
        let ir = sample_ir(42);
        cache.put_ir(&k1, &ir).unwrap();
        cache.put_findings(&k1, &sample_findings()).unwrap();

        // Same content + parser, different ruleset: IR is still a hit (ruleset-independent key)...
        let got_ir = cache
            .get_ir(&k2)
            .expect("IR must be reusable across ruleset change");
        assert!(json_eq(&got_ir, &ir));
        // ...but findings, keyed on the full triple, are a miss until re-run and stored under the new key.
        assert!(cache.get_findings(&k2).is_none());
    }

    #[test]
    fn schema_version_mismatch_wipes_existing_entries() {
        let dir = scratch_dir("schema-wipe");
        let k = key("content", "parser1", "ruleset1");
        {
            let cache = AnalysisCache::open(&dir, "schema-v1").unwrap();
            cache.put_ir(&k, &sample_ir(1)).unwrap();
            cache.put_findings(&k, &sample_findings()).unwrap();
            assert!(cache.get_ir(&k).is_some());
        }
        // Reopening with a different schema version wipes prior entries.
        let cache = AnalysisCache::open(&dir, "schema-v2").unwrap();
        assert!(cache.get_ir(&k).is_none());
        assert!(cache.get_findings(&k).is_none());
    }

    #[test]
    fn schema_version_match_preserves_entries_across_reopen() {
        let dir = scratch_dir("schema-keep");
        let k = key("content", "parser1", "ruleset1");
        {
            let cache = AnalysisCache::open(&dir, "schema-v1").unwrap();
            cache.put_ir(&k, &sample_ir(7)).unwrap();
        }
        let cache = AnalysisCache::open(&dir, "schema-v1").unwrap();
        assert!(cache.get_ir(&k).is_some());
    }

    #[test]
    fn corrupted_ir_entry_is_treated_as_miss_not_panic() {
        let dir = scratch_dir("corrupt-ir");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k = key("content", "parser1", "ruleset1");
        let path = cache.ir_path(&k);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{ not valid json at all").unwrap();

        assert!(cache.get_ir(&k).is_none());
    }

    #[test]
    fn key_mismatch_inside_entry_is_treated_as_miss() {
        // Defends the "compare the stored key, not just the digest" guard documented in hash.rs: even if
        // an entry file exists at the expected path with valid JSON, a stored key that does not match the
        // requested key must not be returned.
        let dir = scratch_dir("key-mismatch");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k = key("content", "parser1", "ruleset1");
        let path = cache.ir_path(&k);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let wrong_entry = IrEntry {
            format_version: FORMAT_VERSION,
            content_hash: "not-the-right-hash".to_string(),
            parser_fingerprint: "parser1".to_string(),
            scope: "a.ts".to_string(),
            ir: sample_ir(1),
        };
        fs::write(&path, serde_json::to_vec(&wrong_entry).unwrap()).unwrap();

        assert!(cache.get_ir(&k).is_none());
    }

    #[test]
    fn miss_on_scope_change() {
        // The bug this field exists to close: two DIFFERENT files (different `scope`) with byte-identical
        // content, the same parser fingerprint, and the same ruleset must NOT alias each other's cache
        // entry — see `CacheKey::scope`'s doc for why a file's cached IR/findings embed its own path.
        let dir = scratch_dir("miss-scope");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k1 = scoped_key("same-content", "parser1", "routes/a.ts", "ruleset1");
        let k2 = scoped_key("same-content", "parser1", "other/a.ts", "ruleset1");
        cache.put_ir(&k1, &sample_ir(1)).unwrap();
        cache.put_findings(&k1, &sample_findings()).unwrap();

        assert!(cache.get_ir(&k2).is_none());
        assert!(cache.get_findings(&k2).is_none());
        // original key is unaffected
        assert!(cache.get_ir(&k1).is_some());
        assert!(cache.get_findings(&k1).is_some());
    }

    #[test]
    fn ir_is_scope_sensitive_not_just_findings() {
        // Unlike `ruleset_fingerprint` (findings-only), `scope` gates the IR lookup too: a `FileIrSlice`'s
        // `symbols`/`io` carry their own originating path, so IR is not purely a function of
        // (content, parser) the way it is for `ruleset_fingerprint`.
        let dir = scratch_dir("scope-gates-ir");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k1 = scoped_key("content", "parser1", "a.ts", "ruleset1");
        let k2 = scoped_key("content", "parser1", "b.ts", "ruleset1");
        cache.put_ir(&k1, &sample_ir(5)).unwrap();

        assert!(
            cache.get_ir(&k2).is_none(),
            "same content+parser but different scope must miss on IR too"
        );
    }

    #[test]
    fn truncated_empty_file_is_treated_as_miss() {
        let dir = scratch_dir("truncated");
        let cache = AnalysisCache::open(&dir, "v1").unwrap();
        let k = key("content", "parser1", "ruleset1");
        let path = cache.findings_path(&k);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"").unwrap();

        assert!(cache.get_findings(&k).is_none());
    }

    #[test]
    fn concurrent_puts_from_multiple_threads_all_land() {
        let dir = scratch_dir("concurrent");
        let cache = std::sync::Arc::new(AnalysisCache::open(&dir, "v1").unwrap());
        let mut handles = Vec::new();
        for i in 0..16u32 {
            let cache = cache.clone();
            handles.push(std::thread::spawn(move || {
                let k = key(&format!("content-{i}"), "parser1", "ruleset1");
                cache.put_ir(&k, &sample_ir(i)).unwrap();
                cache.put_findings(&k, &sample_findings()).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        for i in 0..16u32 {
            let k = key(&format!("content-{i}"), "parser1", "ruleset1");
            let ir = cache
                .get_ir(&k)
                .unwrap_or_else(|| panic!("missing ir for {i}"));
            assert_eq!(ir.loc, i);
            assert!(cache.get_findings(&k).is_some());
        }
    }

    #[test]
    fn concurrent_puts_to_the_same_key_are_harmless() {
        // Every writer for the same key produces byte-identical output, so racing on the exact same
        // target path (the Windows rename caveat documented in the module doc) must never error.
        let dir = scratch_dir("concurrent-same-key");
        let cache = std::sync::Arc::new(AnalysisCache::open(&dir, "v1").unwrap());
        let k = key("same-content", "parser1", "ruleset1");
        let mut handles = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            let k = k.clone();
            handles.push(std::thread::spawn(move || {
                cache.put_ir(&k, &sample_ir(99)).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let ir = cache
            .get_ir(&k)
            .expect("entry must exist after concurrent puts");
        assert_eq!(ir.loc, 99);
    }
}
