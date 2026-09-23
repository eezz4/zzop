//! WHY a file fell back to the lexical projection.
//!
//! Split out of `super` on 2026-09-06 when a fourth arm arrived and the parent file was already at the
//! 300-line cap. The move is not cosmetic: the doc below is the closure argument for the whole set, and
//! a closure argument that lives in the middle of a 300-line module is one nobody re-reads when they add
//! an arm — which is exactly the edit that was being made.

use crate::dispatch::Language;
use crate::EngineConfig;

use super::fresh::{exceeds_recursion_caps, is_oversized, RecursionNeedle};

/// WHY a file fell back to the lexical projection — the four, and only four, ways `FileArtifact` can be
/// degraded. Each arm is a different LEVER for the caller, which is the whole reason the fact is carried
/// instead of collapsed into a bool: an oversized file is a `size_cap` decision the caller can change, an
/// unreadable one is an environment fault, a parse failure is a bug report or an unsupported syntax
/// level, and a nesting-capped one is a refusal this engine makes BEFORE any parser runs.
/// `analyze::diagnostics::degraded_files` is the consumer that turns them back into those sentences.
///
/// The set is closed by construction, not by convention: the ONLY places that build a degraded
/// `FileArtifact` are the read-error early return in [`super::artifact::process_file`], the oversized and
/// nesting branches and the parse-verdict tail of [`super::fresh::compute_fresh_artifact`], and
/// [`super::artifact::artifact_from_ir`]'s warm-cache reconstruction — which derives the same verdict
/// from the same predicates rather than remembering one (see its doc for why that cannot drift).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DegradeCause {
    /// `fs::read` failed — a permission error, or a race with a concurrent delete/replace. Nothing ran on
    /// this file at all: no parse, no rule of any kind, and `loc` is 0.
    Unreadable,
    /// The file's byte length exceeded `EngineConfig::size_cap`, so no parser was invoked. `loc` is still
    /// counted lexically and line-scan DSL rules still ran against the raw text.
    Oversized,
    /// The input would have recursed a parser past what a stack can hold, so no parser was invoked.
    /// Same lexical fallback as `Oversized`, and for the same reason stated one level up: this is a
    /// REFUSAL, not a failure. It is its own arm rather than folded into `ParseFailure` because the two
    /// send the reader to opposite places — a parse failure is a bug report against a frontend, while
    /// this one says the input is outside a declared bound and names the bound.
    ///
    /// 🔴 Named for the DANGER, not for one of its shapes. It was `TooDeeplyNested` while the gate
    /// counted brackets alone; then a binary-operator chain — bracket depth ZERO — turned out to nest a
    /// recursive-descent parser just as hard, and every sentence built on the old name became a lie
    /// about the file in front of the reader. The bounds live in [`super::fresh::recursion`].
    PastRecursionCap(RecursionNeedle),
    /// A parser was invoked for this file's language and did not produce a usable tree (or panicked, which
    /// every frontend here catches and treats as the same verdict). Same lexical fallback as `Oversized`.
    ParseFailure,
}

/// The warm-cache half of the degrade-cause verdict: the cached slice remembers THAT a file degraded
/// (`FileIrSlice::degraded`) but not why, so the reason is re-derived here rather than added to the
/// cached payload.
///
/// **Re-deriving cannot disagree with the cold run's verdict, and that rests on a pinned invariant
/// rather than on care.** The oversize test is the same call the cold gate makes
/// ([`is_oversized`] — one predicate, not a copy), applied to the same `bytes`, because an IR hit means
/// the content hash matched. Its other input, `size_cap`, is folded into `parser_fingerprint` and so
/// into every `CacheKey` (`crate::cache`'s module doc says why; `cache::tests::
/// parser_fingerprint_changes_with_size_cap` pins it) — a run under a different cap cannot hit this
/// entry at all. The nesting test below it has the same property for a different reason: it reads only
/// the bytes and the language, both of which an IR hit has already matched. So the three tests run in
/// the cold path's order and the last one is the residue — "degraded, not oversized, not over the
/// nesting cap" leaves exactly one possibility, the parser verdict.
///
/// ⚠ **This paragraph is the thing that goes stale when a fifth cause arrives.** It is an exhaustion
/// argument, not a description: adding a pre-parse gate without adding its test here silently
/// re-labels those files as parse failures — a bug report against a frontend that did nothing wrong.
///
/// `Unreadable` is structurally absent here: that path returns before any cache lookup, since a file
/// with no bytes has no content to hash.
///
/// The alternative — a `degrade_cause` field on `FileIrSlice` — was not taken. It would put an
/// engine-side enum in `zzop-cache` (a `zzop-core` leaf today) and move `CACHE_SCHEMA_VERSION`, i.e.
/// cold-start every existing cache, to store a value that is a pure function of two things the caller
/// already holds in hand.
pub(super) fn cached_degrade_cause(
    degraded: bool,
    bytes: &[u8],
    language: Option<Language>,
    // The gate reads the PATH too, because dispatch is not the only way a parser reaches a file: a
    // pre-scan host is dispatch-`None` and parsed anyway. Same predicate, same inputs, both lanes.
    rel: &str,
    config: &EngineConfig,
) -> Option<DegradeCause> {
    if !degraded {
        return None;
    }
    if is_oversized(bytes, config) {
        return Some(DegradeCause::Oversized);
    }
    // Same predicate the cold path used, asked again rather than cached — see `DegradeCause`'s doc. The
    // allocation is paid only for a file that is already degraded and not oversized, which is rare.
    if let Some(needle) = exceeds_recursion_caps(&String::from_utf8_lossy(bytes), language, rel) {
        return Some(DegradeCause::PastRecursionCap(needle));
    }
    Some(DegradeCause::ParseFailure)
}
