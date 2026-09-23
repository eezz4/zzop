//! Shared regex-idiom fragments for `${NAME}` references inside `RulePackDef` pattern fields — see
//! `RulePackDef::expand_fragments`'s doc for the full mechanism. This module owns the SHARED bundled set
//! (idioms duplicated across many packs, e.g. the "skip test/story/config files" path exclusion) plus
//! the low-level sentinel-detection/error types both the disk loader and the inline `packDefs`
//! deserialize boundary share.
//!
//! ## Why this is a Rust const, not a `rules/dsl/_fragments.json` file read off disk
//! `rules/dsl/**` is walked wholesale by two OTHER mechanisms that have no notion of "this file isn't a
//! pack": `zzop_config`'s `build.rs` (embeds every `*.json` under `rules/dsl` as a bundled pack source,
//! recursively) and `pack_loader::load_dsl_packs` itself (treats every top-level/depth-1 `*.json` in
//! whatever directory it's pointed at as a candidate pack). A flat `{name: regex}` file dropped anywhere
//! in that tree would get swept into `BUNDLED_PACK_SOURCES` and, downstream, fail `RulePackDef`
//! deserialization (missing `id`/`rules`) at the wire boundary (`zzop_config::mapper::parse_pack_defs`
//! parses it as a permissive `serde_json::Value`, so it would NOT be caught there — it would sail
//! through as a bogus `packDefs` entry and blow up the first time an actual `AnalyzeRequest`/
//! `EnvelopeAnalyzeRequest` tries to deserialize it as a `RulePackDef`) — not a safely-skipped warning,
//! but a hard failure for any real request. So the shared set lives here instead: `shared_fragments.json`,
//! sitting beside this module under `crates/core/src/dsl/` (outside `rules/dsl` entirely), embedded at
//! compile time via `include_str!` and parsed once. Both `pack_loader::parse_dsl_pack` (disk load +
//! validator + bundled-pack parsing, all funnel through it) and the inline `packDefs` path
//! (`RulePackDef::expand_fragments`, called from `zzop-facade`'s `base_engine_config`) resolve `${NAME}`
//! against this exact same map — no filesystem dependency at runtime, no drift between the two paths.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The shared fragment bundle's JSON source — one `{name: regex}` object, hand-edited, checked in.
const SHARED_FRAGMENTS_JSON: &str = include_str!("shared_fragments.json");

static SHARED_FRAGMENTS: OnceLock<BTreeMap<String, String>> = OnceLock::new();

/// The shared fragment bundle, parsed once. Panics on first access if `shared_fragments.json` is not a
/// valid `{name: regex}` JSON object — a committed-file invariant, not something a pack author's input
/// could ever trigger at runtime.
pub(crate) fn shared_fragments() -> &'static BTreeMap<String, String> {
    SHARED_FRAGMENTS.get_or_init(|| {
        serde_json::from_str(SHARED_FRAGMENTS_JSON).expect(
            "crates/core/src/dsl/shared_fragments.json must be a valid {name: regex} object",
        )
    })
}

/// The shared `test-paths` fragment, compiled once — the ONE owner of "is this path a test path"
/// for consumers OUTSIDE pattern expansion. Reading the fragment rather than spelling a
/// second regex is the point: the DSL packs' `${test-paths}` exclusions and every other layer's
/// classification can never disagree about what a test path is, because there is one string.
///
/// Two consumers today, and the second one is why the arms are what they are:
/// * the summary layer's first-screen ordering of test-path findings (the 2026-08-09 U78 ruling), and
/// * [`crate::paths::is_test_file`] — the native/cross-layer "not deployed" predicate, which carried a
///   SECOND arm table until 2026-08-10. That table knew `_test.go` / `test_*.py` / `*Tests.cs` /
///   `FooTest.java` and this fragment did not, so the 132 bundled rules that excluded a shared
///   `${test-paths…}` name (both figures measured 2026-08-10, when 144 rules shipped; v0.30.0 has
///   since exported 17 of them)
///   judged idiomatic Go, Python and C# test files as production code. `is_test_file`'s doc carries the
///   measurement and the two case-sensitivity conflicts the merge had to settle.
pub fn test_path_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(&shared_fragments()["test-paths"]).expect(
            "the shared test-paths fragment must be a valid regex (committed-file invariant)",
        )
    })
}

