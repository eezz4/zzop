//! Contract 12: the parser × rule CAPABILITY MATRIX — machine-pinned reachability FACTS (which per-file
//! channel each parser environment actually projects) cross-checked against every shipped DSL rule's
//! `file_pattern`, so a rule can never silently ship admitting an environment whose required channel this
//! engine does not project.
//!
//! This contract exists because the fact it pins previously lived only as prose, and prose had ALREADY
//! drifted from the code: an audit found "loop spans are TS-only" stated somewhere while
//! `parser/parser-go/src/lang/loop_spans.rs` and `go/goroutine-in-loop`'s `trigger_in_loop` matcher had
//! moved reality out from under that sentence. This module replaces the sentence with a table read
//! straight from the engine's own per-language match arms (ground truth — `pipeline/fresh.rs` for
//! `symbols`/`io`, `pipeline/fresh/spans.rs` for the three span facts) and a
//! canary fixture per parser environment that empirically confirms the table against the REAL engine path
//! (`zzop_engine::analyze_tree`), the same path every other end-to-end rule test in this repo uses.
//!
//! ================================================================================================
//! CLAIM BOUNDARY — READ BEFORE TRUSTING A GREEN RUN HERE FOR ANYTHING ELSE
//! ================================================================================================
//! Every test below is a MINIMAL EXISTENCE check: "does the wiring for channel X exist for parser
//! environment Y" (declared present, canary non-empty) or "is the wiring for channel X on environment Y
//! definitively and structurally absent" (declared absent, canary empty). NONE of this is a firing
//! guarantee, and a green result here must NEVER be read as "a rule reaches results on real code" — that
//! is corpus dogfooding's job (running rules against real-world repositories and checking what actually
//! fires), not this meta-test's:
//!   - The NEGATIVE direction is the only STRONG claim this file makes: "this rule's `file_pattern` admits
//!     an environment whose required channel is declared absent" is machine-certain and means the rule is
//!     FOREVER-SILENT there — a real, provable defect class (the drift class this contract exists to
//!     catch). `every_shipped_rule_matcher_only_admits_environments_whose_required_channel_this_engine_projects`
//!     below asserts exactly this, nothing more.
//!   - The POSITIVE direction ("this environment's channel is present, therefore a rule admitting it
//!     reaches real findings") is deliberately NEVER asserted anywhere in this file. A present channel says
//!     nothing about whether any given rule's specific pattern ever matches real code in that
//!     environment — a rule can have every required channel present and still never fire on a real corpus
//!     (wrong pattern shape, a rare idiom, ...). Mistaking this contract for that guarantee would displace
//!     corpus dogfooding, which is exactly the failure mode the user who commissioned this file warned
//!     against.
//!   - BIDIRECTIONAL: a channel this table declares ABSENT for some environment that the canary fixture
//!     below finds non-empty FAILS just as loudly as the reverse — a capability GAIN (a parser learning a
//!     new channel) cannot hide behind a green run either. This is the exact shape of drift the "loop spans
//!     are TS-only" prose let slip through undetected.
//!
//! ## The declaration table (ground truth: `crates/engine/src/pipeline/fresh.rs`'s per-language match
//! arms for `symbols`/`io` and `fresh::spans`'s for `loop_spans`, NOT prose)
//!
//! Six channels, chosen at the granularity `crates/core/src/normalized.rs`'s `FileProjection` actually
//! models (`symbols: Vec<SourceSymbol>`, `loop_spans: Vec<(u32,u32)>`, `io: IoFacts { provides, consumes }`)
//! plus one further split `SourceSymbol::body_start`/`body_end` earns on its own (a symbol can exist with
//! no body span — Prisma's models are the concrete case, see below):
//! - `symbols` — `Matcher::SymbolScan`'s substrate: this environment's `FileArtifact::symbols` can be
//!   non-empty at all (regardless of whether any symbol carries a body span).
//! - `method_spans` — `Matcher::MethodScan`'s substrate: at least one projected `SourceSymbol` can carry
//!   BOTH `body_start` and `body_end` (`Some`). Independent of `symbols`: Prisma projects `symbols` (each
//!   model becomes a `SourceSymbolKind::Class` symbol, `parser/parser-prisma/src/analysis.rs`'s
//!   `build_common_ir`) but every one has `body_start: None, body_end: None` by construction — `symbols`
//!   present, `method_spans` absent, simultaneously, for the same environment.
//! - `decl_in_span` — the SEMANTICS half of `method_spans`, and the reason that column alone was
//!   dangerous to read. `method_spans: yes` says a span EXISTS; it says nothing about where the span
//!   starts, and until 2026-08-10 six parsers answered that differently (TS/Java/C#: the body block's
//!   `{` line; Go/Rust/Python: the first STATEMENT's line). Six `yes` cells therefore read as
//!   portability while a rule anchored on a DECLARATION — `async`, `@Transactional`, `[HttpGet]`,
//!   `#[get(...)]`, a parameter name — was writable for three of them by formatting coincidence and
//!   structurally unwritable for the other three. This cell asks the question the rule author actually
//!   has: **is the declaration line inside the span**, measured against a fixture whose signature wraps
//!   onto a second line, so no brace placement can make it pass by accident. `zzop_core::SourceSymbol`'s
//!   "Body span contract" is the rule; this column is what proves each environment obeys it.
//! - `loop_spans` — `Matcher::MethodScan`'s `trigger_in_loop` substrate.
//! - `io_provides` / `io_consumes` — `Matcher::IoScan`'s substrate: whether the ASSEMBLED WHOLE-TREE
//!   `IoScanTreeContext::provides`/`::consumes` (`analyze::assemble::provides::compose`'s output — the
//!   union of each language's direct `FileArtifact::io` channel AND every composed fragment channel:
//!   router-mount/procedure-router/controller-prefix) can carry an entry for this environment. This is
//!   deliberately the ASSEMBLED channel, not the raw per-file `FileArtifact::io` field alone: Python/Rust/Go
//!   HTTP route PROVIDES travel as `router_mount_fragments` and only become `IoProvide`s at assemble time
//!   (`pipeline::fresh`'s own doc), so a table keyed on the raw per-file field alone would under-report.
//!
//! `source_lines` (`Matcher::LineScan`'s substrate — plain file text) is NOT a column: it is universal,
//! including the lexical fallback (`pipeline::compute_fresh_artifact` still calls `eval_packs` for a
//! dispatch-`None` file, just with empty `symbols`/`io`/`loop_spans` — see that function's own doc). This
//! is verified empirically below too, not just asserted.
//!
//! - `call_sites` — `Matcher::CallScan`'s substrate: whether this environment's `FileArtifact::call_sites`
//!   (`crates/engine/src/pipeline/fresh/call_sites.rs`) can be non-empty at all.
//! - `string_literals` — `Matcher::LiteralScan`'s substrate: whether this environment's
//!   `FileArtifact::string_literals` (`crates/engine/src/pipeline/fresh/string_literals.rs`) can be
//!   non-empty at all. Landed all six structural languages at once (the A17 wave, 2026-08-03), so
//!   unlike `call_sites` it never had a partial-coverage phase; prisma/sql/lexical-fallback are `no`
//!   because PSL/DDL declare no named string BINDING (a `provider = "…"` config pair and a column
//!   DEFAULT are not declarations that bind a name to a literal in the sense the channel carries, and
//!   no parser arm projects them).
//!
//! | environment        | symbols | method_spans | decl_in_span | loop_spans | io_provides | io_consumes | call_sites | string_literals |
//! |---------------------|---------|---------------|--------------|------------|-------------|-------------|------------|-----------------|
//! | typescript          | yes     | yes           | yes          | yes        | yes         | yes         | yes        | yes             |
//! | python-3            | yes     | yes           | yes          | yes        | yes         | yes         | yes        | yes             |
//! | java-21             | yes     | yes           | yes          | yes        | yes         | yes         | yes        | yes             |
//! | rust                | yes     | yes           | yes          | yes        | yes         | yes         | yes        | yes             |
//! | go                  | yes     | yes           | yes          | yes        | yes         | yes         | yes        | yes             |
//! | prisma              | yes     | no            | no           | no         | yes         | no          | no         | no              |
//! | sql                 | no      | no            | no           | no         | yes         | no          | no         | no              |
//! | csharp              | yes     | yes           | yes          | yes        | yes         | yes         | yes        | yes             |
//! | lexical-fallback    | no      | no            | no           | no         | no          | no         | no         | no              |
//!
//! `decl_in_span` landed 2026-08-10 with the contract it measures, and all six structural rows would
//! have been RED the day before — each parser's own span pin was measured failing on exactly this
//! shape first (`parser-*/src/lang/symbols{,/tests}.rs`, the tests naming the declaration line). That
//! includes the three whose `method_spans: yes` had been carrying the shipped
//! `typescript/async-handler-no-try` rule, because a wrapped signature defeats the block-line reading
//! just as completely as it defeats the first-statement one. Which is the whole argument for the
//! column: the old table could not distinguish "this environment can host a declaration-anchored rule"
//! from "this environment happened to be formatted so one worked".
//!
//! `call_sites` flipped `no` -> `yes` for typescript/python-3 on 2026-08-03 in the wave (W1) that landed
//! the first two producers together with the three rules that read them — `reliability/console-in-be`
//! and `reliability/console-in-loop` over `console-write`, `reliability/env-outside-config` over
//! `env-read` — and for java-21/rust/go/csharp later the same day in W2 (six producer arms dispatched
//! by `pipeline/fresh/call_sites.rs`; rust emits the `env-read` family only, per its producer doc).
//! W2 (same date) flipped go/java-21/csharp (both families) and rust (`env-read` ONLY — the `println!`
//! judgment in `zzop_parser_rust::lang::call_sites`'s module doc: fact-layer console writes whose
//! consuming rules never admit `.rs`, so producing them would be a speculative fact) in the wave that
//! widened those same three rules' `file_pattern`s. NOTE the column is channel-existence, not
//! family-existence: rust's `yes` says "call sites can exist", and which FAMILIES a language produces is
//! each producer module doc's contract. The remaining three rows stay `no` because no producer arm
//! exists for them — for prisma/sql that is a statement about the language (no console write or env read
//! to write down). Keeping the cells honest is what makes the sweep below refuse a `file_pattern` that
//! reaches them.
//!
//! **W3 (`process-exec`) added no cell and no canary construct, and that is the column's contract
//! rather than an omission.** The column asks whether an environment projects the CHANNEL, never which
//! FAMILIES it projects — `call-scan-probe` deliberately names no `kind` for exactly this reason, so a
//! new family cannot move any cell and a probe construct for it would measure nothing new. Which
//! families each language emits is its producer module doc's contract, bound to the rules by
//! `zzop_core::RULE_READ_CALL_KINDS` (`call_kind_readers.rs`) rather than by this table. What W3 DID
//! add here is on the rule side: `required_channels` now reports `call_sites` for a `MethodScan` with
//! `require_call_kind` and for a `LineScan` with `line_call_kind`, so those structurally-gated rules
//! are swept for forever-silence exactly like a `CallScan` — a `LineScan` was previously exempt by
//! construction and no longer is whenever it carries that gate.
//!
//! **The column is measured now, and `call-scan-probe` is what measures it.** Until 2026-08-03 this
//! column was declared but never compared against a real run — a debt this doc named, because
//! `every_declared_environment_has_a_canary_that_measures_it` checks that every ENVIRONMENT is measured,
//! not that every CHANNEL is, and an all-`no` column could only under-claim so the debt stayed bounded.
//! The moment two cells say `yes` the column starts making a POSITIVE claim, so the probe landed in the
//! same change: every canary fixture whose language can express a console write or an environment read now
//! contains one (see `canary_files()`), and `call-scan-probe` — a `CallScan` with no `kind` and no
//! `callee_pattern`, so it asks only "does this environment project call sites AT ALL" — must fire on
//! exactly the `yes` rows. The negative rows are therefore proven against fixtures that genuinely contain
//! the construct, the same discipline `ZZOP_LOOP_MARKER` follows by sitting inside a real loop.
//!
//! `loop_spans` went `no` -> `yes` for python-3/java-21/rust/csharp on 2026-08-02 (each parser's
//! `lang/loop_spans` module + `pipeline/fresh/spans.rs`'s per-language arms): statement loops are a
//! span in all six structural languages; eager-vs-lazy callback arms are per-language calls whose
//! boundary `zzop_core::dsl::SourceFile::loop_spans`'s field doc owns (Python comprehensions in,
//! genexp/Rust adapters/Java Streams/C# LINQ out). Prisma/SQL stay `no` — no loop syntax exists to
//! span, a statement about the language rather than a missing capability.
//!
//! `java-21`'s `io_consumes` flipped `no` -> `yes` on 2026-08-02: `pipeline::io_projection`'s Java arm
//! now projects `zzop_parser_java_21::extract_java_http_consumes` (Spring `RestTemplate`/`WebClient`
//! literal egress, `!degraded`-gated like every other language's consume arm) — the half of the
//! cross-layer join Java lacked while already filling the provide side. The java canary fixture's
//! `getForObject` call is what keeps the flipped cell measured.
//!
//! `prisma`'s `io_provides` flipped `no` -> `yes` when the orphan this table originally DOCUMENTED was
//! wired up. The orphan was: `zzop_parser_prisma::build_common_ir` computed a `db-table` `IoProvide` per
//! model, but the ENGINE's sole call site (`crates/engine/src/pipeline/parsers.rs::parse_prisma`)
//! discarded `ir.ir.io`, keeping only `ir.ir.symbols`/`ir.ir.loc` — so the computed provide never reached
//! `assemble`'s whole-tree list. `parse_prisma` now returns that `IoFacts` and `pipeline::fresh`'s io
//! match has a `Language::Prisma` arm reading it, so a `model` block's table joins the cross-layer
//! `db-table` channel exactly like a `CREATE TABLE`'s does. The canary below (declared-present must be
//! non-empty) is what pins that it stayed wired: this row going back to `no` fails the test, and the
//! prisma fixture's model is what makes it non-empty. `io_consumes` stays `no` — PSL declares tables, it
//! never calls one (the CONSUME side is parser-typescript's `db_table_consume`, a `.ts` environment).
//!
//! `lexical-fallback` has no parser crate (it is `dispatch`'s `None` arm, not `Language::*`) — it is a
//! synthetic 9th row, excluded from the parser-crate SSOT pin below.
//!
//! ## `function_spans` is deliberately NOT a sixth column (2026-07-25)
//!
//! `MethodScan::after_in_same_function`'s substrate (`FileProjection::function_spans`,
//! `pipeline::fresh::spans`) is **TypeScript only** — every other environment,
//! Go included, is a blank. That asymmetry is published in `docs/NORMALIZED_AST.md`, `docs/rules/
//! dsl-reference.md`, and `crates/cache/src/ir_slice.rs`'s module doc, but it is NOT pinned here, and the
//! reason is a property of the channel rather than laziness: this one's absent-fact degrade is a
//! **no-op**, not silence. A `trigger_in_loop` rule cannot fire without `loop_spans`, which is exactly
//! what makes the `loop-scan-probe` below a clean two-sided canary; an `after_in_same_function` rule
//! fires the SAME as an ungated one without `function_spans`, so any probe would have to be INVERTED
//! ("declared present ⇒ probe must NOT fire") and would additionally need every fixture to carry two
//! sibling closures inside one symbol body — a shape half these languages express differently and two
//! (Prisma, SQL) cannot express at all. The inverted, fixture-heavy probe was judged to cost more
//! confusion than the drift it would catch. **If a second language ever learns `function_spans`, revisit
//! this**: at two producers the column earns its fixtures.
//!
//! ## `test_spans` is not a column either, and for a STRONGER reason (2026-08-02)
//!
//! `zzop_core::dsl::SourceFile::test_spans` (`pipeline::fresh::spans`, **Rust
//! only**) is SUBTRACTIVE: no rule REQUIRES it, so its absence can never make a rule forever-silent —
//! which is the only defect class this file's strong negative claim is about. A missing `test_spans`
//! over-reports; every other channel here under-reports. The column would therefore pin a fact that
//! cannot produce the failure this contract exists to catch.
//!
//! It is also unprobeable per environment in the way the columns above are: the fixture that would prove
//! the channel ABSENT has to contain a test region, and outside Rust there is no such syntax to write —
//! `.py`/`.ts`/`.go` name their tests in the PATH, which is the rule packs' `${test-paths-stories}`
//! exclusion's business, not a parser channel's. A blank row would read as a gap when it is a statement
//! that the other axis already covers that language.
//!
//! **That last sentence was FALSE for three of the languages it named, from the day it was written until
//! 2026-08-10.** The premise held — Go, Python and C# do name their tests in the path — but the fragment
//! did not match their spellings: it knew `tests/`, `spec/` and the `.test.`/`.spec.` dot-infix, i.e.
//! TypeScript's conventions and only TypeScript's, so `handler_test.go`, `test_login.py` and
//! `UserTests.cs` were all judged as production code. The worked example in the paragraph above is a
//! `.ts` path, which is exactly how the claim survived review: every example given was one that happened
//! to work, and the two languages with no example were the two that were broken. The fragment now carries
//! every convention (`zzop_core::is_test_file` reads the same string), so the sentence is true — but the
//! shape of the mistake is worth keeping: a claim about N languages backed by one language's example.
//!
//! Where the drift IS pinned, two-sided, on the one environment that has the channel:
//! `crates/engine/tests/analyze_rust_test_spans.rs` — same violation inside and outside a `#[cfg(test)]`
//! region, one finding, on the shipped line. **If a second language ever learns `test_spans`, revisit
//! this**: at two producers the asymmetry stops being self-evident from the one arm in `fresh.rs`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, IoDirection, Matcher, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

