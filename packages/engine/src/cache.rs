//! Wiring between `zzop_cache::AnalysisCache` and the fused per-file pass (`pipeline.rs`) — implements
//! the cache key/fingerprint design `zzop_cache` defines (see `docs/ARCHITECTURE.md`'s "Caching" for the
//! user-facing contract). This module owns what that design leaves to "the caller": opening the on-disk
//! cache once per `analyze_tree` call (degrade, never panic, on failure), composing the ruleset
//! fingerprint, and the deterministic-under-rayon hit/miss counters behind `AnalyzeOutput::cache`.
//!
//! ## Parser fingerprint composition
//!
//! A file's cache key is `(content hash, parser fingerprint, scope, ruleset fingerprint)`. The parser
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

/// Schema version passed to `AnalysisCache::open` — bump on any change to `FileIrSlice`'s or the
/// findings entry's shape that old entries cannot satisfy (see `AnalysisCache::open`'s own doc: a bump
/// is a bulk wipe, not a per-entry migration). This matters even when `serde(default)` could deserialize
/// an old entry without erroring: a missing field silently defaulting (e.g. an empty `Vec`/`false`) is
/// indistinguishable from "genuinely has none", which would make a cache hit against a pre-existing
/// directory serve a wrong answer instead of a fresh recompute — the schema bump forces a clean cache
/// instead. Bump whenever `FileIrSlice` (or the cached findings shape) gains, renames, or removes a
/// field.
///
/// `v16` -> `v17`: `FileIrSlice` gains `controller_prefix_route_fragments`
/// (`controller-prefix-ref-v1`) — a stale entry defaulting it to empty would silently drop a
/// `@Controller(RouteKey.Asset)`-shaped controller's routes on every warm run instead of projecting them.
pub const CACHE_SCHEMA_VERSION: &str = "zzop-cache-v17";

/// Fingerprint for files that never reach a structural parser crate in the fused pass: no `Language` match
/// (`dispatch::dispatch` returned `None` — unrecognized extension), or the size-cap lexical fallback
/// (`pipeline::compute_fresh_artifact`'s oversized branch, which short-circuits before any language-specific
/// parse call). Both produce their result via this engine's own text-only heuristics
/// (`pipeline::lexical_loc`), never a parser crate, so this is a fixed local marker rather than a borrowed
/// `PARSER_FINGERPRINT` — bump the trailing counter if the engine's own lexical-fallback shape changes.
const LEXICAL_FALLBACK_FINGERPRINT: &str = "lexical/engine-v1";

/// `ruleset_fingerprint`'s native-rule-logic-version token for `pipeline::schema_findings`
/// (`zzop_rules_schema::apply_schema_rules`, wired into the fused per-file pass for Prisma files). Unlike
/// a DSL pack (whose content already changes the fingerprint via `pack:?`), this is Rust logic with no
/// pack content to hash, so the version counter (`zzop_rules_schema::STRUCTURAL_RULES_VERSION`) lives
/// beside the rule itself and is bumped whenever its output shape changes.
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
/// key without this token. Bump the trailing counter on any such change.
const DSL_INTERPRETER_FINGERPRINT: &str = "dsl-interpreter-v3";

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
        // Own fingerprint, distinct from `LEXICAL_FALLBACK_FINGERPRINT`: `.java` goes through the real
        // (if lexical) `zzop_parser_java` projector crate, not this engine's own text-only heuristics.
        Some(Language::JavaLexical) => zzop_parser_java::PARSER_FINGERPRINT.to_string(),
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
mod tests {
    use super::*;
    use zzop_core::RuleConfig;

