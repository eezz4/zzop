//! Size-capped eviction — the cache's only reclamation mechanism since the schema version stopped
//! carrying the release version (2026-08-05).
//!
//! ## Why this exists at all
//! Until 2026-08-05 the schema version was `{release}+{source_hash}`, so every release changed it and
//! every upgrade wiped the directory. That wipe was doing two unrelated jobs at once: invalidating
//! entries whose CONTRACT changed (the hash half's job) and reclaiming entries nobody addresses any
//! more (housekeeping). Tying housekeeping to the release axis meant a release that changed no analysis
//! code at all still cost every user a full cold run — measured on `v0.29.0 -> v0.29.1`, which touched
//! zero bytes under `crates/cache`, `crates/core`, `crates/engine`, `parser/` or `rules/`.
//!
//! Dropping the release half fixes that and leaves the hash to do only its own job. But the hash never
//! moves for someone running a released binary — every fingerprint is a compile-time constant — so
//! without a replacement, nothing would ever reclaim anything. This module is that replacement.
//!
//! ## Deleting a live entry is not dangerous, and that is the whole design
//! This crate's entries are pure derived state, immutable once written, addressed by a digest of their
//! own key. Deleting one that is still being asked for produces a MISS, which recomputes the same
//! answer — never a wrong one. So eviction needs no reachability analysis and cannot be "wrong" in the
//! direction that matters; the worst outcome is one slower run.
//!
//! That is worth stating plainly because the module doc this replaces argued the opposite — that a GC
//! "would have to decide 'still reachable?' from outside the run that knows, and getting that wrong
//! deletes a live entry". True of a reachability-based GC, and the reason not to build one. Not true of
//! a size cap, which never asks the question.
//!
//! ## Oldest-WRITTEN, not least-recently-used
//! Eviction orders by mtime, which is when an entry was written, not when it was last read — reads do
//! not touch the file, and making them touch it would put a write on every cache HIT, which is the
//! opposite of what a cache is for. So a hot entry written long ago can be evicted ahead of a cold one
//! written recently. That is accepted: the cost is one recompute, and the alternative costs a write per
//! hit forever.
//!
//! ## Concurrency
//! Safe against a concurrent run sharing the directory, for the same reason the rest of this crate is:
//! a reader whose file disappears mid-read gets a miss (`store`'s read paths treat any error as a miss,
//! pinned by `corrupted_ir_entry_is_treated_as_miss_not_panic`), and a writer racing the evictor simply
//! rewrites what was deleted. No lock is taken and none is needed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Disk budget for one cache directory, across both entry kinds.
///
/// POLICY VALUE (cap axis). At this repo's measured mean of ~1.9 KB/entry that is roughly 137,000
/// entries — far more than any single tree's LIVE set (analyzing this repo writes ~1,550 per run), so in
/// practice the evictor only ever reaches entries already orphaned by an edit. It is also small enough
/// not to be rude on a laptop that analyzes several trees.
///
/// ⚠ This comment said `~6 KB/entry` / `~44,000` until 2026-08-05, roughly 3× off — a policy value whose
/// stated justification was never re-measured after the entry shape changed. Recount before trusting it:
///
/// ```sh
/// find .zzop/cache/ir .zzop/cache/findings -type f -printf '%s\n' |
///   awk '{n++; b+=$1} END{printf "%d entries, %.2f KB mean\n", n, b/n/1024}'
/// ```
///
/// It is deliberately NOT a config key yet. Making it declarable is a separate decision with its own
/// cost (one more knob on a surface this repo keeps deliberately small), and the trigger for taking it
/// is a user who actually hits the cap and wants it moved — which nobody has, because until this module
/// landed there was no cap to hit.
///
/// ⚠ That last paragraph is now SHIPPED PROSE, not just an internal note: `AnalysisCache::eviction_warning`
/// ends by telling the user the cap is "a fixed budget in this build, not a config key", so a reader who
/// wonders how to raise it stops here instead of hunting the config surface for a key that is not there.
/// Verified 2026-08-06 against `crates/config/config-surface.json` — its only cache keys are `cacheDir`
/// (which directory) and `cacheLaneAnchorPattern` (unrelated), neither of which sets a budget. **Whoever
/// makes this declarable has to edit that sentence in the same commit**, or the tool starts denying the
/// existence of its own knob.
pub(crate) const MAX_CACHE_BYTES: u64 = 256 * 1024 * 1024;

/// How far below [`MAX_CACHE_BYTES`] one eviction pass reclaims to.
///
/// Hysteresis, not tuning: evicting to exactly the cap would leave the directory sitting AT the cap, so
/// the next run's handful of new entries would cross it again and pay for another full pass. Reclaiming
/// to three quarters buys roughly 64 MiB of headroom — thousands of runs' worth — so a pass that
/// actually deletes anything is rare rather than per-run.
const EVICT_TO_RATIO_NUM: u64 = 3;
const EVICT_TO_RATIO_DEN: u64 = 4;

/// Brings `dirs`' combined size under [`MAX_CACHE_BYTES`], deleting oldest-written first.
///
/// Returns the number of entries deleted (0 on the overwhelmingly common path where the cache is under
/// budget). Never fails the run: an unreadable directory, an entry that vanishes under us, or a file
/// locked by another process are all skipped, because failing an ANALYSIS over housekeeping would be a
/// strictly worse trade than carrying a few extra megabytes.
pub(crate) fn evict_to_cap(dirs: &[PathBuf], cap: u64) -> usize {
    let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total: u64 = 0;

    for dir in dirs {
        let Ok(read) = fs::read_dir(dir) else {
            continue; // not created yet, or unreadable — nothing to reclaim either way
        };
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let len = meta.len();
            total = total.saturating_add(len);
            // A filesystem with no mtime sorts as oldest; it is a candidate before anything dated,
            // which is the fail-safe direction (worst case: one recompute).
            let written = meta.modified().unwrap_or(UNIX_EPOCH);
            entries.push((written, len, entry.path()));
        }
    }

    if total <= cap {
        return 0;
    }

    let target = cap / EVICT_TO_RATIO_DEN * EVICT_TO_RATIO_NUM;
    // Oldest first. The `len`/path tail makes the order total, so two entries written in the same
    // filesystem tick evict in a defined order rather than an arbitrary one.
    entries.sort();

    let mut deleted = 0usize;
    for (_, len, path) in entries {
        if total <= target {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
            deleted += 1;
        }
    }
    deleted
}

/// The two entry directories under `root`, in the order [`evict_to_cap`] should read them.
pub(crate) fn entry_dirs(root: &Path, subdirs: &[&str]) -> Vec<PathBuf> {
    subdirs.iter().map(|s| root.join(s)).collect()
}

#[cfg(test)]
mod tests;