// -------------------------------------------------------------------------------------------------------
// Declaration table
// -------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Capabilities {
    symbols: bool,
    method_spans: bool,
    decl_in_span: bool,
    loop_spans: bool,
    io_provides: bool,
    io_consumes: bool,
    call_sites: bool,
    string_literals: bool,
}

/// The declaration table transcribed from this module's own doc — see there for the ground-truth
/// citations per environment. Order matches the doc table (parser-crate rows first, `lexical-fallback`
/// last as the synthetic 9th row).
const ENVIRONMENTS: &[(&str, Capabilities)] = &[
    (
        // `call_sites` flipped `no` -> `yes` on 2026-08-03 with W1's producer
        // (`zzop_parser_typescript::extract_call_sites` — `console-write` + `env-read`). The canary
        // fixture's `console.log` and `process.env` reads are what keep this row measured.
        "typescript",
        Capabilities {
            symbols: true,
            method_spans: true,
            decl_in_span: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
            call_sites: true,
            string_literals: true,
        },
    ),
    (
        // `call_sites` flipped in the same wave, via `zzop_parser_python_3::extract_call_sites` — the
        // canary fixture's `print(...)` and `os.getenv(...)` keep it measured.
        "python-3",
        Capabilities {
            symbols: true,
            method_spans: true,
            decl_in_span: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
            call_sites: true,
            string_literals: true,
        },
    ),
    (
        // `io_consumes` flipped `no` -> `yes` on 2026-08-02: `pipeline::io_projection`'s Java arm now
        // projects `zzop_parser_java_21::extract_java_http_consumes` (RestTemplate/WebClient literal
        // egress) — the canary fixture's `getForObject` call is what keeps this row measured.
        // `call_sites` flipped in W2 (2026-08-03) via `zzop_parser_java_21::extract_call_sites` — the
        // canary fixture's `System.out.println` and `System.getenv` keep it measured.
        "java-21",
        Capabilities {
            symbols: true,
            method_spans: true,
            decl_in_span: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
            call_sites: true,
            string_literals: true,
        },
    ),
    (
        // `call_sites` flipped in W2 (2026-08-03) via `zzop_parser_rust::extract_call_sites`, whose one
        // family is `env-read` — the canary fixture's `std::env::var` keeps it measured (its `println!`
        // deliberately projects nothing; the producer module doc owns that judgment).
        "rust",
        Capabilities {
            symbols: true,
            method_spans: true,
            decl_in_span: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
            call_sites: true,
            string_literals: true,
        },
    ),
    (
        // `call_sites` flipped in W2 (2026-08-03) via `zzop_parser_go::extract_call_sites` — the canary
        // fixture's `fmt.Println` and `os.Getenv` keep it measured.
        "go",
        Capabilities {
            symbols: true,
            method_spans: true,
            decl_in_span: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
            call_sites: true,
            string_literals: true,
        },
    ),
    (
        "prisma",
        Capabilities {
            symbols: true,
            method_spans: false,
            decl_in_span: false,
            loop_spans: false,
            io_provides: true,
            io_consumes: false,
            call_sites: false,
            string_literals: false,
        },
    ),
    (
        "sql",
        Capabilities {
            symbols: false,
            method_spans: false,
            decl_in_span: false,
            loop_spans: false,
            io_provides: true,
            io_consumes: false,
            call_sites: false,
            string_literals: false,
        },
    ),
    (
        // `call_sites` flipped in W2 (2026-08-03) via `zzop_parser_csharp::extract_call_sites` — the
        // canary fixture's `System.Console.WriteLine` and `System.Environment.GetEnvironmentVariable`
        // keep it measured.
        "csharp",
        Capabilities {
            symbols: true,
            method_spans: true,
            decl_in_span: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
            call_sites: true,
            string_literals: true,
        },
    ),
    (
        "lexical-fallback",
        Capabilities {
            symbols: false,
            method_spans: false,
            decl_in_span: false,
            loop_spans: false,
            io_provides: false,
            io_consumes: false,
            call_sites: false,
            string_literals: false,
        },
    ),
];