    fn pack(id: &str) -> RulePackDef {
        let json = format!(r#"{{"id": "{id}", "framework": "any", "rules": []}}"#);
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn ruleset_fingerprint_is_order_independent_over_pack_list() {
        let a = pack("a");
        let b = pack("b");
        let config = EngineConfig::default();
        let fp1 = ruleset_fingerprint(&[&a, &b], &config);
        let fp2 = ruleset_fingerprint(&[&b, &a], &config);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn ruleset_fingerprint_changes_when_pack_set_changes() {
        let a = pack("a");
        let b = pack("b");
        let config = EngineConfig::default();
        let fp_a = ruleset_fingerprint(&[&a], &config);
        let fp_ab = ruleset_fingerprint(&[&a, &b], &config);
        assert_ne!(fp_a, fp_ab);
    }

    #[test]
    fn ruleset_fingerprint_changes_when_disabled_rules_changes() {
        let a = pack("a");
        let mut config = EngineConfig::default();
        let fp_before = ruleset_fingerprint(&[&a], &config);
        config.rule_config = RuleConfig {
            disabled_rules: vec!["something".to_string()],
            ..RuleConfig::default()
        };
        let fp_after = ruleset_fingerprint(&[&a], &config);
        assert_ne!(fp_before, fp_after);
    }

    #[test]
    fn parser_fingerprint_differs_by_language() {
        let config = EngineConfig::default();
        let ts = parser_fingerprint(Some(Language::TypeScript), &config);
        let prisma = parser_fingerprint(Some(Language::Prisma), &config);
        let java = parser_fingerprint(Some(Language::JavaLexical), &config);
        let none = parser_fingerprint(None, &config);
        assert_ne!(ts, prisma);
        assert_ne!(ts, none);
        assert_ne!(prisma, none);
        assert_ne!(java, ts);
        assert_ne!(java, prisma);
        assert_ne!(java, none);
    }

    #[test]
    fn parser_fingerprint_changes_with_size_cap() {
        let mut config = EngineConfig::default();
        let fp1 = parser_fingerprint(Some(Language::TypeScript), &config);
        config.size_cap += 1;
        let fp2 = parser_fingerprint(Some(Language::TypeScript), &config);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn parser_fingerprint_changes_with_io_router_names_for_typescript_only() {
        let mut config = EngineConfig::default();
        let ts_before = parser_fingerprint(Some(Language::TypeScript), &config);
        let prisma_before = parser_fingerprint(Some(Language::Prisma), &config);
        let java_before = parser_fingerprint(Some(Language::JavaLexical), &config);
        let none_before = parser_fingerprint(None, &config);

        config.io.router_names = vec!["customRouter".to_string()];

        let ts_after = parser_fingerprint(Some(Language::TypeScript), &config);
        assert_ne!(
            ts_before, ts_after,
            "an io.router_names change must invalidate cached TypeScript entries"
        );
        // Scoped to the TypeScript branch only — Prisma/Java/lexical-fallback fingerprints never consult
        // `config.io`, so they must be unaffected by an `io` change (no needless invalidation).
        assert_eq!(
            prisma_before,
            parser_fingerprint(Some(Language::Prisma), &config)
        );
        assert_eq!(
            java_before,
            parser_fingerprint(Some(Language::JavaLexical), &config)
        );
        assert_eq!(none_before, parser_fingerprint(None, &config));
    }

    #[test]
    fn cache_scope_differs_by_rel_for_the_same_source_id() {
        // Two different files with identical content/parser/ruleset must not collide on a cache entry.
        let config = EngineConfig::default();
        let a = cache_scope(&config, "routes/a.ts");
        let b = cache_scope(&config, "other/a.ts");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_scope_differs_by_source_id_for_the_same_rel() {
        // The multi-tree-sharing-one-cache_dir case: two trees with the same rel path must not collide
        // either.
        let fe_config = EngineConfig {
            source_id: "fe".to_string(),
            ..EngineConfig::default()
        };
        let be_config = EngineConfig {
            source_id: "be".to_string(),
            ..EngineConfig::default()
        };
        let fe = cache_scope(&fe_config, "src/types.ts");
        let be = cache_scope(&be_config, "src/types.ts");
        assert_ne!(fe, be);
    }

    #[test]
    fn cache_scope_does_not_let_source_id_and_rel_bleed_into_each_other() {
        // NUL-separator regression guard: `source_id = "ab"` + `rel = "c"` must differ from
        // `source_id = "a"` + `rel = "bc"` even though naive concatenation would collide.
        let left_config = EngineConfig {
            source_id: "ab".to_string(),
            ..EngineConfig::default()
        };
        let right_config = EngineConfig {
            source_id: "a".to_string(),
            ..EngineConfig::default()
        };
        let left = cache_scope(&left_config, "c");
        let right = cache_scope(&right_config, "bc");
        assert_ne!(left, right);
    }
}
