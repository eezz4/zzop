//! The gate's own tests, split out on 2026-09-08 when the third cap pushed the parent past the
//! 300-line cap.
//!
//! The seam is the same one that put `scan.rs` beside them: the parent owns the POLICY (the cap
//! values and the censuses that bracket them), `scan.rs` owns the MEASUREMENT, and this file owns
//! the evidence that the policy does what its doc claims. Those three move on different days.
use super::*;

/// The gate answers WHICH needle since 2026-09-09 (review ledger V139); these tests ask WHETHER, so
/// they say so once here instead of writing `.is_some()` two dozen times.
fn gated(text: &str, language: Option<Language>, rel: &str) -> bool {
    exceeds_recursion_caps(text, language, rel).is_some()
}

fn refused(text: &str, rel: &str) -> bool {
    text_exceeds_recursion_caps(text, rel).is_some()
}

#[test]
fn the_reproduction_that_aborted_the_process_is_refused_and_a_shallow_sibling_is_not() {
    // One factor apart: the same statement, two depths. The deep one is the 1,017-byte file that
    // took the process down; the shallow one is above every real file measured (29) and must pass.
    let deep = format!("export const x = {}", "(".repeat(1000));
    let shallow = format!("export const x = {}", "(".repeat(200));
    assert!(gated(&deep, Some(Language::TypeScript), "a.ts"));
    assert!(!gated(&shallow, Some(Language::TypeScript), "a.ts"));
}

/// The gate now covers every dispatched language, and this asserts the SHAPE of that coverage.
///
/// 🔴 It replaces `the_gate_is_typescript_only_because_only_typescript_dies`, which asserted the
/// opposite and could not have caught its own falsity: it only ever called `exceeds_recursion_caps`,
/// never a parser, so it tested the gate's scoping against itself. Its failure message claimed the
/// seven other languages "return a degraded verdict at depth 100_000" — a property it never
/// exercised — and its input was `export const x = ((((`, TypeScript that Java, Go, Rust and C#
/// reject at the first token. A test whose name states a causal claim its body cannot reach is
/// worse than no test: it is a green light pointed at the wrong road (review ledger V99).
#[test]
fn the_gate_covers_every_dispatched_language() {
    let deep = "(".repeat(MAX_NESTING_DEPTH + 1);
    for lang in [
        Language::TypeScript,
        Language::Python,
        Language::Java21,
        Language::Rust,
        Language::Go,
        Language::CSharp,
        Language::Prisma,
        Language::Sql,
    ] {
        assert!(
            gated(&deep, Some(lang), "a.ts"),
            "{lang:?} parses recursively like the rest and must be gated too"
        );
    }
    // A file no frontend claims AND no pre-scan reads is never gated — see the case below for the
    // half of that sentence which used to be missing.
    assert!(!gated(&deep, None, "a.txt"));
}

/// The deepest real file in a 56,790-file, eight-language census is 99 (`clap`'s builder tests),
/// so the cap must stay above it or the gate starts refusing code people actually wrote.
///
/// The bound itself is a `const` assertion at module scope (below the cap's definition) rather than
/// an `assert!` here: both sides are constants, so a runtime check would be a test that cannot fail
/// at test time and a build that succeeds while the invariant is broken. What THIS test adds is the
/// half that is not constant — that a file at the measured maximum is actually allowed through.
#[test]
fn a_file_at_the_deepest_measured_real_depth_is_not_gated() {
    let real = "(".repeat(DEEPEST_REAL_FILE_MEASURED);
    for lang in [
        Language::Rust,
        Language::TypeScript,
        Language::Go,
        Language::CSharp,
        Language::Java21,
        Language::Python,
    ] {
        assert!(
            !gated(&real, Some(lang), "a.ts"),
            "{lang:?}: depth {DEEPEST_REAL_FILE_MEASURED} is the deepest REAL file measured and must parse"
        );
    }
}

