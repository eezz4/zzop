//! Shared fixtures/helpers for the DSL interpreter test modules (`tests_*`).

use crate::attributes::AttributeStore;
use crate::finding::Finding;
use crate::io::{IoConsume, IoProvide};
use crate::ir::{SourceSymbol, SourceSymbolKind};

use super::ir_scan::{eval_pack_io_scan, IoScanTreeContext};
use super::{eval_pack, RuleContext, RulePackDef, SourceFile};

/// The three Java security-concern LINE-SCAN rules (`sql-string-concat`/`weak-cipher`/`cmd-injection`)
/// that trace back to the dissolved language-named `java-security` pack (v0.15). `weak-crypto` was in
/// this set until 2026-08-09, when its hash half became a six-language call-scan rule — a matcher this
/// module's bare `SourceFile` (no projected call sites) cannot exercise — and its cipher arms split out
/// as `weak-cipher`, which is what this fixture retains now. We load the real `security.json` and
/// filter so the fixture stays a small, fully-`.java`-applicable set. Goes through
/// `crate::parse_dsl_pack` (not a raw `serde_json::from_str`) so this pack's `${NAME}` fragment refs
/// (its shared test-path `file_exclude_pattern`) resolve exactly like they do at real load time — a
/// raw struct deserialize would leave the literal `"${test-paths-stories}"`/`"${test-paths}"` strings
/// in place, which are not valid regexes and would silently no-op every affected rule.
pub(super) fn pack() -> RulePackDef {
    let mut p: RulePackDef =
        crate::parse_dsl_pack(include_str!("../../../../rules/dsl/security/security.json"))
            .expect("parse security.json");
    p.rules.retain(|r| {
        matches!(
            r.id.as_str(),
            "sql-string-concat" | "weak-cipher" | "cmd-injection"
        )
    });
    p
}

