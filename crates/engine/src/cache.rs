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
/// `v16` -> `v17`: `FileIrSlice` gains `controller_prefix_route_fragments` (`controller-prefix-ref-v1`) — a stale entry defaulting it to empty would silently drop a `@Controller(RouteKey.Asset)`-shaped controller's routes on every warm run.
///
/// `v17` -> `v18`: `FileIrSlice` gains `loop_spans` (`loop-spans-v1`) — a stale entry defaulting it to empty would silently starve `Matcher::MethodScan::trigger_in_loop` of loop evidence on every warm run.
///
/// `v18` -> `v19`: `FileIrSlice` gains `class_shape_fragments` and its `io` payload gains optional `IoConsume::body`/`IoProvide::body` shapes (`body-shape-v1`) — a stale entry defaulting them to empty/`None` would silently starve `cross-layer/body-field-drift` of both sides' evidence.
///
/// `v19` -> `v20`: the `io` payload's `IoConsume` gains the optional `client` provenance tag
/// (`axios-defaults-base-v1`) — a stale entry defaulting it to `None` would silently exempt that
/// file's axios call sites from the assemble-time `axios.defaults.baseURL` path-prefix seam on
/// every warm run (client-scoped seams skip untagged consumes rather than guess).
///
/// `v21` -> `v22`: `FileIrSlice`'s `router_mount_fragments` gain the `express-middleware-v1` shape —
/// `RouterMountEntry::Verb`/`Mount` each gain an `attr_keys: Vec<String>` field and the enum gains a
/// new `ScopedAttr` variant. A stale entry defaulting `attr_keys` to empty (via `#[serde(default)]`)
/// would silently drop a recognized Express middleware guard's judgment on every warm run instead of
/// projecting it into the composed `AttributeStore` — same "missing field silently defaults" hazard
/// this doc's own opening paragraph describes.
///
/// `v22` -> `v23`: `dispatch::Language` gains the `Rust` variant (`.rs` now routes to a real structural
/// parser, `zzop_parser_rust`) — a stale entry for a `.rs` file was cached under the PRIOR (lexical-
/// fallback) fingerprint/projection and would otherwise be served back on a warm run instead of being
/// recomputed with real symbols/imports/io, for any tree that contains `.rs` files.
///
/// `v23` -> `v24`: `dispatch::Language` gains the `Go` variant (`.go` now routes to a real structural
/// parser, `zzop_parser_go`) — a stale entry for a `.go` file was cached under the PRIOR (lexical-
/// fallback) fingerprint/projection and would otherwise be served back on a warm run instead of being
/// recomputed with real symbols/imports/io, for any tree that contains `.go` files.
///
/// `v24` -> `v25`: `dispatch::Language::JavaLexical` is replaced by `Language::Java21` (`.java` now
/// routes to a real structural parser, `zzop_parser_java_21`, tree-sitter-backed) — a stale entry for a
/// `.java` file was cached under the PRIOR (lexical brace-matcher) fingerprint/projection and would
/// otherwise be served back on a warm run instead of being recomputed with real symbols/imports/io. The
/// Java `FileIrSlice` projection changes shape drastically: `imports` goes from always-`None` to a real
/// `ImportMap` (dotted/glob/static specifiers), `symbols` go from unqualified lexical spans to
/// AST-derived, dot-qualified (`Outer.Inner.method`), REAL-visibility (`exported` reflects
/// public/protected/package-private/private, not always `true`) symbols — none of which a `#[serde(
/// default)]`-style migration could safely backfill from an old entry.
/// `v25` -> `v26`: no field gained/renamed/removed, but every cached DSL finding's `message` content
/// changed — `pipeline::findings::eval_packs` now appends `zzop_core::disable_hint`'s config-disable
/// fragment to each DSL finding's message before it reaches `AnalysisCache::put_findings` (D13①). A
/// pre-existing cache entry's `findings` still deserializes cleanly (same `Finding` shape, `message` is
/// still a plain `String`) — the usual "old entry satisfies the new shape" case this doc's opening
/// paragraph says does NOT need a bump — except here staleness is invisible AND user-facing: a warm run
/// would silently serve the OLD hint-less message forever for any unchanged file, while a cold run (or
/// any file whose content changes even slightly) gets the new hint, splitting one tree's findings between
/// two message shapes depending on cache luck. The bump forces every existing entry to recompute once.
///
/// `v26` -> `v27`: batch-wide cached-content changes (2026-07-17) — (1) the unknown-`disabledRules`/
/// `severityOverrides`-id diagnostics moved from the `warnings` channel to the new `configWarnings`
/// channel, (2) DSL rule message texts in `rules/dsl` were revised, and (3) `packsLoaded` entries
/// gained the per-pack `filesInScope` count. Same invisible-staleness class as `v25` -> `v26`: an old
/// entry still deserializes, but a warm run would serve pre-change message text forever, splitting one
/// tree's output between two shapes depending on cache luck.
///
/// `v27` -> `v28`: second blind-round-3 fix batch (2026-07-17) — (1) the unresolved-rule-id diagnostics'
/// "N entry/entries" double-word form was replaced with correctly-pluralized text, (2) several
/// `ir.io`-pointing disclosure texts gained a "where it's actually reachable" parenthetical, (3) the
/// `cross-layer/unconsumed-endpoint`/`unconsumed-mutation-endpoint` findings gained an appended
/// extraction-blindness caveat sentence when a sibling tree contributed zero joinable io, and (4) a new
/// framework-silence tripwire (S6, `orm_schema_silence_warning`) can now fire a brand-new warning class
/// that never existed in any older cache entry. Same invisible-staleness class as `v25` -> `v26`/`v26` ->
/// `v27`: a warm run would otherwise serve pre-change text (or simply never emit the new S6 warning at
/// all) forever for any unchanged file.
///
/// `v28` -> `v29`: blind-field-test fix batch W2 (2026-07-17) — (1) several DSL pack `file_pattern`s
/// were narrowed to stop over-broad cross-language false-fires (a pack scope change can change which
/// files a rule even considers, so a cached finding computed under the OLD broader scope may no
/// longer be correct), and (2) `zzop-facade`'s pack-merge chokepoint now emits a new shadow-warning
/// class when a same-id pack replaces an already-loaded one whole. Same invisible-staleness class as
/// the prior bumps: a warm run would otherwise keep serving findings scoped/warned under the old
/// rules forever for any unchanged file.
///
/// `v29` -> `v30`: blind-field-test fix batch W3-A (2026-07-17) — the overlay synthetic-entry warning
/// text changed and the unparsed-extension warning's Mode A/B wording was corrected; same
/// invisible-staleness class as the prior bumps (a warm run would otherwise keep serving OLD text).
/// `v30` -> `v31`: `Language` gains `Sql` (`.sql` -> `zzop_parser_sql` db-table provides) — same bump class as Rust/Go above.
///
/// `v31` -> `v32`: the S5/S7 framework-silence tripwires gained a PER-APP census (`builtin_fetch_census`/
/// `fetch_wrapper_census`) + internal-intent recount — a brand-new app-scoped warning CLASS and a changed
/// set of counted `fetch(`/wrapper sites (absolute-URL/bare-const egress no longer counts; per-app gating
/// now fires on a dark app a healthy sibling used to mask). Same invisible-staleness class as v27 -> v28's
/// S6 bump: a warm run would otherwise serve pre-change (or entirely missing) warnings forever.
pub const CACHE_SCHEMA_VERSION: &str = "zzop-cache-v32";

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
///
/// `v3` -> `v4`: `MethodScan` gains `trigger_in_loop` — a structural containment gate against
/// `SourceFile::loop_spans` that changes which trigger occurrences a rule fires on for byte-identical
/// source and pack content.
const DSL_INTERPRETER_FINGERPRINT: &str = "dsl-interpreter-v4";

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
        // structural `zzop_parser_java_21` projector crate, not this engine's own text-only heuristics.
        Some(Language::Java21) => zzop_parser_java_21::PARSER_FINGERPRINT.to_string(),
        Some(Language::Python) => zzop_parser_python_3::PARSER_FINGERPRINT.to_string(),
        Some(Language::Rust) => zzop_parser_rust::PARSER_FINGERPRINT.to_string(),
        Some(Language::Go) => zzop_parser_go::PARSER_FINGERPRINT.to_string(),
        Some(Language::Sql) => zzop_parser_sql::PARSER_FINGERPRINT.to_string(),
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
mod tests;
