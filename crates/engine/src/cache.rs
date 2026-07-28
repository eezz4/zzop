//! Wiring between `zzop_cache::AnalysisCache` and the fused per-file pass (`pipeline.rs`) — implements
//! the cache key/fingerprint design `zzop_cache` defines (see `docs/ARCHITECTURE.md`'s "Caching" for the
//! user-facing contract). This module owns what that design leaves to "the caller": opening the on-disk
//! cache once per `analyze_tree` call (degrade, never panic, on failure), composing the ruleset
//! fingerprint, and the deterministic-under-rayon hit/miss counters behind `AnalyzeOutput::cache`.
//!
//! ## Parser fingerprint composition
//!
//! A file's cache key is `(content hash, parser fingerprint, scope, vocabulary fingerprint, ruleset
//! fingerprint)`. The parser
//! fingerprint is mainly "parser id + swc version + parser-logic version counter" (a function of which
//! language handled the file), plus one extra ingredient in [`parser_fingerprint`]:
//! `EngineConfig::size_cap`. `pipeline::process_file`'s oversized-file branch decides lexical-fallback
//! vs structural parse from `bytes.len() > config.size_cap` — a config value, not a per-file constant —
//! so two runs with identical content but a different `size_cap` can legitimately produce different
//! `FileIrSlice`s for the same file. Folding `size_cap` into every fingerprint means a `size_cap` change
//! invalidates the whole cache rather than risk ever returning a wrong-shaped entry.
//!
//! `EngineConfig::io` (`crate::io::IoOptions`, e.g. `router_names`) can also change a TypeScript file's
//! projected `io` for the same content, so [`parser_fingerprint`]'s TypeScript branch folds in
//! `config.io`'s `Debug` output too (same "no `Serialize`, but `Debug` is deterministic for a plain
//! struct with no hashmap inside it" reasoning as [`ruleset_fingerprint`] below). Scoped to the
//! TypeScript branch only — Prisma/lexical-fallback entries never consult `io`.
//!
//! ## Scope: the path-identity gap `CacheKey::scope` closes
//!
//! A file's cache key must include its own path, not just content + fingerprints: a `FileIrSlice`'s
//! `symbols`/`io` and a `Finding` all embed their own originating rel path, stamped at projection time.
//! Two different files with byte-identical content (a duplicate barrel re-export stub, a shared
//! license-header file, two empty test fixtures) that dispatch to the same language and ruleset would
//! otherwise produce the same cache key and silently alias — including which file a path-scoped rule's
//! `file_pattern` gating was evaluated against. [`cache_scope`] closes this: every `CacheKey::scope` is
//! `config.source_id` + the file's own `rel`, NUL-joined. `source_id` is included alongside `rel`
//! because nothing stops a caller from pointing two different trees' `cache_dir` at the same physical
//! directory (e.g. a monorepo sharing one cache across FE/BE roots); `source_id` is already a mandatory
//! field on every tree request, so this reuses an existing disambiguator. `scope` is part of the IR key,
//! not just the findings key, since the path-sensitive fields (`symbols`, `io`) live in `FileIrSlice`.
//!
//! ## Ruleset fingerprint composition
//!
//! Conceptually this hashes each active per-file rule pack's (id, schema_version, content).
//! `zzop_core::dsl::RulePackDef` derives `Deserialize` only — no `Serialize`, no `schema_version` field
//! (packs are versioned as whole files) — so [`ruleset_fingerprint`] hashes each enabled pack's derived
//! `Debug` output instead: deterministic across runs for a plain struct/enum with fixed field order and
//! no hashmap iteration anywhere in the AST. Packs are sorted by id before hashing so load order cannot
//! perturb the fingerprint.
//!
//! `RuleConfig`'s contribution is narrowed to `disabled_rules` — the only field that can change which
//! packs are cache-relevant without the fused pass ever calling `eval_pack` for a newly-disabled pack.
//! `severity_overrides`/`suppressions` are excluded: both are applied once, tree-wide, in
//! `registry::merge_findings`, after every artifact has already contributed its raw per-file findings —
//! they never change what a per-file cache entry should contain.

use std::sync::atomic::{AtomicUsize, Ordering};

use zzop_cache::AnalysisCache;
use zzop_core::RulePackDef;

use crate::dispatch::Language;
use crate::{CacheStats, EngineConfig};

