//! The parser-recursion gate: a pre-parse refusal for input that would take the process down with it.
//!
//! ## The defect this exists for (2026-09-06, external review lane C)
//!
//! `export const x = ` followed by 1,000 `(` — **1,017 bytes** — killed the whole run:
//! `thread '<unknown>' has overflowed its stack`, exit 127, zero bytes of stdout. No finding, no
//! warning, no `degraded` row: every other file in the tree went unreported, and on an MCP host the
//! server simply disappears from the tool list. Measured floor: depth 600 exits 0 with `degraded: 1`,
//! depth 700 aborts.
//!
//! Three defences were already in place and none of them reaches this input:
//!
//! 1. `EngineConfig::size_cap` (1.5 MB by default) is **1,500× larger** than the file.
//! 2. The `catch_unwind` wrappers in [`super::super::parsers`] are irrelevant — a stack overflow
//!    ABORTS, it does not unwind, so there is nothing to catch. That wrapping was built for the 2026-07-29
//!    incident, which was a panic; this is the same blast radius through a different failure mode.
//! 3. `parser/tests/input_strategy.rs` explicitly scopes nesting depth out ("a separate question with a
//!    separate answer") — and no such answer existed anywhere in the tree. This module is that answer.
//!
//! 🔴 **This module doc used to say "only the TypeScript frontend dies: the other seven return
//! `degraded: 1` at depth 100,000", and that was false from the day it was written** (2026-09-07,
//! external review round 12, review ledger V99). Measured, one file per tree: Java aborts at nested-paren
//! depth 5,000, Go / Rust / C# at 20,000, and C#'s whole-project pass at 1,000. The claim survived
//! because the test below asserted it without ever running a parser — see its own note.
//!
//! Two things carry the class now, and they cover different halves of it:
//!
//! 1. [`super::super::deep_stack`] gives every thread that parses 64 MiB of stack, which moves the abort
//!    floor far past anything a repository holds. That is what handles the shapes this gate cannot see:
//!    `<`-nesting and long binary-operator chains are not bracket depth, and a TypeScript file dies on
//!    both while walking straight through the count below.
//! 2. THIS gate, now applied to EVERY language rather than TypeScript alone, refuses the bracket-shaped
//!    pathology outright — so a file past the cap comes back as `degraded: 1` with a named cause instead
//!    of taking the process down. Headroom alone is not enough: Rust `{{{…}}}` at depth 20,000 still
//!    overflows 64 MiB, and a refusal is the honest answer for a 39 KB file that is nothing but braces.
//!
//! ## Why the cap is 256, and how to re-derive it
//!
//! Two numbers bracket it, both measured rather than chosen:
//!
//! - **Above**: the abort floor is between 600 and 700 (bisected with the reproduction above).
//! - **Below**: the deepest nesting any real file reaches. Across **19,399** `.ts/.tsx/.mts/.cts/.js/
//!   .jsx/.mjs/.cjs` files under `cases/`, `corpus/`, `site-src/` and `packages/` — grafana, django,
//!   astro, aspnetcore, prometheus and the rest — the maximum is **29**, with p1 = 15 and p50 = 6. The
//!   deepest three are a 1.9 MB aspnetcore bundle (29), a grafana test (28) and the vendored yarn
//!   release (27).
//!
//! 📅 **Re-derived for all eight languages (2026-09-08), because the gate stopped being TypeScript-only.**
//! Same scan over **56,790** files under `corpus/`, `cases/`, `crates/`, `parser/`, `rules/`,
//! `packages/` and `site-src/` — maximum depth by language: rust **99** (a `clap` builder test),
//! ts/js 39, go 36, csharp 32, python 19, java 19, sql 7, prisma 3. So 256 sits **2.6×** above the
//! deepest real file in any language, and the deepest is not TypeScript.
//!
//! 256 sits ~8.8× above every real JS/TS file measured, 2.6× above the deepest file in any language,
//! and — since the stack reserve landed — far below the abort floor rather than 2.3× below it. Re-derive the lower
//! bound by running [`scan_depths`] over a corpus; the upper bound by bisecting the reproduction.
//!
//! 🔴 **The scan must skip strings, templates, comments and regex literals, and that is not a detail.**
//! A naive bracket count reports **563** for one real grafana file (`alerting/unified/search/search.js`),
//! which would leave no usable window between real code and the abort floor at all. That file's actual
//! nesting is **4**, and it analyzes fine today; the other 559 brackets are inside string and regex
//! bodies. Measuring the wrong thing here does not make the cap conservative — it makes it impossible.

/// The scan itself: one lexer-shaped pass that answers both caps. Split out of this file on
/// 2026-09-08 — see that module's doc for why the seam is where it is.
mod scan;
pub(crate) use scan::{scan_depths, RecursionNeedle, StatementEnd};

