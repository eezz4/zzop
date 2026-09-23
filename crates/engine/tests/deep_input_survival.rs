//! The engine answers on input deep enough to overflow a parser's stack, in every dispatched language.
//!
//! ## Why this file exists, stated as the incident
//!
//! On 2026-09-08 a batch reserved a parsing-sized stack for every parse site and hand-ran eleven deep
//! fixtures through the CLI: eleven exit 0, recorded in a ledger row, marked DONE. Four hours later,
//! the same round routed a projection around the nesting gate — `fresh::oversized` called the
//! call-graph projection with `degraded = false` hard-coded, parsing the very file the gate had just
//! refused. `zzop analyze` on one 40 KB Rust file went back to exit 127 with zero bytes of output, and
//! **nothing in the repository could tell**: the gate's own tests call `exceeds_recursion_caps` and never
//! reach a parser, `deep_stack` has no tests at all, and no test anywhere fed deep input to
//! `analyze_tree`. An external reviewer found it by running the binary (review ledger V99/V114).
//!
//! The lesson is not "add a test for that line". It is that a fix whose only evidence is a table
//! somebody typed once is not a fix, it is that day's observation. This file is the machine.
//!
//! ## The second incident, and the hole it found IN THIS FILE
//!
//! The cases below were written on 2026-09-08 for six DISPATCHED languages, and that word turned out
//! to be the hole. Two populations reach a parser without being dispatched by the walk, and both were
//! outside every case here:
//!
//! 1. PRE-SCAN HOSTS — `.vue`/`.svelte`/`.md`/`.mdx`/`.astro`. Dispatch is `None`, so the gate
//!    returned false with a comment saying "nothing will parse it", and then `assemble::prescan` handed
//!    them to swc. Measured: one 51 KB `.vue` file exits 127 with zero bytes; `.svelte`, `.md` and
//!    `.astro` the same. (`.mdx` survived, and why was not measured — it is covered below anyway.)
//! 2. FILES THE GATE ALREADY REFUSED — `pipeline::fresh::ts_slot` puts a file into `ts_paths`
//!    regardless of `degrade_cause`, and three later passes re-read that set off disk and re-parse it.
//!    Measured: one `composables/deep.ts` in a Nuxt tree exits 127, on a file the gate had just
//!    correctly refused. **The refusal was right; it did not travel.**
//!
//! Both die on `thread 'main'`, so `pipeline::deep_stack`'s 64 MiB reserve does not apply either.
//! Review ledger V127. The lesson for THIS file is narrow and worth stating: a survival suite scoped
//! by "language" cannot see a population defined by "which pass opens the file", and the second is
//! the axis the failure actually lives on.
//!
//! ## Why this is its own test binary
//!
//! Not preference. A stack overflow is PROCESS-WIDE: it aborts instead of unwinding, so a
//! regression here does not fail one test — it takes the binary down and every neighbour's result
//! with it. Measured while writing this file: with the gate disabled the run ends at
//! `error: test failed` and the tests that had not finished never report at all. Alone, that
//! blast radius is one file. The allowlist in `crates/engine/tests/rule_contracts/main.rs` is
//! where a standalone binary registers that reason, and it asserts this paragraph is still here.
//!
//! ## What it asserts, and what it deliberately does not
//!
//! ONE property per case: `analyze_tree` RETURNS, and returns something. Not what it found — a file
//! this deep is past the nesting cap by design, so it is projected lexically and contributes no AST
//! facts, and pinning that would pin today's cap value instead of the survival. Survival is the
//! invariant; the cap is a number that may move.
//!
//! Depths are `MAX_NESTING_DEPTH * 100` and above — far past anything a human writes (the deepest real
//! file measured in this repo is 99, pinned as `DEEPEST_REAL_FILE_MEASURED`) and far past the depth
//! that actually kills each parser when it is allowed to run (Rust died at 20,000; the reserve alone
//! was not enough, which is why the gate exists at all).

use std::fs;