fn capabilities_for(env: &str) -> Capabilities {
    ENVIRONMENTS
        .iter()
        .find(|(e, _)| *e == env)
        .unwrap_or_else(|| {
            panic!(
                "capability_matrix: no declared ENVIRONMENTS row for {env:?} — add one (with a \
                 fresh.rs-cited justification) before referencing it"
            )
        })
        .1
}

// -------------------------------------------------------------------------------------------------------
// Parser-crate SSOT pin — the environment list above must never silently omit a 9th parser.
// -------------------------------------------------------------------------------------------------------

/// Same SSOT `scripts/check-version-lists-parsers.sh` pins: every `parser/*/Cargo.toml` crate must appear
/// in `crates/facade/src/version.rs::version_string()`'s `zzop-parser-<x>={}` format-string tokens. This
/// test additionally requires this file's own `ENVIRONMENTS` table (excluding the synthetic
/// `lexical-fallback` row, which has no parser crate) to have EXACTLY one row per token — so a 9th parser
/// crate fails THIS test (not just the shell guard) until a capability row exists for it.
///
/// MINIMAL-EXISTENCE scope: this only pins the environment LIST is complete; it asserts nothing about any
/// channel value in that row (a wrong value only fails once a canary test below exercises it).
#[test]
fn environments_table_has_exactly_one_row_per_zzop_parser_token_in_version_string() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../facade/src/version.rs");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("capability_matrix: cannot read {}: {e}", path.display()));
    let re = regex::Regex::new(r"zzop-parser-([a-z0-9-]+)=").expect("static regex");
    let mut scanned: Vec<&str> = re
        .captures_iter(&text)
        .map(|c| c.get(1).expect("capture group 1").as_str())
        .collect();
    scanned.sort_unstable();
    scanned.dedup();
    assert!(
        !scanned.is_empty(),
        "capability_matrix: found zero `zzop-parser-<x>=` tokens in {} — did version_string()'s format \
         string change shape? (this test's extraction regex may need updating alongside it)",
        path.display()
    );
    let declared: std::collections::BTreeSet<&str> = ENVIRONMENTS
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| *k != "lexical-fallback")
        .collect();
    let scanned: std::collections::BTreeSet<&str> = scanned.into_iter().collect();
    assert_eq!(
        declared, scanned,
        "capability_matrix's ENVIRONMENTS table (excluding the synthetic `lexical-fallback` row) must \
         have EXACTLY one row per `zzop-parser-<x>=` token version_string() reports — the same 8-parser \
         list `scripts/check-version-lists-parsers.sh` pins. A parser crate missing from either side means \
         either this table needs a new row (a 9th parser shipped with no declared capabilities) or \
         version.rs is stale (a different, older contract already fails first)."
    );
}

// -------------------------------------------------------------------------------------------------------
// Canary fixtures — ONE tiny source file per environment, all analyzed together in ONE `analyze_tree` run.
// -------------------------------------------------------------------------------------------------------

