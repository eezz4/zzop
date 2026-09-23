//! The three STATEMENT-token caps and the census that brackets each, split out of `recursion.rs` on
//! 2026-09-09 for the line cap.
//!
//! The parent owns the GATE — which needle is asked first, and what a refusal is called. This file
//! owns the NUMBERS for the needle whose cap is per-language, because that is the one that moves:
//! the other two are single values and a language cannot change them, while every one of these has a
//! `LONGEST_REAL_*` twin that a corpus re-scan can move under it. The `const _: () = assert!` pairs
//! below are what make that safe — a cap cannot be lowered past its own census without failing to
//! compile.
//!
//! The census that derives the twins is [`super::tests::census`], and it runs the shipped needle.

/// Significant tokens in one statement past which a RUST file is not handed to a parser — the third
/// needle, and the only one whose cap depends on which frontend will read the file.
///
/// 🔴 **Why a third needle at all.** The two caps above are proxies for how deep a walk will recurse,
/// and on 2026-09-08 four shapes walked through both into an exit-127 abort (review ledger V129).
/// Three frontends answer that on the finished tree now — `zzop_core::cst_depth`, where depth is a
/// fact rather than a proxy for one. Two cannot, and for the same reason: `parser-rust` is `syn` and
/// `parser-typescript` is swc, and in both the overflow happens INSIDE the parse, so there is no tree
/// left to measure by the time the answer would be needed.
///
/// 📏 **Headroom does not close it — that is arithmetic, not opinion.** Measured against a fixed
/// stack, the Rust parser costs ~1.4 KB per nesting level: 32,000 levels fit in 64 MiB and 200,000 do
/// not, matching the shipped binary's abort at 45,000 with 40,000 surviving. The size cap admits a
/// 1.5 MB file, whose worst case is ~1.5M levels — about **2.1 GB** of stack. No reservation reaches
/// that, so a bigger thread is a bridge in name only.
///
/// 🔵 **Why the two caps differ, when one number would be simpler.** 📏 Censused across 58,919 files,
/// the longest single statement per extension: rs **626** · java 263 · cs 500 · js 504 · tsx 939 ·
/// ts 1,344 · py 1,417 · go 1,589 · astro 2,575 · sql 4,941 · cjs **12,734** (a vendored yarn
/// release). 📏 And the abort floors differ too: Rust between 40,000 and 45,000, the TypeScript
/// frontend between 100,000 and 200,000. A single cap would have to sit above 12,734 and below
/// 40,000 — it exists (25,000 would do), but it buys ~2x on each side. Two caps buy 16x/4.5x for
/// Rust and 3.9x/3x for TypeScript, because each is set against the population its own frontend
/// actually reads. `syn` is never handed a yarn bundle.
///
/// ⚠ An earlier version of this doc quoted 1,723 for rs and 38,070 overall, from a census whose
/// scanner counted identifier BYTES rather than identifier TOKENS. The numbers above are the
/// corrected count.
///
/// Re-derive the lower bounds by running [`scan_depths`] over a corpus and taking `statement_tokens`
/// per extension; the upper bounds by growing a `!` chain until `zzop analyze` exits 127.
pub(crate) const MAX_RUST_STATEMENT_TOKENS: usize = 10_000;

/// The longest single statement any real `.rs` file reaches, over the census above.
pub(crate) const LONGEST_REAL_RUST_STATEMENT_MEASURED: usize = 626;

/// The same cap for every file the TypeScript frontend reads — which is not only `.ts`: swc is also
/// what `assemble::prescan` hands `.vue`/`.svelte`/`.astro`/`.mdx`/`.md`, and those dispatch to no
/// language at all. [`statement_policy`] keys on the PATH for exactly that reason.
pub(crate) const MAX_TYPESCRIPT_STATEMENT_TOKENS: usize = 50_000;

/// The longest single statement any real file the TypeScript frontend reads — the `.cjs` yarn
/// release, 20x larger than the deepest real `.rs`, and the reason these two caps are not one.
pub(crate) const LONGEST_REAL_TYPESCRIPT_STATEMENT_MEASURED: usize = 12_734;

const _: () = assert!(
    MAX_RUST_STATEMENT_TOKENS > LONGEST_REAL_RUST_STATEMENT_MEASURED,
    "the Rust statement cap dropped to or below the longest statement measured in real code -- \
     re-derive BOTH bounds before moving either"
);

const _: () = assert!(
    MAX_TYPESCRIPT_STATEMENT_TOKENS > LONGEST_REAL_TYPESCRIPT_STATEMENT_MEASURED,
    "the TypeScript statement cap dropped to or below the longest statement measured in real code -- \
     re-derive BOTH bounds before moving either"
);

/// The same cap for `.py`. 🔴 Python was measured for this class once, at ONE size, and the answer
/// was read as "needs nothing" — review ledger V130. It needed something.
///
/// 📏 What the second look found: `x = f` followed by 749,000 `()` is **1,498,006 bytes**, under the
/// 1,500,000 size cap, and takes the process down — exit 127, 53 bytes of stderr, no findings. So
/// does the same shape in `.b`. Bisected, the floor is between **300,000 and 350,000** calls.
///
/// 🔵 Why the first look missed it, and the lesson is about the shapes chosen rather than the size:
/// ruff's own `DEFAULT_MAX_RECURSION_DEPTH` turns PREFIX and conditional chains (`not`, `-`,
/// `b if b else`, `await`) into a parse error, which degrades the file safely — and those are exactly
/// the shapes that got tried. A POSTFIX chain does not recurse in an LR parser, so ruff returns `Ok`,
/// and the frames that overflow are spent afterwards, walking the tree it handed back. ⇒ *a parser
/// surviving is not the visitors surviving*, and a shape that the parser refuses proves nothing about
/// a shape it accepts.
pub(crate) const MAX_PYTHON_STATEMENT_TOKENS: usize = 10_000;

/// The longest single statement any real `.py` file reaches. 📏 Census over 5,795 `.py` files (2026-09-09).
/// 🔴 It was 1_417 and that was not a statement length — [`tests::census`] owns the story and the command.
pub(crate) const LONGEST_REAL_PYTHON_STATEMENT_MEASURED: usize = 1_328;

const _: () = assert!(
    MAX_PYTHON_STATEMENT_TOKENS > LONGEST_REAL_PYTHON_STATEMENT_MEASURED,
    "the Python statement cap dropped to or below the longest statement measured in real code -- \
     re-derive BOTH bounds before moving either"
);