/// The name prefix of every shared fragment that IS the "decline test code" vocabulary — `test-paths`
/// plus its `test-paths-stories` / `test-paths-migrations` extensions.
pub(crate) const TEST_PATH_FRAGMENT_PREFIX: &str = "test-paths";

/// True when `value` is, byte for byte, one of the shared `test-paths*` fragment bodies — i.e. this
/// pattern got here by a rule writing `"file_exclude_pattern": "${test-paths…}"` and the expansion pass
/// substituting it.
///
/// ## Why a VALUE comparison and not a flag recorded during expansion
/// A `bool` on `RuleDef` would be the direct encoding, and it was the first shape tried. It is worse in
/// two concrete ways. It changes `{pack:?}`, which is the cache-fingerprint input and the thing
/// `tests_fragments::byte_identity` pins — an already-expanded pack (every `*_pre_migration.json`
/// fixture, and any embedder that spells the pattern out) would carry `false` where the identical
/// live pack carried `true`, so two packs that behave identically would stop being identical. And it
/// would be a field on the public `RuleDef` that no pack author may write, which is a wire surface
/// created for an internal bookkeeping need. The value comparison has neither problem and answers the
/// same question: is what this rule excludes the shared vocabulary, or something of its own?
///
/// ## Residual, stated plainly — and the count is NOT stated here
/// A pack-local `test-paths-*` fragment is NOT recognized here, and cannot be: `expand_fragments`
/// clears `RulePackDef::fragments` once it has substituted, so by the time a run reaches this the
/// pack-local body is a string with no name attached and no way back to one. Such a rule keeps every
/// built-in language convention (that is what `tests_fragments::superset` pins) and does not pick up a
/// project's `vocabulary.extraTestPathPatterns` tail.
///
/// 🔴 WHICH rules are in that position is `tests_fragments::name_census`'s to say, never this
/// paragraph's. It used to read "Exactly ONE bundled rule is in that position today", naming
/// `reliability/sync-fs-in-handler` — and it stayed that way after `security/secret-env-in-fe` joined
/// on 2026-08-25, which the census row for `test-paths-stories-next-server` records IN WRITING as the
/// "second instance". The count had two owners and the one with no test went stale; worse, the same
/// sentence called the residual "ONE INFO-LEVEL rule" and both of them are `warning`, so the severity
/// that made the under-reach sound cheap was never measured either (review ledger V235).
/// Recount, in one line:
/// `python3 -c "import json,glob;print([(p,k) for p in glob.glob('rules/dsl/**/*.json',recursive=True)
/// for k in (json.load(open(p)).get('fragments') or {}) if k.startswith('test-paths')])"`
///
/// The DIRECTION is what this paragraph is actually for, and it does not depend on the count: an
/// under-reach, never a wrong exclusion. Such a rule still judges a directory the project declared as
/// test surface; nothing is silently skipped. Both `superset` and `name_census` are triage moments
/// where a new such fragment has to be looked at, so the set cannot grow unnoticed — which is exactly
/// how the second instance came to be recorded there while this sentence went on naming one. Closing it
/// properly means carrying the resolved NAME forward from expansion, and the cheap encoding of that — a
/// `bool` on `RuleDef` — is what the section above rejects; the honest fix is a fragment mechanism that
/// can express "base plus one arm" by reference, which is the same missing feature `superset`'s header
/// opens with.
pub(crate) fn is_shared_test_path_vocabulary(value: &str) -> bool {
    shared_fragments()
        .iter()
        .any(|(name, body)| name.starts_with(TEST_PATH_FRAGMENT_PREFIX) && body == value)
}

