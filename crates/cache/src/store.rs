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
//! ## Two reclamation mechanisms, doing two different jobs (2026-08-05)
//!
//! Entries are pure derived state, immutable once written, and addressed by a digest of their own key,
//! so a key that stops being asked for simply stops being read: it is never stale, only orphaned.
//! Editing a source file orphans its old `content_hash` entry, so without reclamation the directory
//! grows monotonically. Two things reclaim, and conflating them was the defect this split fixes:
//!
//! 1. **[`AnalysisCache::open`]'s schema-version wipe** — CONTRACT invalidation. The stored version
//!    differs from the caller's, so every entry was written under a shape or meaning this binary does
//!    not speak. Bulk, immediate, all of it.
//! 2. **[`crate::evict`]'s size cap** — HOUSEKEEPING. The contract is fine; the directory is just
//!    bigger than its budget, so the oldest-written entries go until it is back under. See that
//!    module's doc for why deleting a live entry here is safe (a miss, never a wrong answer).
//!
//! **Until 2026-08-05 mechanism 2 did not exist and mechanism 1 did both jobs**, because
//! `CACHE_SCHEMA_VERSION` carried the release version — so every upgrade wiped, whether or not any
//! analysis contract had moved. That made a release touching zero analysis code cost every user a full
//! cold run, and it is why the release half is gone: the hash half now decides invalidation alone, and
//! the cap decides housekeeping alone.
//!
//! The consequence for key CHANGES is unchanged: adding a field to `CacheKey`/`IrKey` orphans EVERY
//! existing entry at once (every digest moves). That needs no version bump for CORRECTNESS (see the key
//! contract's own note on `key.rs`: every key mutation degrades to a MISS, never a stale hit) — and it
//! no longer needs one for HOUSEKEEPING either, because the cap collects the orphans on its own
//! schedule instead of requiring someone to notice.
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
//! Every write here lands through [`atomic::write_atomic`] (temp file + rename). Its contract — why a
//! lost rename race on Windows is success rather than an error, and what "equivalent, not identical"
//! means for two racing writers — lives in that module's own doc, beside the code it constrains.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zzop_core::Finding;

use crate::ir_slice::FileIrSlice;
use crate::key::{CacheKey, IrKey};

mod atomic;
mod entries;
mod integrity;
use atomic::write_atomic;

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
    /// Self-hash binding this entry's payload to its key — [`integrity`] owns what it detects and,
    /// just as importantly, what it does not.
    payload_digest: String,
    #[serde(flatten)]
    key: IrKey,
    ir: FileIrSlice,
}

/// Same flattened-key shape as [`IrEntry`], over the full five-field key.
#[derive(Serialize, Deserialize)]
struct FindingsEntry {
    format_version: u32,
    payload_digest: String,
    #[serde(flatten)]
    key: CacheKey,
    findings: Vec<Finding>,
}

/// The file-level analysis cache: per-file Common IR slices and per-file rule findings, stored as
/// separate on-disk entries. See the crate doc for the layout and format.
pub struct AnalysisCache {
    root: PathBuf,
    /// How many entries [`Self::open`]'s housekeeping pass deleted on the way in (see
    /// [`Self::evicted_entries`]). Recorded at open and never mutated afterwards — this counts the cap
    /// enforcement that already happened, not a running total of anything this handle does later.
    evicted: usize,
    /// How many entries this handle READ and refused because their payload digest did not match (see
    /// [`integrity`]). Unlike `evicted`, this is a running total: it counts a decision made per lookup,
    /// so it can only be read after the lookups have happened.
    rejected: std::sync::atomic::AtomicUsize,
}

impl AnalysisCache {
    /// Opens (creating if absent) the cache directory at `dir`. `schema_version` identifies the Common IR
    /// / entry-format contract this caller speaks; if the directory's stored version differs (or there is
    /// none yet), every entry is wiped before the new one is recorded — a bulk invalidation for "the IR
    /// contract changed", not a per-entry decision.
    ///
    /// **The comparison is EQUALITY, never an ordering, and that is load-bearing.** A stored version
    /// that is "newer" is just as much a mismatch as one that is older, so a DOWNGRADE wipes exactly
    /// like an upgrade, and skipping any number of generations is indistinguishable from stepping one.
    /// Replacing this with a `stored < mine` test would leave an older binary reading entries written
    /// under a contract it does not speak — a stale HIT, the one outcome this cache must never produce.
    ///
    /// Then housekeeping: [`crate::evict::evict_to_cap`] brings the directory back under its size
    /// budget, on every open — the only moment this crate is guaranteed to be entered. What it deleted
    /// is kept, not discarded: see [`Self::evicted_entries`] / [`Self::eviction_warning`].
    pub fn open(dir: &Path, schema_version: &str) -> io::Result<AnalysisCache> {
        Self::open_with_cap(dir, schema_version, crate::evict::MAX_CACHE_BYTES)
    }

