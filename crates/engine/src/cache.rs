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
//! The third ingredient is `FP_ENGINE`, a source hash of THIS crate, appended to every arm. The
//! per-language `FP_*` constants each cover one parser crate; the code that turns a parse into a cached
//! `FileIrSlice` or `Finding` lives here, and `crates/engine` cannot be a subject of its own
//! dependents' closures. See [`parser_fingerprint`] and `build.rs` for the subject and its scoping.
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

// Source-derived fingerprints, one per dependency closure — see build.rs. Nothing here is bumped by
// hand, which is the whole point: the hash IS the version.
include!(concat!(env!("OUT_DIR"), "/fingerprints.rs"));

/// Joins a parser's human-readable id (`"rust/syn-2"`) to its source-derived hash. The id is what a
/// person reads in a cache path or a bug report and never needs to move; the hash is what actually
/// invalidates.
fn derived(id: &str, source_hash: &str) -> String {
    format!("{id}/{source_hash}")
}

/// Schema version passed to `AnalysisCache::open` — the cache's one bulk invalidator (that function's
/// doc has the rest: a mismatch wipes, and the test is EQUALITY, not an ordering). **One axis since
/// 2026-08-05**: a hash of the two crates that DEFINE what gets persisted, `zzop-cache` and
/// `zzop-core`, each by Cargo dependency closure. See `build.rs`.
///
/// **A release axis used to be glued to the front, and removing it is the point.** The value was
/// `{workspace_version}+{hash}`, so every upgrade wiped — but that half never did INVALIDATION (the
/// hash covers it), it did HOUSEKEEPING, reclaiming entries orphaned by ordinary edits, because nothing
/// else did. So a release changing no analysis code still charged every user a cold run: `v0.29.0 ->
/// v0.29.1` touched zero bytes of the hashed closure and wiped every cache anyway. What that half
/// genuinely bought — that SOMETHING eventually reclaims, since for a released binary the hash is a
/// constant that never moves — is now `zzop_cache::evict`'s size cap, without the false invalidation.
///
/// **It catches MEANING, not just shape** — which is what a hand-bumped counter kept getting wrong. A
/// real 2026-07-27 case: an undeclared vocabulary stopped falling back to built-ins, leaving
/// `vocabulary_fingerprint`'s hashed struct byte-identical while the slices it addressed were computed
/// differently. That change edited `zzop-core`, so the source hash moves on it — and a NEW file under
/// either crate is in the closure the day it lands, which no watched-file list could manage.
///
/// Over-invalidation is the accepted direction: a comment-only edit in either crate wipes the cache,
/// which costs recomputation, where under-invalidation serves a WRONG ANSWER from a stale entry.
pub const CACHE_SCHEMA_VERSION: &str = CACHE_SCHEMA_VERSION_DERIVED;

/// Human-readable id for files that never reach a structural parser crate in the fused pass: no
/// `Language` match (`dispatch::dispatch` returned `None` — unrecognized extension), or the size-cap
/// lexical fallback (`pipeline::compute_fresh_artifact`'s oversized branch, which short-circuits
/// before any language-specific parse call). Both produce their result via this engine's own
/// text-only heuristics (`pipeline::lexical_loc`), never a parser crate, so there is no
/// `PARSER_FINGERPRINT` to borrow an id from — and no version half either: those heuristics live in
/// this crate, so `FP_ENGINE` already versions them from the shared suffix
/// [`parser_fingerprint`] puts on every arm. (This arm carried its own `FP_LEXICAL`, a hash of
/// `src/pipeline`, until that subject became a strict subset of `FP_ENGINE`'s.)
const LEXICAL_FALLBACK_ID: &str = "lexical";

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
        "schema-structural-{}/{}",
        zzop_rules_schema::STRUCTURAL_RULES_VERSION,
        FP_SCHEMA_RULES
    )
}