/// A self-cleaning temp directory — same std-only mkdtemp idiom every other `analyze_tree`-driving test in
/// this repo hand-rolls (see e.g. `rules/dsl/go/go.rs`, `crates/engine/tests/analyze_asset_ref.rs`).
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
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

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `(environment key, canary filename, canary source)`. Every fixture carries `ZZOP_LINE_MARKER` (proves
/// `source_lines` universality), `ZZOP_METHOD_MARKER` inside its own real function/method body where that
/// environment has one (proves `method_spans`), and `ZZOP_LOOP_MARKER` inside a real `for`/loop construct
/// inside that same body where one exists (proves `loop_spans` — deliberately placed inside a REAL loop
/// even for environments this table declares `loop_spans: false`, so the negative is proven bidirectionally
/// against a source that genuinely has a loop, not merely against a fixture that omits one). A symbol
/// literally named `ZzopCanaryTarget` gives every symbol-projecting environment something to declare.
///
/// Every fixture whose language has a function ALSO carries `ZZOP_DECL_MARKER` — the `decl_in_span`
/// column's probe. Its placement is the whole measurement and is not negotiable: a trailing comment on
/// the DECLARATION line of a `zzopCanaryDeclSpan` function whose signature deliberately WRAPS onto a
/// second line, so the opening brace is nowhere near it. A brace-line span misses it, a
/// first-statement span misses it, and only a declaration-anchored span contains it — which is why the
/// fixture cannot be shortened to a one-line header without turning the column back into a formatting
/// coincidence. Prisma/SQL/Kotlin carry the marker as a bare comment, the same discipline
/// `ZZOP_LOOP_MARKER` follows: their `no` is proven against a file that genuinely contains the string.
///
/// Every fixture ALSO carries a `zzopCanaryCallSites` function holding that language's own console write
/// and environment read (`console.log`/`process.env`, `print`/`os.getenv`, `System.out.println`/
/// `System.getenv`, `println!`/`std::env::var`, `fmt.Println`/`os.Getenv`, `Console.WriteLine`/
/// `Environment.GetEnvironmentVariable`, Kotlin's `println`/`System.getenv`) — the `call_sites` column's
/// probe, on exactly the discipline `ZZOP_LOOP_MARKER` follows: a row declared `call_sites: false` is
/// proven false against a fixture that genuinely contains the construct, never against one that omits it,
/// so a producer arm added without flipping its cell turns this file red instead of passing quietly.
/// Prisma and SQL are the two exceptions and they are statements rather than gaps: PSL and DDL have no
/// console write and no environment read to write down, the same reason their `ZZOP_LOOP_MARKER` is a
/// bare comment.
///
/// Every fixture whose language can express one ALSO carries a named string binding
/// (`zzopCanaryBoundLiteral` in that language's casing) — the `string_literals` column's probe, same
/// discipline: a `string_literals: false` row is proven against a fixture that genuinely contains the
/// construct (the Kotlin fixture's `val`), and Prisma/SQL are again the two languages with nothing to
/// write down (no declaration binds a name to a string literal — see the module doc's column note).
fn canary_files() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "typescript",
            "canary.ts",
            r#"// ZZOP_LINE_MARKER

function ZzopCanaryTarget() {
  // ZZOP_METHOD_MARKER
  for (let i = 0; i < 1; i++) {
    // ZZOP_LOOP_MARKER
    zzopLoopBody();
  }
}

function zzopLoopBody() {}

function zzopCanaryDeclSpan( // ZZOP_DECL_MARKER
) {
  zzopLoopBody();
}

function zzopEgress() {
  fetch("https://api.example.com/zzop-canary");
}

function zzopCanaryCallSites() {
  console.log("zzop-canary");
  return process.env.ZZOP_CANARY;
}

const zzopCanaryBoundLiteral = "zzop-canary-value";

apiRoutes.get("/zzop-canary-ts", zzopCanaryHandler);

function zzopCanaryHandler() {}
"#,
        ),
        (
            "python-3",
            "canary.py",
            r#"# ZZOP_LINE_MARKER
from fastapi import FastAPI
import os
import requests

app = FastAPI()


@app.get("/zzop-canary-py")
def zzop_canary_route():
    return 1


def ZzopCanaryTarget():
    # ZZOP_METHOD_MARKER
    for i in range(1):
        # ZZOP_LOOP_MARKER
        zzop_loop_body()


def zzop_loop_body():
    pass


def zzop_canary_decl_span(  # ZZOP_DECL_MARKER
):
    zzop_loop_body()


def zzop_egress():
    requests.get("https://api.example.com/zzop-canary")


def zzop_canary_call_sites():
    print("zzop-canary")
    return os.getenv("ZZOP_CANARY")


zzop_canary_bound_literal = "zzop-canary-value"
"#,
        ),
        (
            "java-21",
            "Canary.java",
            r#"// ZZOP_LINE_MARKER
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.client.RestTemplate;

@RestController
class ZzopCanaryController {
  @GetMapping("/zzop-canary-java")
  void handle() {}
}

class ZzopCanaryTarget {
  void run() {
    // ZZOP_METHOD_MARKER
    for (int i = 0; i < 1; i++) {
      // ZZOP_LOOP_MARKER
      zzopLoopBody();
    }
  }

  void zzopLoopBody() {}

  void zzopCanaryDeclSpan( // ZZOP_DECL_MARKER
  ) {
    zzopLoopBody();
  }

  void zzopEgress() {
    new RestTemplate().getForObject("https://api.example.com/zzop-canary", String.class);
  }

  String zzopCanaryCallSites() {
    System.out.println("zzop-canary");
    return System.getenv("ZZOP_CANARY");
  }

  String zzopCanaryBoundLiteral = "zzop-canary-value";
}
"#,
        ),
        (
            "rust",
            "canary.rs",
            r#"// ZZOP_LINE_MARKER
use axum::Router;
use axum::routing::get;
use reqwest;

fn main() {
    let app = Router::new().route("/zzop-canary-rust", get(zzop_canary_handler));
}

fn zzop_canary_handler() {}

fn ZzopCanaryTarget() {
    // ZZOP_METHOD_MARKER
    for i in 0..1 {
        // ZZOP_LOOP_MARKER
        zzop_loop_body();
    }
}

fn zzop_loop_body() {}

fn zzop_canary_decl_span( // ZZOP_DECL_MARKER
) {
    zzop_loop_body();
}

fn zzop_egress() {
    reqwest::get("https://api.example.com/zzop-canary");
}

fn zzop_canary_call_sites() -> Result<String, std::env::VarError> {
    println!("zzop-canary");
    std::env::var("ZZOP_CANARY")
}

const ZZOP_CANARY_BOUND_LITERAL: &str = "zzop-canary-value";
"#,
        ),
        (
            "go",
            "canary.go",
            r#"package main

// ZZOP_LINE_MARKER

import (
	"fmt"
	"net/http"
	"os"

	"github.com/gin-gonic/gin"
)

func main() {
	r := gin.Default()
	r.GET("/zzop-canary-go", zzopCanaryHandler)
}

func zzopCanaryHandler() {}

// NOTE: unlike every other canary fixture, the ZZOP_METHOD_MARKER/ZZOP_LOOP_MARKER comments below are
// NOT the first statement inside ZzopCanaryTarget's body: tree-sitter-go's grammar makes a leading
// standalone comment a real named child of the enclosing block (unlike swc/ruff/syn/tree-sitter-c-sharp,
// which treat comments as trivia), which used to make `body_line_range`'s "first named child" walk see
// the comment instead of the `for_statement` and report `body_start: None` for the WHOLE function --
// this canary caught that the hard way while this file was being written. Trailing-comment placement
// (same line as a real statement) sidesteps it without touching parser/parser-go itself.
//
// That workaround is no longer load-bearing as of 2026-08-10: the span contract anchors `body_start` on
// the declaration and `body_end` on the closing brace, so `body_line_range` reads nothing inside the
// block and no comment placement can move either boundary. The placement is kept anyway, deliberately.
// It costs nothing, and a fixture that would have caught a real bug is worth more than a tidier one --
// if some future change reintroduces content-dependent boundaries, this shape still exercises them.
func ZzopCanaryTarget() {
	for i := 0; i < 1; i++ {
		zzopLoopBody() // ZZOP_LOOP_MARKER
	}
	_ = "ZZOP_METHOD_MARKER"
}

func zzopLoopBody() {}

func zzopCanaryDeclSpan( // ZZOP_DECL_MARKER
) {
	zzopLoopBody()
}

func zzopEgress() {
	http.Get("/zzop-canary")
}

func zzopCanaryCallSites() string {
	fmt.Println("zzop-canary")
	return os.Getenv("ZZOP_CANARY")
}

const zzopCanaryBoundLiteral = "zzop-canary-value"
"#,
        ),
        (
            "prisma",
            "schema.prisma",
            r#"// ZZOP_LINE_MARKER
// ZZOP_METHOD_MARKER
// ZZOP_LOOP_MARKER
// ZZOP_DECL_MARKER
model ZzopCanaryTarget {
  id String @id
}
"#,
        ),
        (
            "sql",
            "canary.sql",
            r#"-- ZZOP_LINE_MARKER
-- ZZOP_METHOD_MARKER
-- ZZOP_LOOP_MARKER
-- ZZOP_DECL_MARKER
CREATE TABLE zzop_canary_table (id INT);
"#,
        ),
        (
            "csharp",
            "Canary.cs",
            r#"// ZZOP_LINE_MARKER
using System.Net.Http;

public class ZzopCanaryController {
    [HttpGet]
    public string Get() { return ""; }
}

public class ZzopCanaryTarget {
    public void Run() {
        // ZZOP_METHOD_MARKER
        for (int i = 0; i < 1; i++) {
            // ZZOP_LOOP_MARKER
            ZzopLoopBody();
        }
    }

    public void ZzopLoopBody() {}

    public void ZzopCanaryDeclSpan( // ZZOP_DECL_MARKER
    ) {
        ZzopLoopBody();
    }

    public async void ZzopEgress() {
        var client = new HttpClient();
        var r = client.GetAsync("https://api.example.com/zzop-canary");
    }

    public string ZzopCanaryCallSites() {
        System.Console.WriteLine("zzop-canary");
        return System.Environment.GetEnvironmentVariable("ZZOP_CANARY");
    }

    public string ZzopCanaryBoundLiteral = "zzop-canary-value";
}
"#,
        ),
        (
            "lexical-fallback",
            "canary.kt",
            r#"// ZZOP_LINE_MARKER
// ZZOP_METHOD_MARKER
// ZZOP_LOOP_MARKER
// ZZOP_DECL_MARKER
// .kt is not dispatched by any parser this engine ships today (crates/engine/src/dispatch.rs's
// dispatch_by_extension has no "kt" arm) -- this file exercises the lexical-fallback path on purpose.
// The console write and env read below are REAL Kotlin ones, so `call_sites: false` on this row is
// proven against a file that genuinely contains both rather than against one that omits them.
fun main() {}

fun zzopCanaryCallSites(): String? {
    println("zzop-canary")
    return System.getenv("ZZOP_CANARY")
}

// A REAL Kotlin named string binding, same discipline as the console write above: the
// `string_literals: false` cell on this row is proven against a file that genuinely contains the
// construct, never against one that omits it.
val zzopCanaryBoundLiteral = "zzop-canary-value"
"#,
        ),
    ]
}

/// The three canary tests below only ever iterate `canary_files()`. That makes the FIXTURE LIST, not
/// the declared table, their subject set — so an `ENVIRONMENTS` row with no canary is a capability
/// claim nobody has ever measured, and the three tests stay green while saying nothing about it.
///
/// Measured 2026-07-28 (D22 sweep): deleting the `java-21` tuple from `canary_files()` AND flipping
/// that row's `io_consumes` from the correct `false` to a wrong `true` left **all five tests in this
/// file green**. The consequence is worse than an unverified row. `capabilities_for()` feeds
/// `every_shipped_rule_matcher_only_admits_environments_whose_required_channel_this_engine_projects`,
/// whose own doc calls its negative claim STRONG and machine-certain — so a wrong row does not merely
/// go unchecked, it makes that sweep **bless a forever-silent rule as reachable**.
///
/// Today the two sets match exactly (9/9). Nothing kept them that way, which is the whole finding: the
/// sibling `environments_table_has_exactly_one_row_per_zzop_parser_token_in_version_string` pin forces
/// `ENVIRONMENTS` to have a row per shipped parser, and nothing then forces that row to be MEASURED.
#[test]
fn every_declared_environment_has_a_canary_that_measures_it() {
    let declared: BTreeSet<&str> = ENVIRONMENTS.iter().map(|(e, _)| *e).collect();
    let measured: BTreeSet<&str> = canary_files().into_iter().map(|(e, _, _)| e).collect();

    assert!(
        !declared.is_empty() && !measured.is_empty(),
        "capability_matrix: declared={} measured={} — an empty side makes this pin vouch for nothing",
        declared.len(),
        measured.len()
    );

    let unmeasured: Vec<&&str> = declared.difference(&measured).collect();
    assert!(
        unmeasured.is_empty(),
        "capability_matrix: these ENVIRONMENTS rows have NO canary fixture, so their declared \
         capabilities have never been compared against a real engine run: {unmeasured:?}. Add a \
         `canary_files()` entry that exercises the channels the row claims. A row that is only \
         asserted is worse than absent here, because `capabilities_for()` hands it to the rule-side \
         sweep as machine-certain ground truth."
    );

    let undeclared: Vec<&&str> = measured.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "capability_matrix: these canary fixtures name an environment with no ENVIRONMENTS row: \
         {undeclared:?}. `capabilities_for()` would panic on them at run time; declare the row (with \
         its justification) or drop the fixture."
    );
}

fn write_canary_files(dir: &TempDir) {
    for (_, filename, content) in canary_files() {
        dir.write(filename, content);
    }
}

fn canary_engine_output(dir: &TempDir, packs: Vec<RulePackDef>) -> AnalyzeOutput {
    analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "capability-matrix-canary".to_string(),
            packs,
            ..EngineConfig::default()
        },
    )
}

fn file_has_any_symbol(out: &AnalyzeOutput, file: &str) -> bool {
    out.ir.ir.symbols.iter().any(|s| s.file == file)
}