/// Schema version passed to `AnalysisCache::open` — the cache's one bulk invalidator (see
/// `AnalysisCache::open`'s own doc: a mismatch is a wipe, not a per-entry migration). It is a
/// COMPOSITE of two axes, and neither one alone is enough:
///
/// - **Release axis** — `env!("CARGO_PKG_VERSION")`, the workspace version. DERIVED, never typed by
///   hand, so it cannot drift from the release it claims. It did drift: this const sat at `"0.23.0"`
///   for the whole of the 0.24.0 cycle and was corrected by hand, which is exactly the failure a
///   hand-copied version always eventually has. The consequence is deliberate and is now part of what
///   a release DOES: the string changes on every release, so **every release reclaims exactly one
///   cache generation**. This crate has no GC — `zzop_cache`'s store module doc records non-eviction
///   as designed — so a generation that stops being addressed would otherwise sit unread on every
///   user's disk forever. The price is one cold run per upgrade.
/// - **In-cycle axis** — the trailing `+rN` counter, bumped BY HAND when a persisted shape changes
///   BETWEEN releases. The release axis cannot cover that case at all: within one cycle the version
///   does not move, so a developer's (or a nightly consumer's) warm cache would be served slices of
///   the old shape under a version that claims to describe the new one. Monotonic and never reset —
///   a reset would be harmless once the release axis has moved, but buys nothing.
///
/// Bump `+rN` whenever `FileIrSlice`, the cached findings payload, or either entry envelope gains,
/// renames, or removes a field. That holds even when `#[serde(default)]` would deserialize an old
/// entry without erroring: a missing field silently defaulting (an empty `Vec`/`false`) is
/// indistinguishable from "genuinely has none", so a hit against a pre-existing directory would serve
/// a wrong answer instead of recomputing.
///
/// **And whenever the MEANING of an existing key ingredient changes**, which is not a shape change and
/// is easy to miss. `+r2` is exactly that case: on 2026-07-27 an undeclared convention vocabulary
/// stopped falling back to built-ins and started making no judgment at all. The declared struct — which
/// is what [`vocabulary_fingerprint`] hashes — is byte-identical across that change, so the key does NOT
/// move on its own, while the slices it addresses are computed differently. Without this bump a warm
/// cache would serve pre-flip IR under a post-flip key.
///
/// `scripts/check-parser-fingerprint-bump.sh`'s `core` scope enforces this. It compares the const's
/// value EXPRESSION at both ends of the diff range and resolves `env!("CARGO_PKG_VERSION")` against
/// each end's `Cargo.toml`, so both axes are visible to it: a `CORE_SHARED_FILES` change with neither
/// the release version nor `+rN` moved is red, and a release bump alone is accepted as the
/// invalidation it really is.
pub const CACHE_SCHEMA_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+r2");

/// Fingerprint for files that never reach a structural parser crate in the fused pass: no `Language` match
/// (`dispatch::dispatch` returned `None` — unrecognized extension), or the size-cap lexical fallback
/// (`pipeline::compute_fresh_artifact`'s oversized branch, which short-circuits before any language-specific
/// parse call). Both produce their result via this engine's own text-only heuristics
/// (`pipeline::lexical_loc`), never a parser crate, so this is a fixed local marker rather than a borrowed
/// `PARSER_FINGERPRINT` — restamp with the current `CARGO_PKG_VERSION` if the engine's own lexical-fallback
/// shape changes (2026-07-22 version reform: cache-bust tokens are package-version stamps).
const LEXICAL_FALLBACK_FINGERPRINT: &str = "lexical/0.21.0";

/// `ruleset_fingerprint`'s native-rule-logic-version token for `pipeline::schema_findings`
/// (`zzop_rules_schema::apply_schema_rules`, wired into the fused per-file pass for Prisma files). Unlike
/// a DSL pack (whose content already changes the fingerprint via `pack:?`), this is Rust logic with no
/// pack content to hash, so the version counter (`zzop_rules_schema::STRUCTURAL_RULES_VERSION`) lives
/// beside the rule itself. It covers everything that reaches the cached finding — not just the output
/// SHAPE but rule bodies, thresholds, and the message/disable-hint text authored in
/// `rules-schema`'s `message.rs`; see that const's doc for the cached (`structural.rs`) vs
/// recomputed-every-run (`usage.rs`) lane split that decides whether a bump is needed.
fn schema_structural_fingerprint() -> String {
    format!(
        "schema-structural-{}",
        zzop_rules_schema::STRUCTURAL_RULES_VERSION
    )
}

