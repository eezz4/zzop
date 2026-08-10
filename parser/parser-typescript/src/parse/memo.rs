//! The one-entry, thread-local parse memo behind [`super::parse_with_cm`]. Split out of `parse.rs`
//! on 2026-08-08, the batch that added it, because that file crossed the line ratchet; the cut is
//! along the seam the memo already formed (a self-contained cache with one caller).

use swc_core::common::{sync::Lrc, SourceMap};
use swc_core::ecma::ast::Module;

/// One parsed file, kept so the NEXT caller asking for the same `(file, source)` gets it back instead
/// of re-parsing. See [`LAST_PARSE`].
struct ParseMemo {
    file: String,
    source: String,
    /// `None` is memoized DELIBERATELY. A file swc cannot parse is the pathological case — every
    /// extractor would otherwise re-attempt (and re-`catch_unwind`) the same failure.
    parsed: Option<(Lrc<SourceMap>, Lrc<Module>)>,
}

thread_local! {
    /// A ONE-ENTRY, THREAD-LOCAL memo of the most recent parse.
    ///
    /// Why one entry is the right size. The engine's per-file lane (`pipeline::fresh`) hands the same
    /// `(rel, text)` to every extractor in turn, so the repeated parses are always CONSECUTIVE and a
    /// single slot collapses them all. A larger cache would hold files nobody is going to ask about
    /// again and turn a bounded cost into an unbounded one.
    ///
    /// Why thread-local rather than process-global. The file pass is `par_iter`, so a global would need
    /// a lock on the hot path AND would outlive the run — a long-lived `zzop-mcp` process would keep
    /// one file's AST alive forever. Thread-local keeps peak memory on the axis this repo's memory
    /// doctrine already states (`RSS ∝ threads × per-file cap`) instead of adding a new one.
    ///
    /// Why the key holds the SOURCE and not a hash of it. A hash collision here would hand back the
    /// WRONG AST — a silent wrong answer, which is the exact failure class this repo pays the most to
    /// avoid. The comparison is length-first, so the full compare only runs on a genuine hit.
    static LAST_PARSE: std::cell::RefCell<Option<ParseMemo>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn parse_module(file: &str, source: &str) -> Option<Lrc<Module>> {
    parse_with_cm(file, source).map(|(_, m)| m)
}

/// The parsed module for `(file, source)`, from the one-entry thread-local memo when it holds this
/// exact input and from swc otherwise.
///
/// ## Why reusing a `Module` across `GLOBALS` scopes is safe here
///
/// Each swc parse below opens a fresh `GLOBALS.set(&Globals::new(), …)` scope, and a `Module`'s spans
/// carry a `SyntaxContext` that is only meaningful inside the scope that created it. That would matter
/// if anything read one — but **this crate never does**: it runs no transform, resolves no hygiene
/// mark, and names no `SyntaxContext`/`Mark` anywhere (`no_hygiene_dependency_pin` below asserts it,
/// so the claim cannot rot into prose).
///
/// The decisive point is that the memo adds NO new exposure even if that ever changed: the `Module`
/// already escapes its `GLOBALS` scope today — the scope closes on the line before the return — so
/// every caller has always been holding a module whose contexts are out of scope. Caching hands back
/// the same kind of value, just without paying for it again.
pub(crate) fn parse_with_cm(file: &str, source: &str) -> Option<(Lrc<SourceMap>, Lrc<Module>)> {
    if let Some(hit) = LAST_PARSE.with(|slot| {
        let slot = slot.borrow();
        let memo = slot.as_ref()?;
        (memo.file == file && memo.source.len() == source.len() && memo.source == source)
            .then(|| memo.parsed.clone())
    }) {
        return hit;
    }
    let parsed = super::parse_uncached(file, source);
    LAST_PARSE.with(|slot| {
        *slot.borrow_mut() = Some(ParseMemo {
            file: file.to_string(),
            source: source.to_string(),
            parsed: parsed.clone(),
        });
    });
    parsed
}
