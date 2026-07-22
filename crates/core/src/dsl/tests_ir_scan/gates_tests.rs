//! `Matcher::IoScan`'s tree-only gates: `attr_present`/`attr_absent` (`AttributeStore` lookup),
//! `anchor_exclude_pattern` and suppress-marker (both via `IoScanTreeContext::anchor_line`), and the
//! provides-then-consumes/rule-order determinism contract. See the `tests_ir_scan` module doc.

use crate::attributes::{Attribute, AttributeStore, EntityRef};
use crate::io::IoProvide;

use super::super::test_support::{io_consume, io_provide, io_scan_pack, rule_pack, scan_io_tree};
use super::super::{eval_pack_io_scan, IoScanTreeContext, RulePackDef};

// --- attr_absent / attr_present ---

#[test]
fn attr_absent_vetoes_the_entry_with_a_truthy_exact_iokey_attribute() {
    let provides = vec![
        io_provide("http", "POST /api/users", 1),
        io_provide("http", "POST /api/orders", 2),
    ];
    let attrs = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /api/users".into(),
        },
        key: "auth-guarded".into(),
        value: serde_json::json!(true),
    }]);
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","attr_absent":"auth-guarded"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|_, _| None,
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].line, 2,
        "only the un-attributed route should surface"
    );
}

#[test]
fn attr_absent_vetoes_entries_covered_by_a_pathscope_attribute() {
    let provides = vec![
        io_provide("http", "POST /admin/users", 1),
        io_provide("http", "POST /public/ping", 2),
    ];
    let attrs = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::PathScope {
            prefix: "/admin".into(),
        },
        key: "auth-guarded".into(),
        value: serde_json::json!(true),
    }]);
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","attr_absent":"auth-guarded"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|_, _| None,
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].line, 2,
        "the /admin route is covered by the PathScope, so it must not surface"
    );
}

#[test]
fn attr_present_requires_truthy_an_explicit_false_still_fails_the_gate() {
    let provides = vec![
        io_provide("http", "POST /api/a", 1),
        io_provide("http", "POST /api/b", 2),
    ];
    let attrs = AttributeStore::from_attrs(vec![
        Attribute {
            target: EntityRef::IoKey {
                kind: "http".into(),
                key: "POST /api/a".into(),
            },
            key: "rate-limited".into(),
            value: serde_json::json!(false),
        },
        Attribute {
            target: EntityRef::IoKey {
                kind: "http".into(),
                key: "POST /api/b".into(),
            },
            key: "rate-limited".into(),
            value: serde_json::json!(true),
        },
    ]);
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","attr_present":"rate-limited"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|_, _| None,
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].line, 2,
        "an explicit `false` value must not satisfy attr_present"
    );
}

// --- anchor_exclude_pattern ---

#[test]
fn anchor_exclude_pattern_excludes_when_the_anchor_lines_text_matches() {
    let provides = vec![io_provide("http", "POST /api/users", 3)];
    let attrs = AttributeStore::from_attrs(Vec::new());
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","anchor_exclude_pattern":"legacy-ok"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|file, line| {
            (file == "f.ts" && line == 3)
                .then(|| "router.post('/users', h); // legacy-ok".to_string())
        },
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert!(out.is_empty());
}

#[test]
fn anchor_exclude_pattern_does_not_exclude_when_the_callback_returns_none() {
    let provides = vec![io_provide("http", "POST /api/users", 3)];
    let attrs = AttributeStore::from_attrs(Vec::new());
    let pack = io_scan_pack(
        r#"{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides","anchor_exclude_pattern":"legacy-ok"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|_, _| None,
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert_eq!(
        out.len(),
        1,
        "no source text reachable -> the exclusion must not apply"
    );
}

// --- suppress_marker (marker-lookback = the entry's own line, or the line directly above it) ---

#[test]
fn suppress_marker_on_the_anchor_lines_own_text_suppresses_the_finding() {
    let provides = vec![io_provide("http", "GET /legacy", 5)];
    let attrs = AttributeStore::from_attrs(Vec::new());
    let pack = rule_pack(
        r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides"},"suppress_marker":"legacy-ok"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|file, line| {
            (file == "f.ts" && line == 5).then(|| "// legacy-ok".to_string())
        },
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert!(out.is_empty());
}

#[test]
fn suppress_marker_on_the_line_directly_above_the_anchor_suppresses_the_finding() {
    let provides = vec![io_provide("http", "GET /legacy", 5)];
    let attrs = AttributeStore::from_attrs(Vec::new());
    let pack = rule_pack(
        r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"io-scan","file_pattern":"\\.ts$","direction":"provides"},"suppress_marker":"legacy-ok"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|file, line| {
            (file == "f.ts" && line == 4).then(|| "// legacy-ok".to_string())
        },
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert!(
        out.is_empty(),
        "the marker one line above the anchor must still suppress (MARKER_LOOKBACK_LINES=1)"
    );
}

#[test]
fn suppress_marker_recognizes_the_python_hash_comment_leader() {
    // io-scan anchor lines span every provide-producing language — `# auth-gate-ok` on a FastAPI route
    // line must suppress exactly like `// auth-gate-ok` on an Express one (`compile_marker_line_comment`).
    let provides = vec![IoProvide {
        kind: "http".into(),
        key: "GET /legacy".into(),
        file: "app.py".into(),
        line: 5,
        symbol: None,
        body: None,
    }];
    let attrs = AttributeStore::from_attrs(Vec::new());
    let pack = rule_pack(
        r#"{"id":"r","severity":"info","message":"m","matcher":{"type":"io-scan","file_pattern":"\\.py$","direction":"provides"},"suppress_marker":"legacy-ok"}"#,
    );
    let mut out = Vec::new();
    let ctx = IoScanTreeContext {
        provides: &provides,
        consumes: &[],
        attrs: &attrs,
        anchor_line: &|file, line| {
            (file == "app.py" && line == 5).then(|| "# legacy-ok".to_string())
        },
    };
    eval_pack_io_scan(&pack, &ctx, &mut out);
    assert!(
        out.is_empty(),
        "a `#`-comment marker must suppress on Python anchor lines"
    );
}

// --- determinism ---

#[test]
fn eval_pack_io_scan_orders_findings_provides_then_consumes_then_by_rule_order() {
    let provides = vec![
        io_provide("http", "GET /a", 1),
        io_provide("http", "GET /b", 2),
    ];
    let consumes = vec![
        io_consume("http", Some("GET /c"), 3),
        io_consume("http", Some("GET /d"), 4),
    ];
    let pack: RulePackDef = serde_json::from_str(
        r#"{
            "id": "t",
            "framework": "any",
            "rules": [
                {"id": "r1", "severity": "info", "message": "m", "matcher": {"type":"io-scan","file_pattern":"\\.ts$","direction":"any"}},
                {"id": "r2", "severity": "info", "message": "m", "matcher": {"type":"io-scan","file_pattern":"\\.ts$","direction":"any"}}
            ]
        }"#,
    )
    .expect("parse two-rule io-scan pack");

    let f = scan_io_tree(&pack, provides, consumes);
    let order: Vec<(&str, u32)> = f.iter().map(|x| (x.rule_id.as_str(), x.line)).collect();
    assert_eq!(
        order,
        vec![
            ("t/r1", 1),
            ("t/r1", 2),
            ("t/r1", 3),
            ("t/r1", 4),
            ("t/r2", 1),
            ("t/r2", 2),
            ("t/r2", 3),
            ("t/r2", 4),
        ]
    );
}
