//! How many times does ONE analyze pass hand ONE TypeScript file to swc?
//!
//! This file exists because that number used to live in prose. `pipeline::parsers::parse_typescript`'s
//! doc said a well-formed file is "parsed by swc three times per pass (four counting `parse_ok`'s
//! probe)" — a correct statement about that one function and a wrong one about a run, since the per-file
//! lane around it (`pipeline::fresh`, `pipeline::io`) runs many further independent extractors over the
//! same text, each parsing again. The first measurement below came back an order of magnitude above the
//! documented figure. A written count in that position rots the moment an extractor is added, and there
//! is no way to notice: nothing reads a comment.
//!
//! `zzop_parser_typescript::parse_count` counts the real thing (it sits on `parse_with_cm`, the crate's
//! sole swc entry), so this test replaces the sentence with a measurement. When the number moves, this
//! test says so and the new number is recorded here — that is the intended maintenance, not a nuisance:
//! per-file parse count is the dominant cost of a cold run, and a silent doubling of it is exactly the
//! regression worth a failing test.
//!
//! ## Why its own test binary
//! The census is a process-wide counter and cargo runs each `tests/*.rs` as its own process, but tests
//! WITHIN a binary run on parallel threads. So this file deliberately holds exactly one test — any second
//! test here that touched a `.ts` file would race the counter and make both numbers meaningless.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        // A clock is not a unique name. Windows' `SystemTime` granularity is coarse enough
        // that two threads entering here together read the SAME nanos, and two tests that then
        // `git init` one directory collide inside git's own template copy — a red gate with
        // nothing to do with the change under test. The counter is what makes the name unique;
        // the clock only keeps runs apart, and this file was one of the last without it.
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Measured on a default `EngineConfig` (no cache, no packs, no git) over a tree holding exactly one
/// well-formed `.ts` file. Update this constant WITH the measurement when the pipeline's extractor set
/// changes; never adjust it to make a red test green without knowing which extractor moved.
/// 36 -> 37 (2026-08-03): the A17 `string_literals` extractor joined `pipeline::fresh` — one more
/// whole-file swc parse per pass, the same one-parse-per-extractor pattern as its 36 siblings.
///
/// 37 -> 1 (2026-08-08): `parse_with_cm` gained a one-entry thread-local memo, so the ~37 extractors
/// that each handed swc the SAME `(rel, text)` now share one parse. The number stopped being
/// "one per extractor" and became **one per file**, which is why this constant is no longer expected
/// to move when an extractor is added — a new extractor in `pipeline::fresh` should keep it at 1, and
/// a rise means the memo stopped hitting (the likely cause is a caller that re-reads or rewrites the
/// text between extractors, which breaks the key). Falling to 0 means the file was not parsed at all;
/// the fixture assertion below catches that separately.
/// 🔴 **This pin holds ONLY on a route-free tree, and that is not a fixture convenience — it is the
/// boundary of where a parse COUNT is a measurable quantity at all** (2026-09-07, review ledger V48).
///
/// The fixture below has no HTTP route, so `run_callgraph_rules` early-returns and the run makes exactly
/// one whole-tree pass. Add a route and a SECOND pass appears — and the count stops being deterministic.
/// Measured on a two-file tree (one route file, one plain module), default rules:
///
/// | condition | parse counts observed |
/// |---|---|
/// | 8 runs in one process, default threads | **2, 4, 4, 4, 4, 5, 5, 6** |
/// | 8 runs, `RAYON_NUM_THREADS=1` | **5, 5, 5, 5, 5, 5, 6, 6** |
/// | first run only, 6 fresh processes | **5, 6, 6, 6, 6, 6** |
///
/// So it is not fixable by pinning the thread count and not fixable by using a cold process. The memo
/// (`parse_with_cm`) is a `thread_local!` one-entry cache while the counter is a process-global
/// `AtomicU64`; on the second pass, which worker thread picks up a given file decides whether that
/// thread already holds it. Work stealing is not reproducible, so the count is a function of scheduling.
///
/// ⚠ **The product is still deterministic — what wobbles is the WORK, not the ANSWER.** Byte-identical
/// output has its own pin and it is green. Do not read this as an output-determinism defect.
///
/// 🔵 **Consequence for anyone trying to build a parse/perf gate here**: an EQUALITY gate over a
/// route-bearing tree is not available today, at any thread setting. Two things are: this pin (the
/// route-free population, where one pass means one parse per file), and an UPPER-BOUND assertion over a
/// route-bearing tree — the observed maximum across ~40 runs at 1/2/4/default threads was 6 on two
/// files, and a ceiling still catches the regression that matters (a memo that stopped hitting doubles
/// the count; it does not nudge it by one).
/// 📅 **1 -> 2 -> 1, all on 2026-09-08, and every step of that was a measurement correcting a story.**
///
/// | when | value | what was actually true |
/// |---|---|---|
/// | before | 1 | a ONE-FILE fixture. rayon inlines a single work item onto the installing thread, so the second pass found the memo still warm. At N=1 "one parse per file" and "one parse per pass" are indistinguishable — and they were different. |
/// | V99 | 2 | the threading change put each parsing pass on its own thread, which exposed on one file what eight files had always paid (8 files measured 16 both before and after). |
/// | V110 | **1** | the second pass stopped re-reading and re-parsing the tree. Now it is one parse per file for real, at any N. |
///
/// 🔴 **The prose here named the wrong pass for a month, and V108 is what proved it.** It said the
/// second parse was the CALL-GRAPH pass and that this constant would return to 1 once that pass
/// stopped re-reading files. That pass DID stop — and this number did not move, because it never ran
/// over these fixtures at all (`run_callgraph_rules` early-returns with no call-graph rule active, and
/// the first two fixtures are route-free). One backtrace out of `parse_uncached` named the real
/// second parse: `dead_exports::dead_export_findings` -> `parse_dead_export_facts`, in the assemble
/// phase, which its own module doc had been describing plainly the whole time —
/// *"a 100%-cache-hit run still re-reads and re-parses the whole TypeScript tree"*.
///
/// 🔵 **Both are gone now** (V108 for the call-graph pass, V110 for this one), and the route-bearing
/// assertion at the bottom is what keeps the two populations from drifting apart again: the first
/// misattribution survived precisely because every fixture here was route-FREE, so the pass being
/// blamed could not appear in any of them.
///
/// ⚠ **What this constant is a ceiling over.** These fixtures build `EngineConfig::default()` in Rust,
/// which leaves `default_off` empty — so they run analyses the PRODUCT ships off, `unimported-export`
/// among them (measured 2026-09-08: through the config surface, a config that names nothing does not
/// enable it). That makes this number an upper bound over the passes a user can turn on, not a
/// description of a default run. It is the right shape for a regression pin and the wrong shape for a
/// performance claim, and the difference is worth keeping straight.
const PARSES_PER_TS_FILE: u64 = 1;

