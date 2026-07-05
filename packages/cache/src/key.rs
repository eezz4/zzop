//! Cache key — fingerprints used to key the incremental cache: content hash, parser fingerprint,
//! scope, and ruleset fingerprint (see `docs/ARCHITECTURE.md`, "Caching").

use serde::{Deserialize, Serialize};

/// (content hash, parser fingerprint, scope, ruleset fingerprint) for one file. IR lookups key on
/// the first three (IR doesn't depend on active rule packs); findings lookups key on all four.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    /// Hash of the file's raw bytes, not mtime, so the cache survives checkouts/CI restores that
    /// change mtimes but not content.
    pub content_hash: String,
    /// Parser id + pinned parser version + parser-logic version; bumping any invalidates every IR
    /// entry the old parser produced.
    pub parser_fingerprint: String,
    /// Disambiguates "which file, in which tree" (normalized relative path + tree id) — projected
    /// IR/findings can embed the file's own path, so byte-identical files must not alias each
    /// other's entry. Part of the IR key for that reason, not just the findings key.
    pub scope: String,
    /// Fingerprint of the active per-file rule packs; bumping invalidates findings but leaves the
    /// IR entry (parser+scope-keyed) reusable.
    pub ruleset_fingerprint: String,
}