fn file_has_method_span(out: &AnalyzeOutput, file: &str) -> bool {
    out.ir
        .ir
        .symbols
        .iter()
        .any(|s| s.file == file && s.body_start.is_some() && s.body_end.is_some())
}

fn file_has_io_provide(out: &AnalyzeOutput, file: &str) -> bool {
    out.ir
        .ir
        .io
        .as_ref()
        .is_some_and(|io| io.provides.iter().any(|p| p.file == file))
}

fn file_has_io_consume(out: &AnalyzeOutput, file: &str) -> bool {
    out.ir
        .ir
        .io
        .as_ref()
        .is_some_and(|io| io.consumes.iter().any(|c| c.file == file))
}

/// Canary #1 (MINIMAL EXISTENCE — see module doc's claim boundary): `symbols` / `method_spans` per
/// environment, read directly off the REAL `analyze_tree` output (`AnalyzeOutput::ir::ir::symbols`) — no
/// synthetic DSL rule needed, since `SourceSymbol`/`body_start`/`body_end` are already part of that output.
#[test]
fn canary_symbols_and_method_spans_channels_match_the_declared_table() {
    let dir = TempDir::new("zzop-capability-matrix-symbols");
    write_canary_files(&dir);
    let out = canary_engine_output(&dir, Vec::new());

    let mut mismatches = Vec::new();
    for (env, file, _) in canary_files() {
        let caps = capabilities_for(env);
        let has_symbols = file_has_any_symbol(&out, file);
        if has_symbols != caps.symbols {
            mismatches.push(format!(
                "{env} ({file}): declared symbols={}, engine actually projected {has_symbols} \
                 (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc)",
                caps.symbols
            ));
        }
        let has_spans = file_has_method_span(&out, file);
        if has_spans != caps.method_spans {
            mismatches.push(format!(
                "{env} ({file}): declared method_spans={}, engine actually projected {has_spans} \
                 (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc)",
                caps.method_spans
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "capability_matrix: ENVIRONMENTS table's symbols/method_spans columns disagree with the real \
         engine projection (see this module's doc for the exact claim boundary): {mismatches:#?}"
    );
}

/// Canary #2 (MINIMAL EXISTENCE): `io_provides` / `io_consumes` per environment, read off the ASSEMBLED
/// whole-tree `AnalyzeOutput::ir::ir::io` — the same channel `Matcher::IoScan` queries (composed fragments
/// included, not just each language's raw per-file `FileArtifact::io`; see module doc).
#[test]
fn canary_io_provides_and_io_consumes_channels_match_the_declared_table() {
    let dir = TempDir::new("zzop-capability-matrix-io");
    write_canary_files(&dir);
    let out = canary_engine_output(&dir, Vec::new());

    let mut mismatches = Vec::new();
    for (env, file, _) in canary_files() {
        let caps = capabilities_for(env);
        let has_provide = file_has_io_provide(&out, file);
        if has_provide != caps.io_provides {
            mismatches.push(format!(
                "{env} ({file}): declared io_provides={}, engine actually projected {has_provide} \
                 (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc)",
                caps.io_provides
            ));
        }
        let has_consume = file_has_io_consume(&out, file);
        if has_consume != caps.io_consumes {
            mismatches.push(format!(
                "{env} ({file}): declared io_consumes={}, engine actually projected {has_consume} \
                 (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc)",
                caps.io_consumes
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "capability_matrix: ENVIRONMENTS table's io_provides/io_consumes columns disagree with the real \
         assembled engine projection (see this module's doc for the exact claim boundary): {mismatches:#?}"
    );
}

/// The canary probe pack — TWO rules, loaded through the real `load_dsl_packs` path (same JSON schema
/// every shipped pack uses), never hand-constructed `RulePackDef` structs. `.*` admits every canary file;
/// each probe's own doc states its MINIMAL-EXISTENCE scope, matching this module's claim boundary.
const CANARY_PROBE_PACK_JSON: &str = r#"{
  "id": "capability-matrix-canary",
  "schema_version": 1,
  "framework": "any",
  "rules": [
    {
      "id": "line-scan-probe",
      "severity": "info",
      "message": "capability-matrix MINIMAL-EXISTENCE probe (NOT a real finding): fires on ZZOP_LINE_MARKER -- proves the source_lines channel (LineScan's substrate) is universal, including the lexical fallback. A miss here would mean an environment's file text is not even reaching per-file rule evaluation, a far more serious break than any channel this contract tracks.",
      "matcher": {
        "type": "line-scan",
        "file_pattern": ".*",
        "line_pattern": "ZZOP_LINE_MARKER"
      }
    },
    {
      "id": "loop-scan-probe",
      "severity": "info",
      "message": "capability-matrix MINIMAL-EXISTENCE probe (NOT a real finding): fires only when ZZOP_LOOP_MARKER sits inside both a projected symbol body span AND a projected loop span -- proves the loop_spans channel (MethodScan::trigger_in_loop's substrate) is present or definitively absent per environment. A miss does NOT mean the source lacks a real loop (every canary fixture's marker sits inside a genuine for-loop) -- it means this engine does not yet project loop spans for that environment. See this file's module doc for the full claim boundary.",
      "matcher": {
        "type": "method-scan",
        "file_pattern": ".*",
        "patterns": [{ "pattern": "ZZOP_LOOP_MARKER", "label": "hit" }],
        "trigger": "hit",
        "trigger_in_loop": true
      }
    },
    {
      "id": "decl-scan-probe",
      "severity": "info",
      "message": "capability-matrix MINIMAL-EXISTENCE probe (NOT a real finding): fires only when ZZOP_DECL_MARKER -- which sits in a trailing comment on a DECLARATION line whose signature wraps onto the next line -- falls inside a projected symbol body span. This is the decl_in_span column: the semantics half of method_spans. A miss does NOT mean the environment projects no spans (method_spans answers that separately) -- it means its spans start after the declaration, which makes every declaration-anchored rule concept (async, @Transactional, [HttpGet], #[get(...)]) structurally unwritable there. See this file's module doc and zzop_core::SourceSymbol's Body span contract.",
      "matcher": {
        "type": "method-scan",
        "file_pattern": ".*",
        "patterns": [{ "pattern": "ZZOP_DECL_MARKER", "label": "hit" }],
        "trigger": "hit"
      }
    },
    {
      "id": "call-scan-probe",
      "severity": "info",
      "message": "capability-matrix MINIMAL-EXISTENCE probe (NOT a real finding): fires on ANY projected call site -- no `kind` and no `callee_pattern`, deliberately, so it asks the one question this column is about (does this environment project the call_sites channel at all) rather than any question about a particular family. A miss does NOT mean the fixture contains no console write and no environment read -- every fixture whose language can express one contains both -- it means this engine does not project call sites for that environment. See this file's module doc for the full claim boundary.",
      "matcher": {
        "type": "call-scan",
        "file_pattern": ".*"
      }
    },
    {
      "id": "literal-scan-probe",
      "severity": "info",
      "message": "capability-matrix MINIMAL-EXISTENCE probe (NOT a real finding): fires on ANY projected bound string literal -- no `name_pattern` and no `entropy_min`, deliberately, so it asks the one question this column is about (does this environment project the string_literals channel at all). A miss does NOT mean the fixture contains no named string binding -- every fixture whose language can express one contains `zzopCanaryBoundLiteral` (or its casing-convention twin) -- it means this engine does not project bound string literals for that environment. See this file's module doc for the full claim boundary.",
      "matcher": {
        "type": "literal-scan",
        "file_pattern": ".*"
      }
    }
  ]
}
"#;

fn canary_probe_pack() -> RulePackDef {
    let dir = TempDir::new("zzop-capability-matrix-probe-pack");
    dir.write("capability-matrix-canary.json", CANARY_PROBE_PACK_JSON);
    let result = load_dsl_packs(dir.path());
    assert!(
        result.errors.is_empty(),
        "capability_matrix: canary probe pack failed to load: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "capability-matrix-canary")
        .expect("capability-matrix-canary probe pack present")
}

/// Canary #3 (MINIMAL EXISTENCE): `loop_spans` per environment. Unlike `symbols`/`io`, `loop_spans` is
/// never serialized into `AnalyzeOutput` (it is consumed internally by `Matcher::MethodScan` only), so this
/// is the one channel this file proves through a real (not synthetic-in-spirit — loaded via the same
/// `load_dsl_packs` path every shipped pack uses) `trigger_in_loop` rule instead of direct output
/// inspection. Also empirically confirms `source_lines` universality (`line-scan-probe`) as a bonus sanity
/// check, though that column is not part of the declared table (it is constant-true, see module doc).
#[test]
fn canary_loop_spans_channel_matches_the_declared_table_via_a_trigger_in_loop_probe_rule() {
    let dir = TempDir::new("zzop-capability-matrix-loopspans");
    write_canary_files(&dir);
    let out = canary_engine_output(&dir, vec![canary_probe_pack()]);

    let line_scan_hits: std::collections::BTreeSet<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "capability-matrix-canary/line-scan-probe")
        .map(|f| f.file.as_str())
        .collect();
    let loop_scan_hits: std::collections::BTreeSet<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "capability-matrix-canary/loop-scan-probe")
        .map(|f| f.file.as_str())
        .collect();

    let mut mismatches = Vec::new();
    for (env, file, _) in canary_files() {
        // source_lines: universal, including the lexical fallback — not a declared-table column, verified
        // separately here (every fixture carries the marker, so a miss anywhere is always a bug).
        if !line_scan_hits.contains(file) {
            mismatches.push(format!(
                "{env} ({file}): line-scan-probe did NOT fire — source_lines is supposed to be universal \
                 (every file gets per-file DSL evaluation regardless of dispatch/degraded status, see \
                 pipeline::compute_fresh_artifact's own doc)"
            ));
        }
        let caps = capabilities_for(env);
        let fired = loop_scan_hits.contains(file);
        if fired != caps.loop_spans {
            mismatches.push(format!(
                "{env} ({file}): declared loop_spans={}, trigger_in_loop probe actually fired={fired} \
                 (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc)",
                caps.loop_spans
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "capability_matrix: ENVIRONMENTS table's loop_spans column (or the source_lines universality \
         sanity check) disagrees with the real engine projection: {mismatches:#?}"
    );
}

/// Canary #6 (MINIMAL EXISTENCE): `decl_in_span` per environment — `method_spans`' semantics twin, and
/// the only column here that can be `no` while its own prerequisite is `yes`. It is proven through a
/// rule rather than by inspecting output because the question is about a COORDINATE, not a channel:
/// "does `body_start` precede the declaration" is only answerable against a fixture whose declaration
/// is at a known place, and running the real `MethodScan` over one is the honest way to ask.
///
/// A row can only be `no` here in two ways, and both are worth failing on. Either the environment has
/// no spans at all (`method_spans: no` — prisma/sql/lexical-fallback, whose marker sits in a comment
/// precisely so the negative is measured rather than assumed), or it has spans that begin after the
/// declaration — which is the state ALL SIX structural rows were in before 2026-08-10 and which no
/// other test in this file could see.
#[test]
fn canary_decl_in_span_column_matches_the_declared_table_via_a_declaration_anchored_probe_rule() {
    let dir = TempDir::new("zzop-capability-matrix-declspan");
    write_canary_files(&dir);
    let out = canary_engine_output(&dir, vec![canary_probe_pack()]);

    let decl_scan_hits: std::collections::BTreeSet<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "capability-matrix-canary/decl-scan-probe")
        .map(|f| f.file.as_str())
        .collect();

    let mut mismatches = Vec::new();
    for (env, file, _) in canary_files() {
        let caps = capabilities_for(env);
        let fired = decl_scan_hits.contains(file);
        if fired != caps.decl_in_span {
            mismatches.push(format!(
                "{env} ({file}): declared decl_in_span={}, declaration-anchored probe actually \
                 fired={fired} (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc). A \
                 declared-present row that MISSED means this parser's `body_start` no longer begins \
                 at the declaration line, which silently un-writes every declaration-anchored rule \
                 concept for that language — see `zzop_core::SourceSymbol`'s Body span contract.",
                caps.decl_in_span
            ));
        }
        // The column is only meaningful ON TOP of `method_spans`; a row claiming the semantics
        // without the substrate would be incoherent rather than merely wrong.
        if caps.decl_in_span && !caps.method_spans {
            mismatches.push(format!(
                "{env} ({file}): declared decl_in_span=true with method_spans=false — the declaration \
                 line cannot be inside a span the environment does not project"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "capability_matrix: ENVIRONMENTS table's decl_in_span column disagrees with the real engine \
         projection: {mismatches:#?}"
    );
}

/// Canary #5 (MINIMAL EXISTENCE): `string_literals` per environment — `call-scan-probe`'s twin, one
/// channel over: a `LiteralScan` with no `name_pattern` and no `entropy_min` asks only "does this
/// environment project bound string literals AT ALL". Landed WITH the column and the producers (the
/// A17 wave), so unlike `call_sites` the column never had a declared-but-unmeasured phase.
#[test]
fn canary_string_literals_channel_matches_the_declared_table_via_a_literal_scan_probe_rule() {
    let dir = TempDir::new("zzop-capability-matrix-stringliterals");
    write_canary_files(&dir);
    let out = canary_engine_output(&dir, vec![canary_probe_pack()]);

    let literal_scan_hits: std::collections::BTreeSet<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "capability-matrix-canary/literal-scan-probe")
        .map(|f| f.file.as_str())
        .collect();

    let mut mismatches = Vec::new();
    for (env, file, _) in canary_files() {
        let caps = capabilities_for(env);
        let fired = literal_scan_hits.contains(file);
        if fired != caps.string_literals {
            mismatches.push(format!(
                "{env} ({file}): declared string_literals={}, literal-scan probe actually \
                 fired={fired} (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc). A \
                 declared-absent row that fired means a producer arm landed without flipping its \
                 cell; a declared-present row that missed means the projection lost its wiring.",
                caps.string_literals
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "capability_matrix: ENVIRONMENTS table's string_literals column disagrees with the real \
         engine projection: {mismatches:#?}"
    );
}

/// Canary #4 (MINIMAL EXISTENCE): `call_sites` per environment. Like `loop_spans`, the channel is never
/// serialized into `AnalyzeOutput` (only `Matcher::CallScan` consumes it), so it is proven through a real
/// rule loaded via `load_dsl_packs` rather than by inspecting output — but unlike `loop_spans`' probe, this
/// one gates on NOTHING beyond the file pattern, because the column asks whether the channel exists, not
/// whether a family within it does. Which families a shipped rule actually reads is a different contract
/// with a different owner (`zzop_core::RULE_READ_CALL_KINDS`, bound by `call_kind_readers.rs`).
///
/// This is the probe whose absence this file's module doc carried as a named debt until 2026-08-03: with
/// the column all-`no` it could only under-claim, and the first `yes` turned it into a positive claim
/// nothing had measured.
#[test]
fn canary_call_sites_channel_matches_the_declared_table_via_a_call_scan_probe_rule() {
    let dir = TempDir::new("zzop-capability-matrix-callsites");
    write_canary_files(&dir);
    let out = canary_engine_output(&dir, vec![canary_probe_pack()]);

    let call_scan_hits: std::collections::BTreeSet<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "capability-matrix-canary/call-scan-probe")
        .map(|f| f.file.as_str())
        .collect();

    let mut mismatches = Vec::new();
    for (env, file, _) in canary_files() {
        let caps = capabilities_for(env);
        let fired = call_scan_hits.contains(file);
        if fired != caps.call_sites {
            mismatches.push(format!(
                "{env} ({file}): declared call_sites={}, call-scan probe actually fired={fired} \
                 (MINIMAL-EXISTENCE mismatch, not a firing claim — see module doc). A declared-absent \
                 row that FIRED means a producer arm landed in `pipeline/fresh/call_sites.rs` without \
                 flipping this table; a declared-present row that did NOT means the arm went away, or \
                 the fixture's console write / environment read stopped being one this producer \
                 recognizes.",
                caps.call_sites
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "capability_matrix: ENVIRONMENTS table's call_sites column disagrees with the real engine \
         projection: {mismatches:#?}"
    );
}

// -------------------------------------------------------------------------------------------------------
// Rule-side sweep — every shipped rule's matcher vs. the declaration table above.
// -------------------------------------------------------------------------------------------------------

/// `(representative filename, environment it dispatches to)` — a fixed, textual-only list (no files
/// written to disk for this section; only `Regex::is_match` against these literal names). Covers every
/// TypeScript-family extension `dispatch_by_extension` recognizes, one representative per structural
/// parser, `schema.prisma` (the conventional Prisma schema filename), and every extension actually
/// referenced by a shipped rule's `file_pattern` that `dispatch_by_extension` does NOT recognize (the
/// lexical-fallback path: `.vue`/`.jsp`/`.jspx`/`.tag` are all real shipped-pattern extensions; `.kt` is a
/// generic "definitely never dispatched" sentinel).
const REPRESENTATIVE_FILES: &[(&str, &str)] = &[
    ("a.ts", "typescript"),
    ("a.tsx", "typescript"),
    ("a.js", "typescript"),
    ("a.jsx", "typescript"),
    ("a.mjs", "typescript"),
    ("a.cjs", "typescript"),
    ("a.mts", "typescript"),
    ("a.cts", "typescript"),
    ("a.py", "python-3"),
    ("a.pyi", "python-3"),
    ("A.java", "java-21"),
    ("a.rs", "rust"),
    ("a.go", "go"),
    ("schema.prisma", "prisma"),
    ("a.sql", "sql"),
    ("a.cs", "csharp"),
    ("a.vue", "lexical-fallback"),
    ("a.jsp", "lexical-fallback"),
    ("a.jspx", "lexical-fallback"),
    ("a.tag", "lexical-fallback"),
    ("a.kt", "lexical-fallback"),
    // Path-prefixed twins: a rule whose `file_pattern` anchors on a path prefix (e.g.
    // `^api/.+\.ts$`) matches NONE of the bare names above, so without these the sweep would
    // silently SKIP such a rule instead of checking it — a false negative in the unsafe direction
    // for a negative-claim contract (review finding, 2026-07-23). Two common prefixes per weak- or
    // mixed-channel environment keep the probe honest without enumerating every layout.
    ("api/a.ts", "typescript"),
    ("domains/x/routes/a.ts", "typescript"),
    ("api/a.py", "python-3"),
    ("domains/x/routes/a.py", "python-3"),
    ("api/a.go", "go"),
    ("api/A.java", "java-21"),
    ("api/a.rs", "rust"),
    ("api/a.cs", "csharp"),
    ("api/a.vue", "lexical-fallback"),
    ("api/a.kt", "lexical-fallback"),
    ("api/a.sql", "sql"),
];

/// Rules earning a documented exemption from the sweep below — each entry names WHY inline, so the
/// allowlist edit itself is the machine-readable disclosure a new rule cannot silently bypass (a rule
/// admitting a channel-lacking environment with NO allowlist entry fails the sweep, forcing either a
/// pattern fix or a reviewed, commented addition here).
const ALLOWLIST: &[(&str, &str)] = &[
    // `browser/markdown-and-html-sink-unsanitized`'s MethodScan `file_pattern` admits `.vue`, but this engine has
    // no symbol/span parser for `.vue` (dispatch_by_extension has no "vue" arm -> lexical fallback ->
    // method_spans absent). Case (iii) from this contract's adjudication guide: a DELIBERATE broad
    // pattern, already self-disclosed in the rule's OWN shipped message ("It also cannot see across
    // `.vue` single-file components today ... despite `.vue` being in its file pattern for
    // forward-compatibility; only same-file `.ts`/`.tsx`/`.js`/`.jsx` co-occurrence is caught." —
    // rules/dsl/browser/browser.json). The TS/JS lane still works; `.vue` silently never fires — exactly
    // the silent-partial-coverage class this test exists to surface, not hide. Surfaced here (not fixed
    // here — rule-pattern changes are this test's SUBJECT, not this test's job).
    ("browser", "markdown-and-html-sink-unsanitized"),
];

/// The channel(s) `rule.matcher` requires, or `None` for `Matcher::LineScan` (needs only the universal
/// `source_lines` channel, so it can never be an offender regardless of `file_pattern`).
fn required_channels(matcher: &Matcher) -> Option<(&str, Option<&str>, Vec<&'static str>)> {
    match matcher {
        // A plain LineScan needs only the universal `source_lines` channel — but `line_call_kind`
        // (W3) gates every matched line on a projected call site, so a gated rule is FOREVER-SILENT
        // on an environment without the channel, exactly like a CallScan.
        Matcher::LineScan(m) => match &m.line_call_kind {
            None => None,
            Some(_) => Some((
                m.file_pattern.as_str(),
                m.file_exclude_pattern.as_deref(),
                vec!["call_sites"],
            )),
        },
        Matcher::MethodScan(m) => {
            let mut required = vec!["method_spans"];
            if m.trigger_in_loop {
                required.push("loop_spans");
            }
            // `require_call_kind` (W3) gates the span on a projected call site — silent without the
            // channel, so it is a required channel exactly like `trigger_in_loop`'s `loop_spans`.
            if m.require_call_kind.is_some() {
                required.push("call_sites");
            }
            Some((
                m.file_pattern.as_str(),
                m.file_exclude_pattern.as_deref(),
                required,
            ))
        }
        Matcher::SymbolScan(m) => Some((m.file_pattern.as_str(), None, vec!["symbols"])),
        Matcher::IoScan(m) => {
            let required = match m.direction {
                IoDirection::Provides => vec!["io_provides"],
                IoDirection::Consumes => vec!["io_consumes"],
                // `Any` needs EITHER side, encoded as its own key below.
                IoDirection::Any => vec!["io_provides_or_io_consumes"],
            };
            Some((
                m.file_pattern.as_str(),
                m.file_exclude_pattern.as_deref(),
                required,
            ))
        }
        Matcher::CallScan(m) => {
            // `in_loop` adds `loop_spans` the same way `MethodScan::trigger_in_loop` does — the gate reads
            // the identical field and is silent without it.
            let mut required = vec!["call_sites"];
            if m.in_loop {
                required.push("loop_spans");
            }
            Some((
                m.file_pattern.as_str(),
                m.file_exclude_pattern.as_deref(),
                required,
            ))
        }
        Matcher::LiteralScan(m) => Some((
            m.file_pattern.as_str(),
            m.file_exclude_pattern.as_deref(),
            vec!["string_literals"],
        )),
    }
}

fn channel_satisfied(caps: Capabilities, channel: &str) -> bool {
    match channel {
        "symbols" => caps.symbols,
        "method_spans" => caps.method_spans,
        "loop_spans" => caps.loop_spans,
        "io_provides" => caps.io_provides,
        "io_consumes" => caps.io_consumes,
        "call_sites" => caps.call_sites,
        "string_literals" => caps.string_literals,
        "io_provides_or_io_consumes" => caps.io_provides || caps.io_consumes,
        other => panic!("capability_matrix: unknown required-channel key {other:?}"),
    }
}

/// The rule-side sweep (the STRONG, machine-certain claim this whole contract exists for — see module
/// doc): every loaded DSL rule's `file_pattern` (conservatively tested against `REPRESENTATIVE_FILES`, the
/// same representative-filename discipline this contract's design calls for instead of per-rule fixtures)
/// must not admit an environment whose declared capabilities lack a channel that rule's matcher requires,
/// UNLESS the rule is named in `ALLOWLIST` with an inline reason.
#[test]
fn every_shipped_rule_matcher_only_admits_environments_whose_required_channel_this_engine_projects()
{
    let packs = crate::load_all_packs();
    let mut offenders = Vec::new();

    for pack in &packs {
        for rule in &pack.rules {
            if ALLOWLIST.contains(&(pack.id.as_str(), rule.id.as_str())) {
                continue;
            }
            let Some((file_pattern, file_exclude_pattern, required)) =
                required_channels(&rule.matcher)
            else {
                continue; // LineScan — universal channel only, never an offender.
            };
            let Ok(file_re) = regex::Regex::new(file_pattern) else {
                continue; // a malformed pattern is a different contract's problem, not this one's.
            };
            let file_exclude_re = file_exclude_pattern.and_then(|p| regex::Regex::new(p).ok());

            for (filename, env) in REPRESENTATIVE_FILES {
                if !file_re.is_match(filename) {
                    continue;
                }
                if file_exclude_re
                    .as_ref()
                    .is_some_and(|re| re.is_match(filename))
                {
                    continue;
                }
                let caps = capabilities_for(env);
                for channel in &required {
                    if !channel_satisfied(caps, channel) {
                        offenders.push(format!(
                            "{}/{}: file_pattern {file_pattern:?} admits {filename} ({env}), whose \
                             declared capability table lacks `{channel}` -> this rule is FOREVER-SILENT \
                             on {env} files (MINIMAL-EXISTENCE claim: this proves the rule CANNOT fire \
                             there, it says nothing about whether it fires elsewhere — see module doc). \
                             If this is deliberate (case iii: a broad pattern where the TS/primary lane \
                             still works), add a commented ALLOWLIST entry here instead of a silent skip.",
                            pack.id, rule.id
                        ));
                    }
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "capability_matrix rule-side sweep found forever-silent matcher/environment combinations: \
         {offenders:#?}"
    );
}

// -------------------------------------------------------------------------------------------------------
// io FIELD-level capability table — `response` / `body` / `retry_configured`, the three OPTIONAL fields
// that ride an `IoProvide`/`IoConsume` entry. Until 2026-08-03 their language coverage lived only as
// prose (rule messages, module docs) — the exact absent-by-silence mechanism that let `loop_spans` sit
// TS+Go-only unnoticed. Same convention as the channel table above: a declaration table transcribed
// from the producers' own code, and a canary that measures every row bidirectionally against the real
// `analyze_tree` path. These are NOT matcher substrates (no DSL matcher reads them — their consumers
// are the native cross-layer rules), so they deliberately do not join `Capabilities`/
// `required_channels`; a separate table also keeps this section additive beside the channel columns.
//
// | environment      | io_provide_response | io_provide_body | io_consume_retry_configured |
// |------------------|---------------------|-----------------|-----------------------------|
// | typescript       | yes                 | yes             | yes                         |
// | every other row  | no                  | no              | no                          |
//
// Ground truth per column (grep-censused 2026-08-03, not guessed):
// - `io_provide_response` — ONE producer: parser-typescript's Nest controller-decorator return-type
//   capture (`adapters/controller_decorators/method_facts.rs`, `response-shape-v1`, the ⓖ wave).
// - `io_provide_body` — ONE producer: the same Nest capture's `@Body()` DTO arm (`method_body_shape`
//   in the same file; `analyze/compose/controller_prefix.rs` only carries it through).
// - `io_consume_retry_configured` — ONE producer: parser-typescript's egress collector
//   (`adapters/egress/collector.rs`, `egress-retry-v1`).
//
// **A `yes` cell is CHANNEL-EXISTENCE, not a framework gate** (the second dimension the extension-keyed
// sightline cannot express): inside TypeScript extensions, only the NEST controller-decorator shape
// emits `response`/`body` — an Express/Hono/Next file-convention route in the same `.ts` tree emits
// neither — and only the axios/fetch EGRESS COLLECTOR sets `retry_configured` — the hono-client, tRPC
// and fetch-wrapper consume paths leave it unset. So `yes` means "this environment CAN carry the
// field", never "every route/call in this environment does". The `no` rows are proven against fixtures
// that genuinely CONTAIN the construct where the language can express it (a FastAPI `-> Out` return
// annotation + Pydantic body param + tenacity-retried write; a Spring `@RequestBody` + DTO return +
// `@Retryable` write; an axum `Json<Out>` handler; a C# typed action with `[FromBody]`) — the
// `ZZOP_LOOP_MARKER` discipline. Go handlers write to a `ResponseWriter`/context rather than declaring
// a response type in any signature position, and Prisma/SQL provides are `db-table` declarations with
// no HTTP contract to annotate — for those rows the absent construct is a statement about the language,
// like their loop rows.
// -------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct IoFieldCapabilities {
    io_provide_response: bool,
    io_provide_body: bool,
    io_consume_retry_configured: bool,
}

/// The io-field declaration table — see the section comment above for the per-column ground-truth
/// citations. Rows deliberately mirror `ENVIRONMENTS` one-to-one (pinned below), so a 10th environment
/// cannot land with its io-field coverage undeclared.
const IO_FIELD_ENVIRONMENTS: &[(&str, IoFieldCapabilities)] = &[
    (
        "typescript",
        IoFieldCapabilities {
            io_provide_response: true,
            io_provide_body: true,
            io_consume_retry_configured: true,
        },
    ),
    (
        "python-3",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
    (
        "java-21",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
    (
        "rust",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
    (
        "go",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
    (
        "prisma",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
    (
        "sql",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
    (
        "csharp",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
    (
        "lexical-fallback",
        IoFieldCapabilities {
            io_provide_response: false,
            io_provide_body: false,
            io_consume_retry_configured: false,
        },
    ),
];

fn io_field_capabilities_for(env: &str) -> IoFieldCapabilities {
    IO_FIELD_ENVIRONMENTS
        .iter()
        .find(|(e, _)| *e == env)
        .unwrap_or_else(|| {
            panic!(
                "capability_matrix: no IO_FIELD_ENVIRONMENTS row for {env:?} — add one (with a \
                 producer-cited justification) before referencing it"
            )
        })
        .1
}

/// `(environment key, fixture files)` — this section's OWN fixtures, separate from `canary_files()`
/// on purpose (those feed set-equality pins and channel probes; splicing extra constructs into them
/// would entangle two contracts). Every fixture that CAN express a construct contains it genuinely —
/// see the section comment for which rows are construct-absence statements instead.
fn io_field_canary_files() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            "typescript",
            vec![
                (
                    "zzopIoFields.controller.ts",
                    r#"declare function Controller(prefix: string): ClassDecorator;
declare function Get(path?: string): MethodDecorator;
declare function Post(path?: string): MethodDecorator;
declare function Body(): ParameterDecorator;

class ZzopCanaryOutDto {
  id: string;
}

class ZzopCanaryCreateDto {
  name: string;
}

@Controller('zzop-canary-io-fields')
export class ZzopCanaryIoFieldsController {
  @Get('out')
  read(): Promise<ZzopCanaryOutDto> {
    return Promise.resolve(new ZzopCanaryOutDto());
  }

  @Post('in')
  create(@Body() dto: ZzopCanaryCreateDto): Promise<ZzopCanaryOutDto> {
    return Promise.resolve(new ZzopCanaryOutDto());
  }
}
"#,
                ),
                (
                    "zzopIoFieldsRetry.ts",
                    r#"import axios from 'axios';
import axiosRetry from 'axios-retry';

axiosRetry(axios, { retries: 3 });

export function zzopRetriedWrite() {
  return axios.post('/zzop-canary-retried-write', { name: 'x' });
}
"#,
                ),
            ],
        ),
        (
            "python-3",
            vec![(
                "zzop_io_fields.py",
                r#"from fastapi import FastAPI
from pydantic import BaseModel
import requests
from tenacity import retry

app = FastAPI()


class ZzopCanaryOut(BaseModel):
    id: str


class ZzopCanaryIn(BaseModel):
    name: str


@app.get("/zzop-canary-io-fields")
def zzop_read() -> ZzopCanaryOut:
    return ZzopCanaryOut(id="1")


@app.post("/zzop-canary-io-fields")
def zzop_create(payload: ZzopCanaryIn) -> ZzopCanaryOut:
    return ZzopCanaryOut(id="1")


@retry
def zzop_retried_write():
    requests.post("https://api.example.com/zzop-canary", json={"name": "x"})
"#,
            )],
        ),
        (
            "java-21",
            vec![(
                "ZzopIoFields.java",
                r#"import org.springframework.retry.annotation.Retryable;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.client.RestTemplate;

@RestController
class ZzopCanaryIoFieldsController {
  @GetMapping("/zzop-canary-io-fields")
  ZzopCanaryOutDto read() { return new ZzopCanaryOutDto(); }

  @PostMapping("/zzop-canary-io-fields")
  ZzopCanaryOutDto create(@RequestBody ZzopCanaryInDto dto) { return new ZzopCanaryOutDto(); }

  @Retryable
  void zzopRetriedWrite() {
    new RestTemplate().postForObject("https://api.example.com/zzop-canary", null, String.class);
  }
}

class ZzopCanaryOutDto { public String id; }

class ZzopCanaryInDto { public String name; }
"#,
            )],
        ),
        (
            "rust",
            vec![(
                "zzop_io_fields.rs",
                r#"use axum::routing::{get, post};
use axum::{Json, Router};

struct ZzopCanaryOut { id: String }
struct ZzopCanaryIn { name: String }

fn main() {
    let app = Router::new()
        .route("/zzop-canary-io-fields", get(zzop_read))
        .route("/zzop-canary-io-fields", post(zzop_create));
}

async fn zzop_read() -> Json<ZzopCanaryOut> {
    Json(ZzopCanaryOut { id: "1".to_string() })
}

async fn zzop_create(Json(payload): Json<ZzopCanaryIn>) -> Json<ZzopCanaryOut> {
    Json(ZzopCanaryOut { id: payload.name })
}
"#,
            )],
        ),
        (
            "go",
            vec![(
                "zzop_io_fields.go",
                r#"package main

import (
	"net/http"

	"github.com/gin-gonic/gin"
)

// Go handlers write to the context/ResponseWriter — there is no signature position that declares a
// response DTO, so the `no` cells on this row are construct-absence statements (section comment).
func main() {
	r := gin.Default()
	r.GET("/zzop-canary-io-fields", zzopIoFieldsHandler)
}

func zzopIoFieldsHandler(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"id": "1"})
}
"#,
            )],
        ),
        (
            "prisma",
            vec![(
                "zzop_io_fields.prisma",
                r#"// A `db-table` provide has no HTTP contract to annotate — construct-absence row.
model ZzopCanaryIoFields {
  id String @id
}
"#,
            )],
        ),
        (
            "sql",
            vec![(
                "zzop_io_fields.sql",
                r#"-- A `db-table` provide has no HTTP contract to annotate — construct-absence row.
CREATE TABLE zzop_canary_io_fields (id INT);
"#,
            )],
        ),
        (
            "csharp",
            vec![(
                "ZzopIoFields.cs",
                r#"public class ZzopCanaryIoFieldsController {
    [HttpGet]
    public ZzopCanaryOutDto Read() { return new ZzopCanaryOutDto(); }

    [HttpPost]
    public ZzopCanaryOutDto Create([FromBody] ZzopCanaryInDto dto) { return new ZzopCanaryOutDto(); }
}

public class ZzopCanaryOutDto { public string Id; }

public class ZzopCanaryInDto { public string Name; }
"#,
            )],
        ),
        (
            "lexical-fallback",
            vec![(
                "zzopIoFields.kt",
                r#"// .kt is never dispatched (lexical fallback) — a genuinely TYPED Kotlin handler shape, so this
// row's `no` cells are proven against a file containing the construct, not one omitting it.
data class ZzopCanaryOut(val id: String)

fun zzopRead(): ZzopCanaryOut = ZzopCanaryOut("1")
"#,
            )],
        ),
    ]
}

/// The io-field table's row set must mirror `ENVIRONMENTS` exactly, and every row must have its own
/// fixture — the same "a row nobody measures is worse than absent" lesson
/// `every_declared_environment_has_a_canary_that_measures_it` documents.
#[test]
fn io_field_environments_mirror_the_channel_table_and_each_row_has_a_fixture() {
    let channel_rows: BTreeSet<&str> = ENVIRONMENTS.iter().map(|(e, _)| *e).collect();
    let field_rows: BTreeSet<&str> = IO_FIELD_ENVIRONMENTS.iter().map(|(e, _)| *e).collect();
    assert_eq!(
        channel_rows, field_rows,
        "capability_matrix: IO_FIELD_ENVIRONMENTS must have exactly one row per ENVIRONMENTS row — \
         a new environment cannot land with its io-field coverage undeclared"
    );
    let fixture_rows: BTreeSet<&str> = io_field_canary_files()
        .into_iter()
        .map(|(e, _)| e)
        .collect();
    assert_eq!(
        field_rows, fixture_rows,
        "capability_matrix: every IO_FIELD_ENVIRONMENTS row needs an io_field_canary_files() entry \
         (and vice versa) — an unmeasured declaration row is a claim nobody has compared to the engine"
    );
}

/// Canary #6 (MINIMAL EXISTENCE, bidirectional): the three io FIELDS per environment, read off the
/// ASSEMBLED whole-tree io exactly like canary #2 — `response`/`body` after assemble-time DTO
/// resolution (the state the native rules actually read), `retry_configured` as the collector left it.
/// A declared-`no` row whose fixture measures `Some` means a producer landed without flipping its
/// cell; a declared-`yes` row measuring `None` means the capture (or its assemble resolution) lost
/// its wiring.
#[test]
fn canary_io_fields_match_the_declared_table() {
    let dir = TempDir::new("zzop-capability-matrix-iofields");
    for (_, files) in io_field_canary_files() {
        for (name, content) in files {
            dir.write(name, content);
        }
    }
    let out = canary_engine_output(&dir, Vec::new());
    let io = out.ir.ir.io.as_ref();

    let mut mismatches = Vec::new();
    for (env, files) in io_field_canary_files() {
        let caps = io_field_capabilities_for(env);
        let in_env = |file: &str| files.iter().any(|(name, _)| *name == file);
        let has_response = io.is_some_and(|io| {
            io.provides
                .iter()
                .any(|p| in_env(&p.file) && p.response.is_some())
        });
        let has_body = io.is_some_and(|io| {
            io.provides
                .iter()
                .any(|p| in_env(&p.file) && p.body.is_some())
        });
        let has_retry = io.is_some_and(|io| {
            io.consumes
                .iter()
                .any(|c| in_env(&c.file) && c.retry_configured == Some(true))
        });
        for (column, declared, measured) in [
            (
                "io_provide_response",
                caps.io_provide_response,
                has_response,
            ),
            ("io_provide_body", caps.io_provide_body, has_body),
            (
                "io_consume_retry_configured",
                caps.io_consume_retry_configured,
                has_retry,
            ),
        ] {
            if declared != measured {
                mismatches.push(format!(
                    "{env}: declared {column}={declared}, engine actually projected {measured} \
                     (MINIMAL-EXISTENCE mismatch — and remember the section comment: a `yes` is \
                     channel-existence, gated inside the environment by the ONE producing framework \
                     shape, never an all-frameworks claim)"
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "capability_matrix: IO_FIELD_ENVIRONMENTS disagrees with the real assembled engine \
         projection: {mismatches:#?}"
    );
}