/// Logic-version token for the DSL *interpreter* (`zzop_core::dsl`) itself — the same stale-cache gap
/// `schema_structural_fingerprint` closes for native rule logic. Pack JSON already self-invalidates via
/// `{pack:?}` above, but a pure-Rust interpreter semantics change (matcher evaluation, suppress-marker
/// window, ...) alters findings for byte-identical source AND identical pack content — invisible to the
/// key without this token. **Nothing to restamp by hand**: `FP_DSL` is a derived source hash
/// (`crates/engine/build.rs`), so an interpreter change moves it on its own. The instruction that used
/// to sit here — "restamp with the current `CARGO_PKG_VERSION`" — outlived the 2026-07-29 derivation
/// reform and would have had a reader edit a value that is not written by hand.
const DSL_INTERPRETER_FINGERPRINT: &str = FP_DSL;

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

/// The parser-fingerprint half of a file's `CacheKey` (see module doc for the `engine`/`size_cap`/`io`
/// additions beyond the borrowed `PARSER_FINGERPRINT` constants).
///
/// **`+engine=` is a SHARED suffix on every arm, and it is what makes this key honest about who produced
/// the cached bytes.** Each arm's `FP_*` covers only the parser crate that did the *parse*; everything
/// downstream of it — `pipeline/io_projection.rs`, every gate in `pipeline/fresh.rs`, the message text
/// `pipeline/findings.rs` authors into a cached `Finding`, `vocabulary/resolved.rs` — lives in THIS
/// crate, which is structurally absent from those closures because it is the crate that depends on them.
/// Without the suffix, an engine-only bugfix left every warm cache serving the old answer as fresh.
/// `FP_ENGINE`'s subject and the reason it is this crate's own `src` rather than its dependency closure
/// are in `build.rs`.
///
/// A hand-stamped `+degrade-v2` token used to sit on the TypeScript arm, versioning
/// `pipeline::parse_typescript`'s degraded-file classification. That code is under `crates/engine/src`,
/// so `FP_ENGINE` now derives it; the token is gone and nothing in this key is stamped by hand.
///
/// `config.io` is folded in via `{:?}` rather than a `serde_json` serialization, same as
/// [`ruleset_fingerprint`]'s use of `Debug` for `RulePackDef`: `IoOptions` has no `Serialize` impl, and
/// its only field is a plain caller-ordered `Vec<String>` (no `HashMap`), so `Debug` output is
/// deterministic across runs.
pub(crate) fn parser_fingerprint(language: Option<Language>, config: &EngineConfig) -> String {
    let base = match language {
        Some(Language::TypeScript) => {
            format!(
                "{}+io={:?}",
                derived(zzop_parser_typescript::PARSER_FINGERPRINT, FP_TYPESCRIPT),
                config.io
            )
        }
        Some(Language::Prisma) => derived(zzop_parser_prisma::PARSER_FINGERPRINT, FP_PRISMA),
        // Own fingerprint (not `LEXICAL_FALLBACK_FINGERPRINT`): `.java` uses the real structural projector.
        Some(Language::Java21) => derived(zzop_parser_java_21::PARSER_FINGERPRINT, FP_JAVA),
        Some(Language::Python) => derived(zzop_parser_python_3::PARSER_FINGERPRINT, FP_PYTHON),
        Some(Language::Rust) => derived(zzop_parser_rust::PARSER_FINGERPRINT, FP_RUST),
        Some(Language::Go) => derived(zzop_parser_go::PARSER_FINGERPRINT, FP_GO),
        Some(Language::Sql) => derived(zzop_parser_sql::PARSER_FINGERPRINT, FP_SQL),
        Some(Language::CSharp) => derived(zzop_parser_csharp::PARSER_FINGERPRINT, FP_CSHARP),
        None => LEXICAL_FALLBACK_ID.to_string(),
    };
    format!("{base}+engine={FP_ENGINE}+size_cap={}", config.size_cap)
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

/// Shapes the manifest bytes `FP_ENGINE` hashes. Declared here so its tests run and so editing it moves
/// the fingerprint it shapes; its CALLER is `build.rs`, which `include!`s the same file. See its header.
mod manifest_version;
pub(crate) mod surface;
#[cfg(test)]
mod tests;