use crate::dispatch::Language;

/// Bracket nesting past which no parser is invoked. See the module doc for the two measurements that
/// bracket this number; it is not a taste call and must not be moved without re-taking both.
pub(crate) const MAX_NESTING_DEPTH: usize = 256;

/// The deepest bracket nesting any real file reaches, over the eight-language census the module doc
/// describes (56,790 files; the maximum is a `clap` builder test in Rust, not TypeScript).
///
/// Kept beside the cap so the two move together: re-taking the census means updating this, and the
/// assertion below then says whether the cap still clears it.
pub(crate) const DEEPEST_REAL_FILE_MEASURED: usize = 99;

// The cap must sit above the deepest real file, or this gate refuses code someone wrote on purpose.
// A `const` assertion rather than a test: both sides are constants, so this fails the BUILD the moment
// someone lowers the cap below the census — a test would only fail when someone ran it.
const _: () = assert!(
    MAX_NESTING_DEPTH > DEEPEST_REAL_FILE_MEASURED,
    "the nesting cap dropped to or below the deepest real file measured -- re-derive BOTH bounds \
     (the module doc says how) before moving either"
);

/// Length of a binary-operator chain past which no parser is invoked — the SECOND needle, and the
/// reason there is one.
///
/// Brackets are not the only thing that nests a recursive-descent parser. `a + a + a + …` is
/// left-associative, so every operator is another frame, and the bracket count of such a file is
/// ZERO. Measured on this HEAD before this cap existed: a TypeScript file of 100,000 terms (400 KB)
/// analyzed fine; **150,000 terms (600 KB) overflowed the stack** — under the 1.5 MB size cap, with
/// the bracket count returning 0, so nothing refused it. 🔴 And its failure shape was worse than
/// the bracket one: **exit 0**, zero bytes of stdout, the overflow message on stderr. A caller
/// checking the exit code sees success and no data (review ledger V115).
///
/// The number: real code tops out at **245** (this repository, 1,691 files) and **222** (the OSS
/// corpus, 2,285 files, including `the-algorithm`). 4,000 clears both by 16x and sits 30x under the
/// measured death point. Re-derive it the same way — `scripts/measure/` has no census for this axis
/// yet, and the scan that produced those two numbers is the one in [`scan_depths`].
pub(crate) const MAX_OPERATOR_RUN: usize = 4_000;

/// The longest binary-operator chain any real file reaches, over the two populations above.
pub(crate) const LONGEST_REAL_OPERATOR_RUN: usize = 245;

const _: () = assert!(
    MAX_OPERATOR_RUN > LONGEST_REAL_OPERATOR_RUN,
    "the operator-run cap dropped to or below the longest chain measured in real code -- re-derive \
     BOTH bounds before moving either"
);

mod bounds;
use bounds::{
    MAX_PYTHON_STATEMENT_TOKENS, MAX_RUST_STATEMENT_TOKENS, MAX_TYPESCRIPT_STATEMENT_TOKENS,
};

/// Which statement cap this PATH is subject to, or `None` where no frontend needs one.
///
/// Keyed on the path rather than the dispatched language, because the two disagree exactly where it
/// matters: a `.vue` dispatches to no language and is still read by swc (review ledger V127), and
/// every caller of the gate has a path while one of them does not have a language.
///
/// 📏 Every frontend is accounted for here, and that sentence used to be false. This doc once said
/// "the three tree-sitter frontends return `None` on purpose -- they are covered by
/// `zzop_core::cst_depth`", which was true of three and silent about the other two that also return
/// `None`. One of those two was Python, and it was dying (review ledger V130). The account, all
/// eight:
///
/// - C#, Go, Java -> `None` here; covered by `zzop_core::cst_depth`, which measures the finished tree
///   and has a far wider window than this proxy.
/// - Rust, TypeScript, Python -> capped here, because each overflows inside or after a parse that
///   leaves no tree to measure in time.
/// - SQL, Prisma -> no statement cap, and none needed. They are NOT uncovered: the two universal
///   needles above still apply (a 400,000-paren `.sql` comes back `degraded` on the bracket cap).
///   What they lack is a recursive parser to overflow past those. 📏 Measured rather than assumed:
///   400,000 nested parens (800 KB) and a 370,000-term chain (1.48 MB) in `.sql`, and a 1.2 MB
///   `.prisma`, all return exit 0.
///
/// ⚠ A frontend added later gets `None` by default, which is the unsafe direction. The test
/// `every_dispatched_language_is_accounted_for_by_one_defence` fails when that happens.
///
/// 🔴 It returns the BOUNDARY too, and that is not tidiness. `scan_depths` used to assume `;`/`{`/`}`
/// for every language, which is false for Python and turned that language's cap into a cap on the
/// whole FILE (review ledger V140). The two answers are chosen by the same dispatch and now leave by
/// the same door, so a language cannot get one without the other.
fn statement_policy(rel: &str) -> (Option<usize>, StatementEnd) {
    if is_prescan_host(rel) {
        return (
            Some(MAX_TYPESCRIPT_STATEMENT_TOKENS),
            StatementEnd::Delimiters,
        );
    }
    match crate::dispatch::dispatch_by_extension(rel) {
        Some(Language::Rust) => (Some(MAX_RUST_STATEMENT_TOKENS), StatementEnd::Delimiters),
        Some(Language::TypeScript) => (
            Some(MAX_TYPESCRIPT_STATEMENT_TOKENS),
            StatementEnd::Delimiters,
        ),
        Some(Language::Python) => (
            Some(MAX_PYTHON_STATEMENT_TOKENS),
            StatementEnd::DelimitersOrLineBreak,
        ),
        _ => (None, StatementEnd::Delimiters),
    }
}