use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("zzop-deep-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 100x the cap. The gate is what has to hold; the reserve behind it is the second line, not the first.
const DEPTH: usize = 25_600;

/// The property PLUS the mechanism: an answer came back, AND this file came back REFUSED.
///
/// 🔴 [`survives`] alone cannot see the CST-depth cap, and that is measured rather than assumed: with
/// the cap removed, all six cases at the bottom of this file still passed it. This harness does not
/// abort where the shipped binary does, so "an answer came back" was green before the fix and after
/// it — a test whose name states a cause its body cannot reach, which is the exact failure
/// [`the_gate_covers_every_dispatched_language`] was written to replace, reappearing one file later.
///
/// Asserting the REFUSAL is what makes those cases fail when the cap is gone, and it is the observable
/// the cap really produces: `degraded: ["A.cs"]` where an uncapped run analyzes the file in full.
fn refused(tag: &str, file: &str, source: String) {
    let out = survives_tree(tag, &[(file, source)], 1);
    assert!(
        out.degraded.iter().any(|d| d == file),
        "{file} must come back refused rather than analyzed ({tag}) -- degraded was {:?}",
        out.degraded
    );
}

fn survives(tag: &str, file: &str, source: String) {
    survives_tree(tag, &[(file, source)], 1);
}

/// The same property over a tree of more than one file, for the passes that only run when the tree
/// has a shape — a Nuxt app needs its `nuxt.config.*` before `nuxt_auto_import` reads anything at all.
/// `expect_files` is the walk count, kept explicit so a fixture that grows a file has to say so.
fn survives_tree(tag: &str, files: &[(&str, String)], expect_files: usize) -> AnalyzeOutput {
    let dir = TempDir::new(tag);
    for (file, source) in files {
        let path = dir.path().join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
    }
    let cfg = EngineConfig {
        source_id: "deep".to_string(),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    // Non-vacuous: the walk really reached the file. Without this the test would pass on a tree the
    // engine never opened, which is the shape the census fixtures had to grow a guard against too.
    assert_eq!(
        out.coverage.files, expect_files,
        "the fixture file(s) must have been walked ({tag})"
    );
    out
}

#[test]
fn a_rust_file_nested_far_past_the_cap_still_returns() {
    survives(
        "rs",
        "a.rs",
        format!("fn f() {}{}\n", "{".repeat(DEPTH), "}".repeat(DEPTH)),
    );
}

#[test]
fn a_typescript_file_nested_far_past_the_cap_still_returns() {
    survives(
        "ts",
        "a.ts",
        format!(
            "export const x = {}1{};\n",
            "(".repeat(DEPTH),
            ")".repeat(DEPTH)
        ),
    );
}

#[test]
fn a_python_file_nested_far_past_the_cap_still_returns() {
    survives(
        "py",
        "a.py",
        format!("x = {}1{}\n", "(".repeat(DEPTH), ")".repeat(DEPTH)),
    );
}

#[test]
fn a_java_file_nested_far_past_the_cap_still_returns() {
    survives(
        "java",
        "A.java",
        format!(
            "class A {{ void f() {}{} }}\n",
            "{".repeat(DEPTH),
            "}".repeat(DEPTH)
        ),
    );
}

#[test]
fn a_csharp_file_nested_far_past_the_cap_still_returns() {
    survives(
        "cs",
        "A.cs",
        format!(
            "class A {{ void F() {}{} }}\n",
            "{".repeat(DEPTH),
            "}".repeat(DEPTH)
        ),
    );
}

#[test]
fn a_go_file_nested_far_past_the_cap_still_returns() {
    survives(
        "go",
        "a.go",
        format!(
            "package main\nfunc f() {}{}\n",
            "{".repeat(DEPTH),
            "}".repeat(DEPTH)
        ),
    );
}

// The file class that actually broke: past the SIZE cap as well as the nesting cap, which routes
// through `fresh::oversized` — the function that was calling parsers on files it had just refused.
#[test]
fn a_file_past_both_caps_still_returns() {
    let padding = format!("// {}\n", "x".repeat(2_000_000));
    survives(
        "both",
        "a.rs",
        format!(
            "{padding}fn f() {}{}\n",
            "{".repeat(DEPTH),
            "}".repeat(DEPTH)
        ),
    );
}

// ── The second needle: a chain with no brackets at all ───────────────────────────────────────────
//
// `1 + 1 + 1 + …` is left-associative, so it nests the parser once per operator while
// the bracket count returns 0. Measured before the second cap existed: 100,000 terms analyzed,
// 150,000 terms overflowed — under the size cap, with the bracket gate reporting nothing to refuse,
// and the process exiting **0** with empty stdout (review ledger V115).
//
// 200,000 terms here: comfortably past the death point, so the assertion is that the gate refuses it
// before a parser sees it, not that some parser survives it.
const TERMS: usize = 200_000;

fn chain(term: &str) -> String {
    let mut s = String::with_capacity(TERMS * 4);
    for i in 0..TERMS {
        if i > 0 {
            s.push_str(" + ");
        }
        s.push_str(term);
    }
    s
}

#[test]
fn a_typescript_operator_chain_past_the_cap_still_returns() {
    survives(
        "ts-chain",
        "a.ts",
        format!("export const x = {};\n", chain("1")),
    );
}

#[test]
fn a_rust_operator_chain_past_the_cap_still_returns() {
    survives(
        "rs-chain",
        "a.rs",
        format!("fn f() -> i64 {{ {} }}\n", chain("1")),
    );
}

#[test]
fn a_python_operator_chain_past_the_cap_still_returns() {
    survives("py-chain", "a.py", format!("x = {}\n", chain("1")));
}

#[test]
fn a_java_operator_chain_past_the_cap_still_returns() {
    survives(
        "java-chain",
        "A.java",
        format!("class A {{ long f() {{ return {}; }} }}\n", chain("1")),
    );
}

#[test]
fn a_csharp_operator_chain_past_the_cap_still_returns() {
    survives(
        "cs-chain",
        "A.cs",
        format!("class A {{ long F() {{ return {}; }} }}\n", chain("1")),
    );
}

#[test]
fn a_go_operator_chain_past_the_cap_still_returns() {
    survives(
        "go-chain",
        "a.go",
        format!("package main\nfunc f() int {{ return {} }}\n", chain("1")),
    );
}

// ── The third needle: a file no frontend claims, that a parser reads anyway ──────────────────────
//
// `PRESCAN_IMPORT_HOSTS` — the extensions `assemble::prescan` opens and feeds to swc. Their dispatch
// is `None`, which is exactly why the gate skipped them: its comment said nothing would parse them.
// One case per host rather than a loop, so a failure names the extension without a message argument.

fn script_block(deep: &str) -> String {
    format!("<script>\nimport x from \"./x\";\nconst y = {deep};\n</script>\n")
}

fn deep_parens() -> String {
    format!("{}1{}", "(".repeat(DEPTH), ")".repeat(DEPTH))
}

#[test]
fn a_vue_prescan_host_nested_far_past_the_cap_still_returns() {
    survives("vue", "a.vue", script_block(&deep_parens()));
}

#[test]
fn a_svelte_prescan_host_nested_far_past_the_cap_still_returns() {
    survives("svelte", "a.svelte", script_block(&deep_parens()));
}

#[test]
fn a_markdown_prescan_host_nested_far_past_the_cap_still_returns() {
    survives("md", "a.md", script_block(&deep_parens()));
}

#[test]
fn an_mdx_prescan_host_nested_far_past_the_cap_still_returns() {
    // The one host that did NOT die in the reproduction. It is here anyway: "it happened not to"
    // is not a property, and the day its mode changes this is the case that notices.
    survives(
        "mdx",
        "a.mdx",
        format!(
            "import x from \"./x\";\nexport const y = {};\n",
            deep_parens()
        ),
    );
}

#[test]
fn an_astro_prescan_host_nested_far_past_the_cap_still_returns() {
    survives(
        "astro",
        "a.astro",
        format!(
            "---\nimport x from \"./x\";\nconst y = {};\n---\n<div/>\n",
            deep_parens()
        ),
    );
}

// ── The fourth needle: the gate refused it, and a later pass read it off disk anyway ─────────────
//
// This is the only case in this file where the gate WORKS and the answer still died, which makes it
// the only one that can catch a regression in `analyze::read_for_parse` itself. The Nuxt config is
// load-bearing: with no `nuxt.config.*` there are no app dirs, so `nuxt_auto_import` reads nothing and
// the fixture proves nothing.
#[test]
fn a_refused_file_that_a_second_pass_re_reads_still_returns() {
    survives_tree(
        "nuxt-reread",
        &[
            (
                "nuxt.config.ts",
                "export default defineNuxtConfig({})\n".to_string(),
            ),
            (
                "composables/deep.ts",
                format!("export const x = {};\n", deep_parens()),
            ),
        ],
        2,
    );
}

// ── The fifth axis: deep in the TREE, invisible to both text needles ─────────────────────────────
//
// 📏 Every case above nests with brackets or a counted operator, so the pre-parse gate sees it and
// refuses. These four do not, and on 2026-09-08 all four took the process down with exit 127, zero
// findings and the rest of the tree unreported (review ledger V129):
//
//   `b ? 1 : b ? 1 : …` — no bracket, and `?` is not in the operator set
//   `!!!!…b`            — no bracket, no binary operator, and at 70 KB the smallest killer found
//   `a.b.b.b…`          — a member chain is neither
//   `- - - b`           — every `-` IS counted, and the "multi-byte operator counts once" rule
//                         collapses the whole chain to a run of ONE
//
// They are refused now by CST DEPTH instead (`zzop_core::cst_depth`), measured on the finished tree
// where the answer is a fact rather than a proxy for one. The sizes below are the ones that actually
// aborted, so these reproduce the defect rather than merely exercising the new cap.
//
// ⚠ The Go and Java twins are the same SHAPE in languages whose abort floor was never bisected — they
// survived at 50,000 and were not pushed further. They assert the property, not a reproduction.

const CHAIN: usize = 70_000;

#[test]
fn a_csharp_conditional_chain_past_the_cap_still_returns() {
    refused(
        "cs-ternary",
        "A.cs",
        format!(
            "class C {{ void M(bool b) {{ var x = {}1; }} }}\n",
            "b ? 1 : ".repeat(CHAIN)
        ),
    );
}

#[test]
fn a_csharp_unary_chain_past_the_cap_still_returns() {
    refused(
        "cs-not",
        "A.cs",
        format!(
            "class C {{ void M(bool b) {{ var x = {}b; }} }}\n",
            "!".repeat(CHAIN)
        ),
    );
}

#[test]
fn a_csharp_member_chain_past_the_cap_still_returns() {
    refused(
        "cs-member",
        "A.cs",
        format!(
            "class C {{ void M() {{ var x = a{}; }} }}\n",
            ".b".repeat(CHAIN)
        ),
    );
}

#[test]
fn a_csharp_counted_operator_chain_the_run_rule_collapses_still_returns() {
    // The one that says the operator needle is not merely incomplete but can be DEFEATED by an input
    // made of the very bytes it counts.
    refused(
        "cs-neg",
        "A.cs",
        format!(
            "class C {{ void M(int b) {{ var x = {}b; }} }}\n",
            "- ".repeat(CHAIN)
        ),
    );
}

#[test]
fn a_go_unary_chain_past_the_cap_still_returns() {
    refused(
        "go-not",
        "a.go",
        format!(
            "package p\nfunc f(b bool) {{ x := {}b; _ = x }}\n",
            "!".repeat(CHAIN)
        ),
    );
}

#[test]
fn a_java_unary_chain_past_the_cap_still_returns() {
    refused(
        "java-not",
        "A.java",
        format!(
            "class A {{ void f(boolean b) {{ boolean x = {}b; }} }}\n",
            "!".repeat(CHAIN)
        ),
    );
}

/// 🔴 The Rust half of V129, and the one that could not be answered the same way. `parser-rust` is
/// `syn`, whose overflow happens INSIDE the parse, so the CST-depth cap the other three frontends use
/// has nothing to measure — the process is already gone. 📏 And headroom is arithmetic, not opinion:
/// `syn` costs ~1.4 KB of stack per level (32,000 fit in 64 MiB, 200,000 do not), so the 1.5 MB size
/// cap's worst case needs ~2.1 GB. This one is refused before `syn` is called at all.
#[test]
fn a_rust_unary_chain_past_the_statement_cap_still_returns() {
    refused(
        "rs-not",
        "a.rs",
        format!(
            "fn f(b: bool) {{ let x = {}b; let _ = x; }}\n",
            "!".repeat(CHAIN)
        ),
    );
}

/// 🔴 The TypeScript half, found only because the four C# shapes were re-run at the largest size the
/// SIZE cap still admits. 📏 At 70,000 links a `.ts` member chain analyzes fine; at 200,000 it exits
/// 127, and so does a `!` chain. swc recurses inside the parse exactly as `syn` does, so this is the
/// statement cap's population too — with its own number, because the largest real file this frontend
/// reads is a vendored yarn release 20x bigger than the deepest real `.rs`.
#[test]
fn a_typescript_unary_chain_past_the_statement_cap_still_returns() {
    refused(
        "ts-not",
        "a.ts",
        format!("const x = {}b;\n", "!".repeat(CHAIN)),
    );
}

#[test]
fn a_typescript_member_chain_past_the_statement_cap_still_returns() {
    refused(
        "ts-member",
        "a.ts",
        format!("const x = a{};\n", ".b".repeat(CHAIN)),
    );
}

/// A prescan host dispatches to NO language and swc reads it anyway — the V127 shape. If the cap
/// followed the dispatch instead of the path, this is the one population it could not see.
#[test]
fn a_prescan_host_past_the_statement_cap_still_returns() {
    refused(
        "vue-not",
        "a.vue",
        format!("<script>const x = {}b;</script>\n", "!".repeat(CHAIN)),
    );
}

/// 🔴 Python, and the reason it is here late. This class was declared closed for Python once, on
/// `not`-chains and member chains at 70,000 — shapes ruff's own recursion limit turns into a parse
/// error, so they degrade safely and prove nothing about the shapes it accepts. 📏 A POSTFIX chain
/// does not recurse in an LR parser: `x = f` + `()` × 749,000 is 1,498,006 bytes, UNDER the 1,500,000
/// size cap, and exited 127 with 53 bytes of stderr and no findings (review ledger V130).
#[test]
fn a_python_call_chain_past_the_statement_cap_still_returns() {
    refused("py-call", "a.py", format!("x = f{}\n", "()".repeat(CHAIN)));
}

#[test]
fn a_python_attribute_chain_past_the_statement_cap_still_returns() {
    refused("py-attr", "a.py", format!("x = a{}\n", ".b".repeat(CHAIN)));
}

/// The other half of the account: SQL and Prisma are covered by NOTHING, and that is correct rather
/// than an oversight — neither has a recursive parser to overflow. 📏 Measured at the largest size
/// the size cap admits, so the claim is a measurement and not a guess.
#[test]
fn sql_and_prisma_need_no_cap_and_still_return() {
    survives(
        "sql-deep",
        "q.sql",
        format!("SELECT {}1{};\n", "(".repeat(DEPTH), ")".repeat(DEPTH)),
    );
    survives(
        "prisma-deep",
        "schema.prisma",
        format!(
            "model M {{\n  id Int @id{}\n}}\n",
            " @default(1)".repeat(50_000)
        ),
    );
}