/// A pre-scan host is gated even though no frontend DISPATCHES it, because `assemble::prescan`
/// parses it anyway. This is the clause whose absence let one 51 KB `.vue` file exit 127 with zero
/// bytes of stdout (review ledger V127) — the gate was asking about dispatch when the question is
/// whether anything parses.
#[test]
fn a_prescan_host_is_gated_even_though_nothing_dispatches_it() {
    let deep = "(".repeat(MAX_NESTING_DEPTH + 1);
    for rel in ["a.vue", "a.svelte", "a.md", "a.mdx", "a.astro"] {
        assert!(
            gated(&deep, None, rel),
            "{rel} is read by a pre-scan and handed to swc, so the gate must see it"
        );
    }
    // And the population that really is parsed by nobody stays ungated — the clause is a widening,
    // not the removal of the `None` arm.
    assert!(!gated(&deep, None, "a.txt"));
    assert!(!gated(&deep, None, "LICENSE"));
}

#[test]
fn brackets_inside_strings_comments_and_regexes_do_not_count() {
    // This is the measurement that decides whether a cap is possible at all: the naive count for a
    // real grafana file is 563 and its actual nesting is 4. Same shape, in miniature.
    let s = format!(
        "const re = /{}/; const t = `{}`; // {}",
        "(".repeat(400),
        "(".repeat(400),
        "(".repeat(400)
    );
    assert_eq!(scan_depths(&s, StatementEnd::Delimiters).brackets, 0);
    assert!(!gated(&s, Some(Language::TypeScript), "a.ts"));
}

/// 📏 The statement cap is chosen by PATH, and the three numbers are what that buys.
///
/// The same bytes are refused as Rust, allowed as TypeScript (whose real population is 20x larger),
/// and ignored entirely as Java — which is covered by the tree-depth cap instead and needs no proxy.
/// A single cap for all three would have to clear a vendored yarn bundle, and would then sit only
/// ~2x under the Rust abort floor rather than 4.5x.
#[test]
fn the_statement_cap_is_chosen_by_path_and_a_prescan_host_gets_one_too() {
    let long = format!(
        "fn f(b: bool) {{ let x = {}b; }}",
        "!".repeat(MAX_RUST_STATEMENT_TOKENS + 1)
    );
    assert!(gated(&long, Some(Language::Rust), "a.rs"));
    assert!(!gated(&long, Some(Language::TypeScript), "a.ts"));
    assert!(!gated(&long, Some(Language::Java21), "A.java"));

    // Ordinary Rust, well past the deepest real statement measured, still parses.
    let ordinary = format!("fn f(b: bool) {{ let x = {}b; }}", "!".repeat(2_000));
    assert!(!gated(&ordinary, Some(Language::Rust), "a.rs"));

    // 🔴 A prescan host dispatches to NO language and swc reads it anyway — the V127 shape. The cap
    // has to follow the path, not the dispatch, or the one population that already produced an
    // exit-127 abort is the one population it cannot see.
    let huge = format!(
        "<script>const x = {}b;</script>",
        "!".repeat(MAX_TYPESCRIPT_STATEMENT_TOKENS + 1)
    );
    assert!(gated(&huge, None, "a.vue"));
    assert!(!gated(&huge, None, "a.txt"));
}

/// 📏 Every dispatched language is covered by exactly one defence, and this is written as an
/// EXHAUSTIVE match on purpose: a ninth `Language` stops compiling here until someone says which
/// defence covers it.
///
/// 🔴 That is the whole point. `statement_policy` returns `None` by default, and `None` is the unsafe
/// direction — a frontend that nobody classified falls into it silently. Python did, and a 1.5 MB
/// file inside the size cap took the process down with exit 127 and no findings (review ledger V130).
/// A test that merely asserted today's three caps would have been green through all of it.
#[test]
fn every_dispatched_language_is_accounted_for_by_one_defence() {
    #[derive(PartialEq, Debug)]
    enum Defence {
        /// Measured on the finished tree — `zzop_core::cst_depth`.
        CstDepth,
        /// Capped before the parse, because the overflow happens where no tree survives to be read.
        StatementCap,
        /// Nothing to overflow: no recursive parser. Measured, not assumed — see `statement_cap`'s doc.
        NoRecursiveParser,
    }

    for (lang, rel) in [
        (Language::CSharp, "a.cs"),
        (Language::Go, "a.go"),
        (Language::Java21, "A.java"),
        (Language::Rust, "a.rs"),
        (Language::TypeScript, "a.ts"),
        (Language::Python, "a.py"),
        (Language::Sql, "a.sql"),
        (Language::Prisma, "schema.prisma"),
    ] {
        let defence = match lang {
            Language::CSharp | Language::Go | Language::Java21 => Defence::CstDepth,
            Language::Rust | Language::TypeScript | Language::Python => Defence::StatementCap,
            Language::Sql | Language::Prisma => Defence::NoRecursiveParser,
        };
        assert_eq!(
            statement_policy(rel).0.is_some(),
            defence == Defence::StatementCap,
            "{lang:?} ({rel}) is classified {defence:?} but statement_cap disagrees"
        );
    }
}

