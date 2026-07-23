//! Contract 12: the parser × rule CAPABILITY MATRIX — machine-pinned reachability FACTS (which per-file
//! channel each parser environment actually projects) cross-checked against every shipped DSL rule's
//! `file_pattern`, so a rule can never silently ship admitting an environment whose required channel this
//! engine does not project.
//!
//! This contract exists because the fact it pins previously lived only as prose, and prose had ALREADY
//! drifted from the code: an audit found "loop spans are TS-only" stated somewhere while
//! `parser/parser-go/src/lang/loop_spans.rs` and `go/go-goroutine-in-loop`'s `trigger_in_loop` matcher had
//! moved reality out from under that sentence. This module replaces the sentence with a table read
//! straight from `crates/engine/src/pipeline/fresh.rs`'s own per-language match arms (ground truth) and a
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
//! arms for `symbols`/`io`/`loop_spans`, NOT prose)
//!
//! Five channels, chosen at the granularity `crates/core/src/normalized.rs`'s `FileProjection` actually
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
//! | environment        | symbols | method_spans | loop_spans | io_provides | io_consumes |
//! |---------------------|---------|---------------|------------|-------------|-------------|
//! | typescript          | yes     | yes           | yes        | yes         | yes         |
//! | python-3            | yes     | yes           | no         | yes         | yes         |
//! | java-21             | yes     | yes           | no         | yes         | no          |
//! | rust                | yes     | yes           | no         | yes         | yes         |
//! | go                  | yes     | yes           | yes        | yes         | yes         |
//! | prisma              | yes     | no            | no         | no          | no          |
//! | sql                 | no      | no            | no         | yes         | no          |
//! | csharp              | yes     | yes           | no         | yes         | yes         |
//! | lexical-fallback    | no      | no            | no         | no          | no          |
//!
//! `prisma`'s `io_provides: no` is worth a second look: `zzop_parser_prisma::build_common_ir` DOES compute
//! a `db-table` `IoProvide` per model (`analysis.rs`'s own doc: "a `(kind="db-table", ...)` io PROVIDE at
//! the model's declaration line"). But the ENGINE's sole call site
//! (`crates/engine/src/pipeline/parsers.rs::parse_prisma`) discards `ir.ir.io`, keeping only
//! `ir.ir.symbols`/`ir.ir.loc` — that computed provide never reaches `assemble`'s whole-tree list. This is
//! not a bug this contract flags (no shipped rule's `file_pattern` admits `.prisma` with an `IoScan`
//! matcher today, so nothing is silently broken by it) but it IS a real orphaned capability, surfaced in
//! the report this test's author filed rather than fixed here (out of this test's scope — see the human
//! report for the pointer). If a future change threads that discarded `IoFacts` through, `prisma`'s
//! `io_provides` canary goes non-empty and the bidirectional check above starts failing until this row is
//! updated — exactly the forward-looking regression guard the "gains can't hide behind green" mandate asks
//! for.
//!
//! `lexical-fallback` has no parser crate (it is `dispatch`'s `None` arm, not `Language::*`) — it is a
//! synthetic 9th row, excluded from the parser-crate SSOT pin below.

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
    loop_spans: bool,
    io_provides: bool,
    io_consumes: bool,
}