/// Logic-version token for the DSL *interpreter* (`zzop_core::dsl`) itself — the same stale-cache gap
/// `schema_structural_fingerprint` closes for native rule logic. Pack JSON already self-invalidates via
/// `{pack:?}` above, but a pure-Rust interpreter semantics change (matcher evaluation, suppress-marker
/// window, ...) alters findings for byte-identical source AND identical pack content — invisible to the
/// key without this token. Restamp with the current `CARGO_PKG_VERSION` on any such change (2026-07-22
/// version reform: cache-bust tokens are package-version stamps).
const DSL_INTERPRETER_FINGERPRINT: &str =
    "dsl/0.24.0+zzop-marker-prefix-v1+method-scan-trigger-lines";

/// Opens the on-disk cache at `config.cache_dir`, if set. Never panics: an open failure (bad permissions,
/// path collides with a plain file, disk full while writing the schema-version marker, ...) degrades to
/// "cache off" with one human-readable entry pushed to `warnings` — the same shape of degrade-not-crash
/// contract `analyze::collect_git` already uses for git-collection failures.
pub(crate) fn open_cache(
    config: &EngineConfig,
    warnings: &mut Vec<String>,
) -> Option<AnalysisCache> {
    let dir = config.cache_dir.as_ref()?;
    match AnalysisCache::open(dir, CACHE_SCHEMA_VERSION) {
        Ok(cache) => Some(cache),
        Err(e) => {
            warnings.push(format!(
                "cache disabled: failed to open {}: {e}",
                dir.display()
            ));
            None
        }
    }
}

/// The parser-fingerprint half of a file's `CacheKey` (see module doc for the `size_cap`/`io` additions
/// beyond the borrowed `PARSER_FINGERPRINT` constants).
///
/// The TypeScript branch's `+degrade-v2` suffix marks which degraded-file detection logic
/// `pipeline::parse_typescript` uses to classify a file — a change to that logic changes which files this
/// engine reports as `degraded` for byte-identical content, dispatched language, and `size_cap`, so it
/// needs its own fingerprint ingredient scoped to the TypeScript branch (Prisma/lexical-fallback entries
/// are unaffected and should not be invalidated by a TS-only logic change).
///
/// `config.io` is folded in via `{:?}` rather than a `serde_json` serialization, same as
/// [`ruleset_fingerprint`]'s use of `Debug` for `RulePackDef`: `IoOptions` has no `Serialize` impl, and
/// its only field is a plain caller-ordered `Vec<String>` (no `HashMap`), so `Debug` output is
/// deterministic across runs.
pub(crate) fn parser_fingerprint(language: Option<Language>, config: &EngineConfig) -> String {
    let base = match language {
        Some(Language::TypeScript) => {
            format!(
                "{}+degrade-v2+io={:?}",
                zzop_parser_typescript::PARSER_FINGERPRINT,
                config.io
            )
        }
        Some(Language::Prisma) => zzop_parser_prisma::PARSER_FINGERPRINT.to_string(),
        // Own fingerprint (not `LEXICAL_FALLBACK_FINGERPRINT`): `.java` uses the real structural projector.
        Some(Language::Java21) => zzop_parser_java_21::PARSER_FINGERPRINT.to_string(),
        Some(Language::Python) => zzop_parser_python_3::PARSER_FINGERPRINT.to_string(),
        Some(Language::Rust) => zzop_parser_rust::PARSER_FINGERPRINT.to_string(),
        Some(Language::Go) => zzop_parser_go::PARSER_FINGERPRINT.to_string(),
        Some(Language::Sql) => zzop_parser_sql::PARSER_FINGERPRINT.to_string(),
        Some(Language::CSharp) => zzop_parser_csharp::PARSER_FINGERPRINT.to_string(),
        None => LEXICAL_FALLBACK_FINGERPRINT.to_string(),
    };
    format!("{base}+size_cap={}", config.size_cap)
}

/// The `scope` half of a file's `CacheKey` (see module doc, "Scope: the path-identity gap `CacheKey::scope`
/// closes", for the bug this fixes and why both halves — `source_id` and `rel` — are needed). NUL-joined so
/// neither half can bleed into the other (e.g. `source_id = "ab"` + `rel = "c"` must not collide with
/// `source_id = "a"` + `rel = "bc"`), matching the NUL-separator convention already used by
/// [`ruleset_fingerprint`]'s pack-part joining below.
pub(crate) fn cache_scope(config: &EngineConfig, rel: &str) -> String {
    format!("{}\u{0}{rel}", config.source_id)
}