/// The same fact for C#, added 2026-09-08 (review ledger V116) — and it did not exist until then.
///
/// 🔴 **The axis was wired for ONE language out of eight.** TypeScript got a memo and this pin on
/// 2026-08-08; C# got neither, and nothing anywhere could answer "how many times do we parse one
/// C# file". The answer turned out to be NINE engine entry points over sixteen in-crate
/// `parse_tree` call sites, with `parse_csharp` itself parsing once as a failure gate and again per
/// sub-extractor — a fact its own comment stated in prose while no number held it.
///
/// ⚠ This does NOT reach 1 the way TypeScript did, and the difference is structural: two C# passes
/// are whole-PROJECT (the namespace index and the provides project pass) and run in their own phase
/// over their own file lists, so their parses are not consecutive with the per-file lane's and a
/// one-entry memo cannot collapse them.
///
/// 📏 **Fourteen, and that is the number as shipped.** A one-entry memo like TypeScript's takes it to
/// three (8 files: 112 parses -> 24) and moves the wall clock by NOTHING — alternating A/B, fresh tree,
/// 4,000-term ternary: 12.26/9.88/10.11s without against 10.40/10.23/10.91s with. So the memo is not in
/// the tree and this constant pins the real cost rather than an aspiration;
/// `zzop_parser_csharp::parse_census`'s module doc holds that measurement and the revival condition.
/// ⇒ **A drop here is good news and needs its cause named**; a rise means a new pass or extractor.
///
/// (Fourteen, not the nine entry points the crate exposes, because several parse more than once
/// internally: `parse_csharp` alone parses as a failure gate and then again per sub-extractor.)
const PARSES_PER_CSHARP_FILE: u64 = 14;