    /// [`Self::open`] with the disk budget injected, so a test can OBSERVE the eviction step with a few
    /// small entries rather than 256 MiB of them. Before this seam, deleting the `evict_to_cap` call
    /// below left the whole workspace suite green (measured 2026-08-05): `evict`'s own tests call that
    /// function directly, and nothing watched `open` calling it.
    pub(crate) fn open_with_cap(dir: &Path, version: &str, cap: u64) -> io::Result<AnalysisCache> {
        fs::create_dir_all(dir)?;
        let version_path = dir.join(SCHEMA_VERSION_FILE);
        let existing = fs::read_to_string(&version_path).ok();
        if existing.as_deref() != Some(version) {
            wipe_entries(dir)?;
            write_atomic(&version_path, version.as_bytes())?;
        }
        fs::create_dir_all(dir.join(IR_DIR))?;
        fs::create_dir_all(dir.join(FINDINGS_DIR))?;
        let evicted = crate::evict::evict_to_cap(
            &crate::evict::entry_dirs(dir, &[IR_DIR, FINDINGS_DIR]),
            cap,
        );
        Ok(AnalysisCache {
            root: dir.to_path_buf(),
            evicted,
            rejected: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// How many entries the housekeeping pass deleted during [`Self::open`] — 0 on the overwhelmingly
    /// common path where the cache was already under budget.
    ///
    /// Kept as its own accessor because eviction is otherwise a SILENT state change: the next run pays a re-analysis
    /// cost for every dropped entry, and without this the user has no way to learn why it got slower.
    /// A caller that wants the ready-made disclosure sentence should use [`Self::eviction_warning`]
    /// rather than re-deriving one from this number.
    /// `#[cfg(test)]`, and that is the honest shape (2026-09-07, review ledger V75). It was `pub`, and
    /// narrowing it made the compiler say the sharper thing: NOTHING in a non-test build calls it. The
    /// production channel is [`Self::eviction_warning`], which reads `self.evicted` directly — so this
    /// accessor exists to let a test assert the exact COUNT rather than parse it back out of a sentence,
    /// which is the message-string coupling this repo keeps out of its tests.
    #[cfg(test)]
    pub(crate) fn evicted_entries(&self) -> usize {
        self.evicted
    }

    /// How many entries this handle read and REFUSED because their payload digest did not match the
    /// payload — see [`store::integrity`](integrity) for exactly what that detects and what it does
    /// not. 0 on every healthy run.
    ///
    /// Exposed for the same reason as [`Self::evicted_entries`], one degree more urgently: recomputing
    /// silently would leave a cache that has been serving hand-edited answers indistinguishable from
    /// one that never was. The number is meaningful only after the lookups it counts, so read it at the
    /// end of a run.
    pub fn rejected_entries(&self) -> usize {
        self.rejected.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn record_rejected(&self) {
        self.rejected
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// The user-facing disclosure for [`Self::rejected_entries`], or `None` when nothing was refused.
    ///
    /// `None` at zero for the same reason [`Self::eviction_warning`] is silent at zero — and here the
    /// bar is higher still, because a line that appears on every healthy run is exactly how a reader
    /// learns to skip the one run where it matters.
    pub fn integrity_warning(&self) -> Option<String> {
        let n = self.rejected_entries();
        if n == 0 {
            return None;
        }
        let (noun, verb) = if n == 1 {
            ("entry", "its payload does")
        } else {
            ("entries", "their payloads do")
        };
        Some(format!(
            "{n} cache {noun} did not match {} own recorded contents and {} recomputed from source \
             instead of being served — {verb} not hash to what zzop wrote there, so something \
             modified the entry after it was written (a hand-edit, a partial overwrite, or disk \
             corruption). The findings you are reading for those files are freshly computed and \
             correct. This is a detector, not an authenticity check: the digest is keyless and public, \
             so an edit that also recomputed it would pass unnoticed. If you did not edit the cache, \
             treat this as a reason to delete the cache directory and re-run.",
            if n == 1 { "its" } else { "their" },
            if n == 1 { "was" } else { "were" },
        ))
    }

    /// The user-facing disclosure for [`Self::evicted_entries`], or `None` when nothing was evicted.
    ///
    /// **`None` at zero is the contract, not an optimization.** Eviction is rare by design (the budget
    /// is far larger than any single tree's live set — see [`crate::evict::MAX_CACHE_BYTES`]), so a line
    /// emitted on every run would be noise on ~every run and would train readers to ignore the channel
    /// it arrives on, which is where the failures that DO matter are reported.
    ///
    /// The sentence lives here rather than at the call site because this is the only place the fact can
    /// be exercised: the cap is a private constant, and only this crate can drive an eviction (via
    /// `open_with_cap`) to check what the message actually says. Its closing clause — that the cap is
    /// not a config key — is owned by [`crate::evict::MAX_CACHE_BYTES`], not restated here.
    pub fn eviction_warning(&self) -> Option<String> {
        if self.evicted == 0 {
            return None;
        }
        let noun = if self.evicted == 1 {
            "entry"
        } else {
            "entries"
        };
        Some(format!(
            "cache housekeeping: {} {noun} evicted to bring the cache back under its size cap. Not an \
             error — cache entries are derived state and the oldest-written go first, so no analysis \
             result is lost. The cost is one slower run: the next analysis recomputes the files those \
             entries covered instead of reading them back, and runs after it are warm again. The cap is \
             a fixed budget in this build, not a config key.",
            self.evicted
        ))
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

#[cfg(test)]
mod tests;
