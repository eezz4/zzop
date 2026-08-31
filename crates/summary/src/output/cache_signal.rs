//! CACHE PROVENANCE — the reply's answer to "how much of what I am reading was recomputed?"
//!
//! Every other silence in this product self-reports, and this one did not. A file served whole from the
//! per-file cache never re-runs its rules: its findings are REPLAYED out of `.zzop/cache`. That is
//! ordinary and correct, and it is also the one path by which a reply can carry a zero nobody computed
//! this run — so a reader who has only the reply had no way to tell a recomputed finding list from a
//! replayed one. The counts existed (`zzop_engine::CacheStats`, on the wire as `cache`) but reached the
//! reader only through `--profile-rules`, a knob asked for a different reason entirely.
//!
//! Same discipline as [`super::timings`]: the numbers are run-VARYING and ride in the data, the prose is
//! run-INVARIANT and ships once, inside the object it describes — so a consumer who reads the numbers
//! cannot fail to have read what they do and do not prove.

/// The run-invariant half. Two things a reader would otherwise get wrong, in the order that matters.
///
/// FIRST, what a hit means for the findings: replayed, not recomputed. SECOND, the boundary of the
/// guarantee. The fingerprint set is genuinely strong on accidental staleness — an external reviewer
/// spent seven tricks on it (identical mtime with an equal-length replacement, a byte-identical file
/// forcing cross-file invalidation, truncated/empty/garbage entries, a changed config fingerprint, four
/// concurrent runs) and every one correctly recomputed. A WELL-FORMED entry used to be the hole — an
/// entry was a plain JSON document with nothing of its own to check, so hand-editing `findings` to `[]`
/// deleted those findings from every later run (measured 2026-08-16 on a file whose hardcoded credential
/// was still in the source). It is no longer: an entry carries a digest of its own payload and a
/// mismatch forces a recompute, which this line now says. The boundary moved rather than disappeared:
/// the digest is keyless and public, so it detects corruption and cannot prove authorship — an edit that
/// recomputed the digest too still passes. The line draws THAT boundary rather than claiming the
/// stronger thing, because the alternative is a reader who cannot audit the answer they were given. (The digest itself was silently self-refuting until 2026-08-20:
/// it was taken over `serde_json` output, whose object key order follows a `HashMap`'s per-instance
/// seed, so ~40 entries per run failed their own check and the cache never went warm.)
const MEANING: &str = "`hitFiles` of `fileCount` files were served whole from the cache: their findings \
     in this reply were REPLAYED from the cache directory, not recomputed from the file as it is now. \
     Reuse is decided by fingerprints over the file's content, the ruleset, the declared vocabulary and \
     the engine version, and every accidental-staleness path they cover was measured as correctly \
     recomputing. Beyond them, each entry carries a digest of its own payload — a keyless integrity \
     check — so an entry edited without recomputing that digest is refused, its file is recomputed from \
     source, and any this run refused are reported to you. Where that stops: the digest is public, so it \
     detects corruption rather than proving authorship, and an edit that recomputed the digest too would \
     pass unnoticed. If a zero here has to be trusted, recompute the whole tree — delete the cache \
     directory, or set `cacheDir` to null — and compare. `missFiles` were computed by this run.";

/// Shapes the facade output's `cache` field into the reply's `cache` object, or `None` when this run
/// used no cache at all.
///
/// Absent, never null: `cacheDir: null` (or an unopenable cache) leaves the facade's own field null, and
/// the key's PRESENCE is itself the signal that a cache was in play. A run with caching off has no
/// provenance question to answer, so it gains no key — the same absent-vs-null contract
/// `architecture`/`ruleTimings` keep in the same reply.
///
/// `fileCount` is repeated inside the object rather than left to the sibling top-level key, for the
/// reason the ratio is the whole point: `22 / 22` and `22 / 4000` are different claims about the reply,
/// and a reader should not have to join two keys to see which one they hold.
pub(crate) fn shape_cache_signal(output_view: &serde_json::Value) -> Option<serde_json::Value> {
    let cache = output_view.get("cache")?;
    let hits = cache.get("hits")?.as_u64()?;
    let misses = cache.get("misses").and_then(serde_json::Value::as_u64)?;
    Some(serde_json::json!({
        "hitFiles": hits,
        "missFiles": misses,
        "fileCount": output_view.get("fileCount").cloned().unwrap_or(serde_json::Value::Null),
        "meaning": MEANING,
    }))
}