/// The declaration table transcribed from this module's own doc — see there for the ground-truth
/// citations per environment. Order matches the doc table (parser-crate rows first, `lexical-fallback`
/// last as the synthetic 9th row).
const ENVIRONMENTS: &[(&str, Capabilities)] = &[
    (
        "typescript",
        Capabilities {
            symbols: true,
            method_spans: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
        },
    ),
    (
        "python-3",
        Capabilities {
            symbols: true,
            method_spans: true,
            loop_spans: false,
            io_provides: true,
            io_consumes: true,
        },
    ),
    (
        "java-21",
        Capabilities {
            symbols: true,
            method_spans: true,
            loop_spans: false,
            io_provides: true,
            io_consumes: false,
        },
    ),
    (
        "rust",
        Capabilities {
            symbols: true,
            method_spans: true,
            loop_spans: false,
            io_provides: true,
            io_consumes: true,
        },
    ),
    (
        "go",
        Capabilities {
            symbols: true,
            method_spans: true,
            loop_spans: true,
            io_provides: true,
            io_consumes: true,
        },
    ),
    (
        "prisma",
        Capabilities {
            symbols: true,
            method_spans: false,
            loop_spans: false,
            io_provides: false,
            io_consumes: false,
        },
    ),
    (
        "sql",
        Capabilities {
            symbols: false,
            method_spans: false,
            loop_spans: false,
            io_provides: true,
            io_consumes: false,
        },
    ),
    (
        "csharp",
        Capabilities {
            symbols: true,
            method_spans: true,
            loop_spans: false,
            io_provides: true,
            io_consumes: true,
        },
    ),
    (
        "lexical-fallback",
        Capabilities {
            symbols: false,
            method_spans: false,
            loop_spans: false,
            io_provides: false,
            io_consumes: false,
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

function zzopEgress() {
  fetch("https://api.example.com/zzop-canary");
}

apiRoutes.get("/zzop-canary-ts", zzopCanaryHandler);

function zzopCanaryHandler() {}
"#,
        ),
        (
            "python-3",
            "canary.py",
            r#"# ZZOP_LINE_MARKER
from fastapi import FastAPI
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


def zzop_egress():
    requests.get("https://api.example.com/zzop-canary")
"#,
        ),
        (
            "java-21",
            "Canary.java",
            r#"// ZZOP_LINE_MARKER
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

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

fn zzop_egress() {
    reqwest::get("https://api.example.com/zzop-canary");
}
"#,
        ),
        (
            "go",
            "canary.go",
            r#"package main

// ZZOP_LINE_MARKER

import (
	"net/http"

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
func ZzopCanaryTarget() {
	for i := 0; i < 1; i++ {
		zzopLoopBody() // ZZOP_LOOP_MARKER
	}
	_ = "ZZOP_METHOD_MARKER"
}

func zzopLoopBody() {}

func zzopEgress() {
	http.Get("/zzop-canary")
}
"#,
        ),
        (
            "prisma",
            "schema.prisma",
            r#"// ZZOP_LINE_MARKER
// ZZOP_METHOD_MARKER
// ZZOP_LOOP_MARKER
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

    public async void ZzopEgress() {
        var client = new HttpClient();
        var r = client.GetAsync("https://api.example.com/zzop-canary");
    }
}
"#,
        ),
        (
            "lexical-fallback",
            "canary.kt",
            r#"// ZZOP_LINE_MARKER
// ZZOP_METHOD_MARKER
// ZZOP_LOOP_MARKER
// .kt is not dispatched by any parser this engine ships today (crates/engine/src/dispatch.rs's
// dispatch_by_extension has no "kt" arm) -- this file exercises the lexical-fallback path on purpose.
fun main() {}
"#,
        ),
    ]
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
    // `browser/unsanitized-markdown-html`'s MethodScan `file_pattern` admits `.vue`, but this engine has
    // no symbol/span parser for `.vue` (dispatch_by_extension has no "vue" arm -> lexical fallback ->
    // method_spans absent). Case (iii) from this contract's adjudication guide: a DELIBERATE broad
    // pattern, already self-disclosed in the rule's OWN shipped message ("It also cannot see across
    // `.vue` single-file components today ... despite `.vue` being in its file pattern for
    // forward-compatibility; only same-file `.ts`/`.tsx`/`.js`/`.jsx` co-occurrence is caught." —
    // rules/dsl/browser/browser.json). The TS/JS lane still works; `.vue` silently never fires — exactly
    // the silent-partial-coverage class this test exists to surface, not hide. Surfaced here (not fixed
    // here — rule-pattern changes are this test's SUBJECT, not this test's job).
    ("browser", "unsanitized-markdown-html"),
];

/// The channel(s) `rule.matcher` requires, or `None` for `Matcher::LineScan` (needs only the universal
/// `source_lines` channel, so it can never be an offender regardless of `file_pattern`).
fn required_channels(matcher: &Matcher) -> Option<(&str, Option<&str>, Vec<&'static str>)> {
    match matcher {
        Matcher::LineScan(_) => None,
        Matcher::MethodScan(m) => {
            let mut required = vec!["method_spans"];
            if m.trigger_in_loop {
                required.push("loop_spans");
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
    }
}

fn channel_satisfied(caps: Capabilities, channel: &str) -> bool {
    match channel {
        "symbols" => caps.symbols,
        "method_spans" => caps.method_spans,
        "loop_spans" => caps.loop_spans,
        "io_provides" => caps.io_provides,
        "io_consumes" => caps.io_consumes,
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