/// The refusal review ledger V140 was filed for: a file with nothing recursive in it anywhere.
///
/// 📏 The reviewer's shape, kept verbatim in spirit — 4,001 lines of `CONST_i = i`, no brackets, no
/// operators, no commas, ~52 KB. Under the old boundary this counted as ONE statement of roughly
/// 12,000 tokens and was refused, and the warning then told its owner the file had more than 256
/// nested brackets. It has none.
#[test]
fn a_python_module_of_short_statements_is_not_refused() {
    let src: String = (0..4_001).map(|i| format!("CONST_{i} = {i}\n")).collect();
    assert!(!refused(&src, "src/constants.py"));
}

/// And the gate still closes on the shape it exists for. `()` repeated does not reset anything —
/// brackets balance immediately so depth never exceeds 1, and there is no operator and no comma — so
/// this trips the statement cap and ONLY the statement cap.
#[test]
fn one_long_python_statement_is_still_refused() {
    let src = format!("x = f{}\n", "()".repeat(6_000));
    assert!(refused(&src, "src/killer.py"));
}

/// A line break inside brackets is an IMPLICIT continuation, so it does not end the statement. This
/// is the guard that keeps the fix above from becoming a hole: without the `depth == 0` condition the
/// same killer, wrapped in one call and spread over lines, would reset on every newline and walk
/// straight through the cap.
#[test]
fn a_python_line_break_inside_brackets_does_not_end_the_statement() {
    let mut src = String::from("x = f(\n");
    for _ in 0..6_000 {
        src.push_str("()\n");
    }
    src.push_str(")\n");
    assert!(refused(&src, "src/killer.py"));

    // The control: the same bytes with the wrapping call removed are 6,000 separate statements.
    let loose: String = "()\n".repeat(6_000);
    assert!(!refused(&loose, "src/loose.py"));
}

/// A trailing backslash is an EXPLICIT continuation, and it is the other half of the same guard.
#[test]
fn a_python_backslash_continuation_does_not_end_the_statement() {
    let joined = "not \\\n".repeat(6_000);
    assert!(refused(&joined, "src/joined.py"));

    let separate = "not x\n".repeat(6_000);
    assert!(!refused(&separate, "src/separate.py"));
}

/// The boundary moved for Python and for nothing else — asserted in the direction that can fail.
///
/// 🔴 The first draft of this test asserted that the same text is accepted everywhere, and `.rs`
/// refused it. That was the TEST being wrong and the gate being right: a C-family file with no `;`,
/// no brace and no comma really is one statement, and there is no such thing as real Rust that looks
/// like this. So the honest assertion is the opposite one — the identical bytes are refused on every
/// delimiter language and accepted on Python — which is exactly what "the boundary is per-language"
/// means, and which fails the moment the line-break reset leaks into another arm.
#[test]
fn the_line_break_boundary_is_python_only() {
    // 20,000 lines, ~3 tokens each: past the 10,000 cap for `.rs` AND the 50,000 one the TypeScript
    // frontend gets, so no arm is accepted merely for having the larger cap.
    let src: String = (0..20_000).map(|i| format!("CONST_{i} = {i}\n")).collect();
    assert!(!refused(&src, "a.py"));
    for rel in ["a.ts", "a.tsx", "a.rs", "a.vue"] {
        assert!(
            refused(&src, rel),
            "{rel} accepted a file that is one statement under its own boundary -- the Python \
             line-break reset has leaked into another language's arm"
        );
    }
}
mod census;