/// Does this expanded pattern DECLINE TEST PATHS — as opposed to declining some other path class?
///
/// The looser sibling of [`is_shared_test_path_vocabulary`], and the two are not interchangeable. That
/// one asks about PROVENANCE ("did a `${test-paths…}` ref put this exact string here?") because the
/// expansion pass needs to know which values it produced. This one asks about BEHAVIOR ("does this
/// pattern turn test paths off?"), so it accepts a value that merely CONTAINS a shared body — a pattern
/// composed as the vocabulary plus an extra alternative still declines every test path the vocabulary
/// names, and a caller reasoning about what a rule skips must count it.
///
/// ## Why this predicate exists at all
/// `file_exclude_pattern` was, for the whole life of the bundled packs, a synonym for "skip test files":
/// every use of it was a `${test-paths…}` ref. A guard written in that period could and did read the
/// field's mere PRESENCE as the test-path decision (`zzop_facade`'s CLAUSE D), which held only while the
/// synonymy did. It stopped holding on 2026-08-26, when `security/config-file-secret` took a
/// `file_exclude_pattern` that declines TRANSLATION CATALOGUES (`locales/<lang>/…`, `i18n/<lang>.json`)
/// and has nothing to do with test code. Asking the question directly is what keeps the guard's meaning
/// attached to its name.
///
/// ## Residual
/// A HAND-ROLLED test-path exclusion — one that spells `\.test\.tsx?$` itself instead of referencing the
/// shared vocabulary — reads as `false` here. That is the same residual
/// [`is_shared_test_path_vocabulary`] documents, and it is bounded by the same fact: no bundled rule
/// carries a hand-rolled path exclusion (`vetoed_files`'s doc recounts both sides), and
/// `tests_fragments::name_census` is the triage moment where a new spelling has to be looked at.
pub fn declines_shared_test_paths(value: &str) -> bool {
    shared_fragments()
        .iter()
        .any(|(name, body)| name.starts_with(TEST_PATH_FRAGMENT_PREFIX) && value.contains(body))
}

/// If `value` is EXACTLY `${NAME}` (the whole string, no other characters), returns `NAME`. This is the
/// one collision-safe reference shape this pass supports — no inline substring composition (`"foo ${bar}
/// baz"` is left untouched, a literal regex, never treated as a ref).
///
/// ## Sentinel choice: why `${...}` cannot collide with a real regex
/// Under the `regex` crate's syntax, a bare `{` is only valid immediately after an atom, as a numeric
/// repetition quantifier (`{n}` / `{n,}` / `{n,m}` — digits only). Preceded by `$` (an end-of-line/text
/// anchor — not a quantifiable atom), `{` there is never a valid quantifier position, so `${` followed by
/// non-digit content through to a closing `}` is either a straight-up compile error or, at best, a shape
/// no pack author would ever hand-write as a real pattern. A fragment name is a kebab-case identifier
/// (letters/digits/`_`/`-`), never all-digits, so `${NAME}` can never simultaneously (a) be a value a pack
/// author would legitimately write as a real pattern and (b) compile as one — no shipped pattern can
/// already be an unintentional whole-value match for this shape (verified for every committed pack by
/// `dsl::tests_fragments::byte_identity::no_shipped_pattern_contains_the_sentinel_except_as_an_intended_whole_value_ref`).
pub(crate) fn fragment_ref_name(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("${")?.strip_suffix('}')?;
    if inner.is_empty() {
        None
    } else {
        Some(inner)
    }
}

/// A `${NAME}` reference that failed to resolve — returned by `RulePackDef::expand_fragments`, and (via
/// its `Display`) folded into `pack_loader::parse_dsl_pack`'s ordinary error-string path, so an unknown
/// fragment fails a pack load exactly like a malformed JSON body or a bad `schema_version` does today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentError {
    /// `${NAME}` named a fragment absent from BOTH this pack's own `fragments` map and the shared
    /// bundled set.
    Unknown {
        rule: String,
        field: String,
        name: String,
    },
    /// The fragment `name` resolved to, but its OWN text is itself a whole-value `${...}` reference —
    /// this pass is single-pass/non-recursive by design (see `RulePackDef::expand_fragments`'s doc), so
    /// this is a hard error rather than a silent no-op or an infinite-expansion risk.
    Nested {
        rule: String,
        field: String,
        name: String,
    },
}

impl std::fmt::Display for FragmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FragmentError::Unknown { rule, field, name } => write!(
                f,
                "rule \"{rule}\": `{field}` references unknown fragment \"${{{name}}}\" (not found in \
                 this pack's own `fragments` map or the shared bundled set)"
            ),
            FragmentError::Nested { rule, field, name } => write!(
                f,
                "rule \"{rule}\": `{field}` references fragment \"{name}\", whose own value is itself a \
                 `${{...}}` reference — nested/recursive fragment expansion is not supported"
            ),
        }
    }
}

impl std::error::Error for FragmentError {}