/// Whether this file is refused by the recursion gate. See the module doc for the two caps.
///
/// THE gate, with the same two-caller shape as [`super::oversized::is_oversized`]: the cold path asks it
/// here, and `crate::pipeline::artifact::cached_degrade_cause` asks it again to name a cached degrade's
/// cause. A second comparison written there would drift the day this one changes.
///
/// 🔴 `pub(crate)`, and the widening is the fix for an incident rather than a convenience. Passes in
/// [`crate::analyze`] re-read files off disk and parse them a second time, and those reads do not come
/// through this stage — so a file this gate refused reached a parser anyway. They ask through
/// [`crate::analyze::read_for_parse`] now, which asks this (review ledger V127).
pub(crate) fn exceeds_recursion_caps(
    text: &str,
    language: Option<Language>,
    rel: &str,
) -> Option<RecursionNeedle> {
    // EVERY language, not just TypeScript. The scoping used to rest on a measurement that had never
    // been taken (see the module doc); once taken, four more frontends aborted on the same shape.
    //
    // 🔴 And `None` is NOT a free pass, which is what this used to say: the old comment read "a file no
    // frontend claims is still not gated: nothing will parse it", and that sentence was false the day it
    // was written. `assemble::prescan` hands `.vue`/`.svelte`/`.md`/`.mdx`/`.astro` — all dispatch-`None`
    // — straight to swc. Measured on this HEAD before this line changed: ONE 51 KB `.vue` file took
    // `zzop analyze` to exit 127 with zero bytes of stdout, and `.svelte`, `.md` and `.astro` did the
    // same (review ledger V127). The question this gate answers is "will anything parse this file", and
    // dispatch is only one of the two ways the answer is yes.
    if language.is_none() && !is_prescan_host(rel) {
        return None;
    }
    text_exceeds_recursion_caps(text, rel)
}

/// The caps alone, for a caller that IS the parser.
///
/// [`exceeds_recursion_caps`] above answers a question the walk has to ask — *will anything parse this
/// file?* — and only then compares. A pass that has already decided to parse has answered that question
/// by existing, so it asks this instead. Same comparison, one copy: the day a third cap appears it
/// appears here and both callers get it.
/// `rel` is what selects the third cap AND where a statement ends — see [`statement_policy`]. A path no frontend with a recursive
/// parser reads gets the two universal caps and nothing more.
pub(crate) fn text_exceeds_recursion_caps(text: &str, rel: &str) -> Option<RecursionNeedle> {
    let (cap, ends) = statement_policy(rel);
    let d = scan_depths(text, ends);
    // Reading order, not severity order: a file over two bounds is reported by the first, and the
    // reader who fixes it will meet the second on the next run. Naming both would need the message
    // to carry a set, and no caller has ever wanted one.
    if d.brackets > MAX_NESTING_DEPTH {
        return Some(RecursionNeedle::Brackets {
            cap: MAX_NESTING_DEPTH,
        });
    }
    if d.operator_run > MAX_OPERATOR_RUN {
        return Some(RecursionNeedle::OperatorRun {
            cap: MAX_OPERATOR_RUN,
        });
    }
    cap.filter(|cap| d.statement_tokens > *cap)
        .map(|cap| RecursionNeedle::StatementTokens { cap })
}

/// Whether a pre-scan host reads this file's extension — the second way a parser gets to a file the
/// dispatch table does not claim. The list's owner is `zzop_parser_typescript::PRESCAN_IMPORT_HOSTS`;
/// this asks it rather than restating it, so a sixth host is gated the day it is added.
fn is_prescan_host(rel: &str) -> bool {
    std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| zzop_parser_typescript::prescan_mode(ext).is_some())
}

#[cfg(test)]
mod tests;