pub(super) fn scan(src: &str, rel: &str) -> Vec<Finding> {
    let files = vec![SourceFile {
        rel: rel.into(),
        text: src.into(),
        ..Default::default()
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(&pack(), &ctx)
}

/// Builds a `(name, body_start, body_end)` method span into a `SourceSymbol` — hand-built here since
/// Java isn't parsed in Rust yet; a real parser adapter would project these from its AST.
pub(super) fn method(name: &str, body_start: u32, body_end: u32) -> SourceSymbol {
    SourceSymbol {
        id: format!("C.java#{name}"),
        file: "C.java".into(),
        name: name.into(),
        kind: SourceSymbolKind::Function,
        line: body_start,
        exported: false,
        is_default: false,
        body_start: Some(body_start),
        body_end: Some(body_end),
        write_sites: Vec::new(),
    }
}

/// `scan_methods` with hand-supplied `SourceFile::call_sites` — needed by every fixture for a rule
/// carrying `MethodScan::require_call_kind`, whose exec/API WITNESS is a projected parser fact rather
/// than a lexical pattern. Hand-built for the same reason [`method`]'s spans are: this crate parses no
/// Java, so a test must stand in for what `zzop_parser_java_21::extract_call_sites` would project.
/// Passing an empty vec is therefore a real negative for such a rule, not a setup shortcut — the
/// channel's degrade direction is silence. `scan_methods` (the no-site spelling every other fixture
/// uses) is one line of `tests_method_scan`, kept there to stay inside this file's line cap.
pub(super) fn scan_methods_with_call_sites(
    src: &str,
    symbols: Vec<SourceSymbol>,
    call_sites: Vec<crate::CallSite>,
) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites,
        string_literals: Vec::new(),
        rel: "C.java".into(),
        text: src.into(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(&pack(), &ctx)
}

pub(super) fn snippet(f: &Finding) -> String {
    f.data.as_ref().unwrap()["snippet"]
        .as_str()
        .unwrap()
        .to_string()
}

pub(super) fn label(f: &Finding) -> String {
    f.data.as_ref().unwrap()["label"]
        .as_str()
        .unwrap()
        .to_string()
}

pub(super) fn symbol(
    name: &str,
    kind: SourceSymbolKind,
    line: u32,
    exported: bool,
) -> SourceSymbol {
    SourceSymbol {
        id: format!("f.ts#{name}"),
        file: "f.ts".into(),
        name: name.into(),
        kind,
        line,
        exported,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

pub(super) fn symbol_scan_pack(matcher_json: &str) -> RulePackDef {
    let src = format!(
        r#"{{"id":"t","framework":"any","rules":[{{"id":"r","severity":"info","message":"m","matcher":{matcher_json}}}]}}"#
    );
    serde_json::from_str(&src).expect("parse inline symbol-scan pack")
}

pub(super) fn scan_symbols(
    rel: &str,
    symbols: Vec<SourceSymbol>,
    matcher_json: &str,
) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites: Vec::new(),
        string_literals: Vec::new(),
        rel: rel.into(),
        text: String::new(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(&symbol_scan_pack(matcher_json), &ctx)
}

pub(super) fn io_scan_pack(matcher_json: &str) -> RulePackDef {
    let src = format!(
        r#"{{"id":"t","framework":"any","rules":[{{"id":"r","severity":"info","message":"m","matcher":{matcher_json}}}]}}"#
    );
    serde_json::from_str(&src).expect("parse inline io-scan pack")
}

/// Whole-tree io-scan evaluation (`eval_pack_io_scan`) against plain `provides`/`consumes` vecs, an empty
/// `AttributeStore` (every `attr_present`/`attr_absent` lookup misses), and a `None`-returning
/// `anchor_line` (no source text reachable — the envelope-mode shape). Covers the common case; a test
/// exercising `attr_present`/`attr_absent`/`anchor_exclude_pattern`/suppress-marker builds its own
/// `IoScanTreeContext` directly (see the `tests_ir_scan/` directory module).
pub(super) fn scan_io_tree(
    pack: &RulePackDef,
    provides: Vec<IoProvide>,
    consumes: Vec<IoConsume>,
) -> Vec<Finding> {
    let attrs = AttributeStore::from_attrs(Vec::new());
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &consumes,
        attrs: &attrs,
        anchor_line: &|_file: &str, _line: u32| None,
    };
    let mut out = Vec::new();
    eval_pack_io_scan(pack, &ctx, &mut out);
    out
}

pub(super) fn io_provide(kind: &str, key: &str, line: u32) -> IoProvide {
    IoProvide {
        response: None,
        body: None,
        kind: kind.into(),
        key: key.into(),
        file: "f.ts".into(),
        line,
        symbol: None,
    }
}

/// Like `io_provide`, but with a `symbol` set — for `IoScan::symbol_pattern` tests.
pub(super) fn io_provide_symbol(kind: &str, key: &str, line: u32, symbol: &str) -> IoProvide {
    IoProvide {
        symbol: Some(symbol.into()),
        ..io_provide(kind, key, line)
    }
}

pub(super) fn io_consume(kind: &str, key: Option<&str>, line: u32) -> IoConsume {
    IoConsume {
        client: None,
        body: None,
        kind: kind.into(),
        key: key.map(Into::into),
        file: "f.ts".into(),
        line,
        raw: None,
        method: None,
        retry_configured: None,
    }
}

pub(super) fn findings_as_json(f: &[Finding]) -> Vec<serde_json::Value> {
    f.iter().map(|x| serde_json::to_value(x).unwrap()).collect()
}

/// Builds a one-rule pack from a full inline rule JSON object — the rule's fields (`id`, `severity`,
/// `message`, `matcher`) live on `RuleDef`, so a test needs the full rule shape, not just a `matcher`.
pub(super) fn rule_pack(rule_json: &str) -> RulePackDef {
    let src = format!(r#"{{"id":"t","framework":"any","rules":[{rule_json}]}}"#);
    serde_json::from_str(&src).expect("parse inline rule pack")
}

pub(super) fn scan_pack(
    pack: &RulePackDef,
    rel: &str,
    src: &str,
    symbols: Vec<SourceSymbol>,
) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites: Vec::new(),
        string_literals: Vec::new(),
        rel: rel.into(),
        text: src.into(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(pack, &ctx)
}

/// Like `scan_pack`, but also lets a test hand-supply `SourceFile::loop_spans` — needed only for the
/// `trigger_in_loop` tests, every other `scan_pack` caller has no use for a non-empty vec.
pub(super) fn scan_pack_loops(
    pack: &RulePackDef,
    rel: &str,
    src: &str,
    symbols: Vec<SourceSymbol>,
    loop_spans: Vec<(u32, u32)>,
) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans,
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites: Vec::new(),
        string_literals: Vec::new(),
        rel: rel.into(),
        text: src.into(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(pack, &ctx)
}

/// `scan_pack`'s counterpart for `SourceFile::function_spans` — the substrate of
/// `MethodScan::after_in_same_function`. Kept separate from `scan_pack_loops` for the same reason that
/// one is separate from `scan_pack`: no other caller has a use for a non-empty vec, and threading both
/// through one signature would make every unrelated call site carry two placeholder arguments.
pub(super) fn scan_pack_fns(
    pack: &RulePackDef,
    rel: &str,
    src: &str,
    symbols: Vec<SourceSymbol>,
    function_spans: Vec<(u32, u32)>,
) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        function_spans,
        test_spans: Vec::new(),
        call_sites: Vec::new(),
        string_literals: Vec::new(),
        rel: rel.into(),
        text: src.into(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext { files: &files };
    eval_pack(pack, &ctx)
}