/// The vocabulary-fingerprint component of a file's `CacheKey` — a hash of the run's DECLARED convention
/// vocabulary, present in the IR key as well as the findings key (see `CacheKey::vocabulary_fingerprint`).
///
/// ## One fingerprint over the WHOLE struct, not a per-lane subset
///
/// Individual vocabulary keys land in different lanes: `ormReceiverPattern` reaches the cached IR,
/// `moneyTokens` reaches the cached findings, `authGuardPattern` reaches only the uncached whole-graph
/// pass. Keying each lane on just the subset it reads would be precise — and would be a standing drift
/// hazard, because that subset is a hand-maintained CLAIM about which lane consumes what, and nothing
/// makes the claim fail when a consumer moves. The two errors are not symmetric: over-invalidating costs
/// a recompute, under-invalidating serves a WRONG answer from a warm cache. `packs.disabled` already sets
/// this precedent — it is folded into `ruleset_fingerprint` wholesale rather than per-pack.
///
/// ## The digest input is the serialized struct, so a NEW key is covered by construction
///
/// `VocabularyConfig` derives `Serialize` with every field always emitted (no `skip_serializing_if`), so
/// `serde_json` output is a total function of its field list: adding the next vocabulary key changes this
/// hash without anyone remembering to extend a list here. That is the whole point of hashing the wire
/// shape rather than a hand-picked tuple — a hand-picked tuple is the same drift hazard as a per-lane
/// subset, one level down. JSON (not `Debug`) because `serde_json` is already this crate's canonical
/// serializer for config-shaped values and the struct's own field ORDER, which `Debug` and `serde` both
/// follow, is what makes either deterministic.
///
/// Consequence worth stating plainly: this hashes what was DECLARED, not what was RESOLVED, so a request
/// that omits `vocabulary` entirely and a request that declares exactly the built-ins are two different
/// fingerprints for one behavior. That is over-invalidation, which is the safe direction; the product
/// front end (`zzop-config`) injects `VocabularyConfig::built_in()` into every request, so the CLI/MCP
/// lanes are stable and only a raw-facade embedder can straddle the two.
pub(crate) fn vocabulary_fingerprint(config: &EngineConfig) -> String {
    let declared = serde_json::to_string(&config.vocabulary).unwrap_or_default();
    AnalysisCache::content_hash(declared.as_bytes())
}

/// The ruleset-fingerprint half of a file's `CacheKey`, over the already `is_enabled`-filtered pack set
/// `run_file_pass` computes once per `analyze_tree` call (see module doc for the composition and the
/// deviations from the spec's literal "serialized JSON" wording).
pub(crate) fn ruleset_fingerprint(enabled_packs: &[&RulePackDef], config: &EngineConfig) -> String {
    let mut pack_parts: Vec<String> = enabled_packs
        .iter()
        .map(|pack| format!("{}\u{0}{pack:?}", pack.id))
        .collect();
    pack_parts.sort();

    let mut disabled_sorted = config.rule_config.disabled_rules.clone();
    disabled_sorted.sort();
    let disabled_json = serde_json::to_string(&disabled_sorted).unwrap_or_default();

    let schema_structural_fingerprint = schema_structural_fingerprint();
    let combined = format!(
        "{}\u{1}{disabled_json}\u{1}{schema_structural_fingerprint}\u{1}{DSL_INTERPRETER_FINGERPRINT}",
        pack_parts.join("\u{0}")
    );
    AnalysisCache::content_hash(combined.as_bytes())
}

/// Deterministic hit/miss counters for `AnalyzeOutput::cache`, safe to share (by shared reference) across
/// `pipeline::run_file_pass`'s `rayon::par_iter` — atomics rather than a `Mutex<CacheStats>` since the two
/// counters never need to be updated together atomically (each `process_file` call touches at most one of
/// them, exactly once).
#[derive(Default)]
pub(crate) struct CacheCounters {
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl CacheCounters {
    pub(crate) fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn into_stats(self) -> CacheStats {
        CacheStats {
            hits: self.hits.into_inner(),
            misses: self.misses.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests;
