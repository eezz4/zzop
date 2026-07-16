//! Shared fixtures/helpers for the DSL interpreter test modules (`tests_*`).

use crate::finding::Finding;
use crate::io::{IoConsume, IoFacts, IoProvide};
use crate::ir::{SourceSymbol, SourceSymbolKind};

use super::{eval_pack, RuleContext, RulePackDef, SourceFile};

/// The three Java security-concern rules (`sql-taint`/`weak-crypto`/`cmd-injection`) that moved into
/// `be-security` when the language-named `java-security` pack was dissolved (v0.15). We load the real
/// `be-security.json` and filter to just those three so the fixture stays a small, fully-`.java`-applicable
/// set.
pub(super) fn pack() -> RulePackDef {
    let mut p: RulePackDef = serde_json::from_str(include_str!(
        "../../../../rules/dsl/be-security/be-security.json"
    ))
    .expect("parse be-security.json");
    p.rules
        .retain(|r| matches!(r.id.as_str(), "sql-taint" | "weak-crypto" | "cmd-injection"));
    p
}

pub(super) fn scan(src: &str, rel: &str) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: rel.into(),
        text: src.into(),
        symbols: vec![],
        io: None,
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
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

pub(super) fn scan_methods(src: &str, symbols: Vec<SourceSymbol>) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: "C.java".into(),
        text: src.into(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
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
        rel: rel.into(),
        text: String::new(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    eval_pack(&symbol_scan_pack(matcher_json), &ctx)
}

pub(super) fn io_scan_pack(matcher_json: &str) -> RulePackDef {
    let src = format!(
        r#"{{"id":"t","framework":"any","rules":[{{"id":"r","severity":"info","message":"m","matcher":{matcher_json}}}]}}"#
    );
    serde_json::from_str(&src).expect("parse inline io-scan pack")
}

pub(super) fn scan_io(rel: &str, io: IoFacts, matcher_json: &str) -> Vec<Finding> {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: rel.into(),
        text: String::new(),
        symbols: vec![],
        io: Some(io),
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    eval_pack(&io_scan_pack(matcher_json), &ctx)
}

pub(super) fn io_provide(kind: &str, key: &str, line: u32) -> IoProvide {
    IoProvide {
        body: None,
        kind: kind.into(),
        key: key.into(),
        file: "f.ts".into(),
        line,
        symbol: None,
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
    }
}

pub(super) fn findings_as_json(f: &[Finding]) -> Vec<serde_json::Value> {
    f.iter().map(|x| serde_json::to_value(x).unwrap()).collect()
}

/// Builds a one-rule pack from a full inline rule JSON object — needed here since `suppress_marker`
/// lives on `RuleDef`, not inside `matcher`.
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
        rel: rel.into(),
        text: src.into(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
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
        rel: rel.into(),
        text: src.into(),
        symbols,
        io: None,
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    eval_pack(pack, &ctx)
}