#[test]
fn one_analyze_pass_parses_one_ts_file_this_many_times() {
    let dir = TempDir::new("zzop-engine-parse-census");
    fs::write(
        dir.path().join("a.ts"),
        "export function a(): number { return 1; }\n",
    )
    .unwrap();

    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    };

    // Baseline taken AFTER any lazily-initialized work the harness itself might have done, and the tree
    // holds one source file, so the delta is attributable to that file alone.
    zzop_parser_typescript::reset_parse_count();
    let out = analyze_tree(dir.path(), &cfg);
    let parses = zzop_parser_typescript::parse_count();

    // Non-vacuous: the file really was analyzed structurally, not skipped as oversized/lexical-only.
    assert!(
        out.nodes.iter().any(|n| n.path == "a.ts"),
        "fixture file must have been analyzed: {:?}",
        out.nodes.iter().map(|n| &n.path).collect::<Vec<_>>()
    );

    assert_eq!(
        parses, PARSES_PER_TS_FILE,
        "swc parses per .ts file per pass changed ({PARSES_PER_TS_FILE} -> {parses}). \
         That is the cold-run cost of every TypeScript file this engine parses at all. Since \
         2026-08-08 the expected value is ONE PER FILE, not one per extractor: a rise means \
         `parse_with_cm`'s one-entry memo stopped hitting (look for a caller that re-reads or \
         rewrites the file text between extractors, which changes the key), not that an extractor \
         was added."
    );
    // ── Second measurement, in the SAME test on purpose ─────────────────────────────────────────
    //
    // `parse_count` is a process-global `AtomicU64` and cargo runs tests in a thread per test, so two
    // `#[test]` functions in this file would race: each one's `reset_parse_count` zeroes the other's
    // measurement mid-flight. Measured while adding this — a second test made the first read 18
    // instead of 2. **This file holds exactly one test, and that is a constraint, not a style.**
    //
    // What it adds: the SHAPE of the cost. A one-file fixture cannot tell "the call-graph pass parses
    // every file again" from "one file lost a memo hit" — the two look identical at N = 1, and that
    // ambiguity is what let the old constant read 1 for months. At N = 8 they are 16 and 9.
    //
    // Measured while writing this: 16, both before and after V99's threading change. The doubling was
    // real and older than that change, and it did NOT belong to the call-graph pass — this comment's
    // first claim, and wrong, because that pass does not run over a route-free tree at all. It was
    // `dead_exports`, and it is gone (review ledger V110): 8 files now read 8.
    const MANY: usize = 8;
    let many = TempDir::new("zzop-engine-parse-census-many");
    for i in 0..MANY {
        fs::write(
            many.path().join(format!("f{i}.ts")),
            format!("export function f{i}(): number {{ return {i}; }}\n"),
        )
        .unwrap();
    }
    zzop_parser_typescript::reset_parse_count();
    let many_out = analyze_tree(many.path(), &cfg);
    let many_parses = zzop_parser_typescript::parse_count();
    assert_eq!(
        many_out.nodes.len(),
        MANY,
        "every fixture file must have been analyzed"
    );
    assert_eq!(
        many_parses,
        PARSES_PER_TS_FILE * MANY as u64,
        "{many_parses} parses for {MANY} files -- expected exactly {PARSES_PER_TS_FILE} per file. \
         BELOW this is a later pass learning to reuse the file pass's work (good: update \
         PARSES_PER_TS_FILE down and say which pass stopped). ABOVE it is a third pass, or the \
         one-entry memo no longer collapsing the file pass's consecutive parses (bad)."
    );

    // ── Third measurement, same test, DIFFERENT POPULATION ──────────────────────────────────────
    //
    // Everything above is route-FREE, and that is not a detail: `run_callgraph_rules` early-returns
    // when no call-graph rule is active, so the pass this file's prose used to blame never executed in
    // either fixture. A tree with routes is the only population that can see it, and until 2026-09-08
    // nothing measured that population — which is how the misattribution survived (review ledger V110).
    //
    // 📏 Measured on this 8-file route-bearing tree: 24 -> 16 when the call-graph extraction moved into
    // the per-file lane (V108), then 16 -> 8 when `dead_exports` stopped re-parsing (V110). 3N -> 2N ->
    // N. Three runs each at default threads and two more at RAYON_NUM_THREADS=1, every one identical.
    //
    // 🔵 That stability is itself a result. This file's header says an EQUALITY gate over a
    // route-bearing tree is "not available today, at any thread setting" — it was the CALL-GRAPH pass's
    // own memo interaction that made the count wobble (the observed max was 6 on a two-file tree). With
    // that pass no longer parsing, the wobble is gone and the equality below is a real gate. If it ever
    // starts flapping, that is the finding: something began parsing on a schedule the memo cannot see.
    const ROUTED: usize = 8;
    let routed = TempDir::new("zzop-engine-parse-census-routed");
    for i in 0..ROUTED {
        fs::write(
            routed.path().join(format!("r{i}.ts")),
            format!(
                "import {{ Controller, Get, Post }} from '@nestjs/common';\n\
                 export function helper{i}(): number {{ return {i}; }}\n\
                 @Controller('r{i}')\n\
                 export class C{i} {{\n\
                   @Post()\n\
                   write{i}() {{ return helper{i}(); }}\n\
                 }}\n"
            ),
        )
        .unwrap();
    }
    zzop_parser_typescript::reset_parse_count();
    let routed_out = analyze_tree(routed.path(), &cfg);
    let routed_parses = zzop_parser_typescript::parse_count();
    assert_eq!(
        routed_out.nodes.len(),
        ROUTED,
        "every routed fixture file must have been analyzed"
    );
    assert_eq!(
        routed_parses,
        PARSES_PER_TS_FILE * ROUTED as u64,
        "{routed_parses} parses for {ROUTED} ROUTE-BEARING files -- expected the same {PARSES_PER_TS_FILE} \
         per file as the route-free tree above. A route-bearing tree used to pay one MORE than it: the \
         call-graph pass re-read and re-parsed every source (V108). If this rises above the route-free \
         count, that pass (or a new one) is parsing again; if the two populations disagree at all, a \
         pass is charging for routes."
    );

    // ── Fourth measurement, same test, DIFFERENT LANGUAGE ───────────────────────────────────────
    //
    // Same test function, and for the same reason as the three above: `parse_count` is a
    // process-global `AtomicU64` in each parser crate, so a second `#[test]` here would race this
    // one's window. That constraint is why the C# arm is a paragraph rather than a file.
    //
    // Why C# is the second language to get this, of the seven that did not have it: an external review
    // measured a C# file that never finishes (review ledger V116), and localizing it turned up nine
    // independent parses of the same text. The count is the part of that a machine can hold.
    const CS: usize = 8;
    let cs = TempDir::new("zzop-engine-parse-census-csharp");
    for i in 0..CS {
        fs::write(
            cs.path().join(format!("C{i}.cs")),
            format!("class C{i} {{ int F{i}() {{ return {i}; }} }}\n"),
        )
        .unwrap();
    }
    zzop_parser_csharp::reset_parse_count();
    let cs_out = analyze_tree(cs.path(), &cfg);
    let cs_parses = zzop_parser_csharp::parse_count();
    assert_eq!(
        cs_out.nodes.len(),
        CS,
        "every C# fixture file must have been analyzed"
    );
    assert_eq!(
        cs_parses,
        PARSES_PER_CSHARP_FILE * CS as u64,
        "{cs_parses} tree-sitter parses for {CS} .cs files -- expected {PARSES_PER_CSHARP_FILE} per \
         file. ABOVE this means a caller stopped hitting the one-entry memo in \
         `zzop_parser_csharp::parse_memo` (look for a pass that re-reads the file text between \
         extractors, which changes the key), or that a new whole-project pass was added — those \
         cannot share the per-file lane's slot and each costs one more per file. BELOW it is a pass \
         learning to reuse work: update the constant down and say which one."
    );
}
