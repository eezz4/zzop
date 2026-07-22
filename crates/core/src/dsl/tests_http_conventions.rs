//! http-conventions fixture tests — a test-only matcher-shape demo (inlined rather than a shipped
//! `rules/dsl/*.json`) combining io-scan (whole-tree, via `eval_pack_io_scan`) and symbol-scan (per-file,
//! via `eval_pack`) rules.

use crate::io::{IoConsume, IoProvide};
use crate::ir::SourceSymbolKind;

use super::test_support::{scan_io_tree, symbol};
use super::{eval_pack, RuleContext, RulePackDef, SourceFile};

const HTTP_CONVENTIONS_JSON: &str = r#"{
    "id": "http-conventions",
    "framework": "any",
    "schema_version": 1,
    "rules": [
        {
            "id": "endpoint-version-prefix",
            "severity": "warning",
            "message": "HTTP endpoint is not exposed under a versioned /api/v<N>/ prefix — add a version prefix so a future breaking change doesn't silently break existing callers.",
            "matcher": {
                "type": "io-scan",
                "file_pattern": "(?i)routes?/",
                "direction": "provides",
                "kind": "http",
                "key_pattern": "^(?:GET|POST|PUT|PATCH|DELETE) /api/v[0-9]+/",
                "negate": true
            }
        },
        {
            "id": "unversioned-fetch",
            "severity": "info",
            "message": "Fetches an HTTP endpoint that is not under a versioned /api/v<N>/ prefix, or whose target could not be statically resolved — confirm this call is intentionally unversioned.",
            "matcher": {
                "type": "io-scan",
                "file_pattern": "(?i)\\.(ts|tsx)$",
                "direction": "consumes",
                "kind": "http",
                "key_pattern": "^(?:GET|POST|PUT|PATCH|DELETE) /api/v[0-9]+/",
                "negate": true
            }
        },
        {
            "id": "exported-handler-naming",
            "severity": "warning",
            "message": "Exported route handler does not follow the handle<Name> naming convention — rename it so handlers are easy to grep and distinguish from plain helpers.",
            "matcher": {
                "type": "symbol-scan",
                "file_pattern": "(?i)routes?/.*\\.tsx?$",
                "kind": "function",
                "exported": true,
                "name_pattern": "^handle[A-Z]",
                "negate": true
            }
        }
    ]
}"#;

fn http_conventions_pack() -> RulePackDef {
    serde_json::from_str(HTTP_CONVENTIONS_JSON).expect("parse inlined http-conventions fixture")
}

#[test]
fn http_conventions_flags_unversioned_provided_endpoint() {
    let provides = vec![IoProvide {
        kind: "http".into(),
        key: "GET /authen/getUserInfo".into(),
        file: "routes/authRoutes.ts".into(),
        line: 12,
        symbol: None,
        body: None,
    }];
    let f = scan_io_tree(&http_conventions_pack(), provides, vec![]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].rule_id, "http-conventions/endpoint-version-prefix");
    assert_eq!(f[0].line, 12);
}

#[test]
fn http_conventions_does_not_flag_versioned_endpoint() {
    let provides = vec![IoProvide {
        kind: "http".into(),
        key: "GET /api/v1/authen/getUserInfo".into(),
        file: "routes/authRoutes.ts".into(),
        line: 12,
        symbol: None,
        body: None,
    }];
    let f = scan_io_tree(&http_conventions_pack(), provides, vec![]);
    assert!(f
        .iter()
        .all(|x| x.rule_id != "http-conventions/endpoint-version-prefix"));
}

#[test]
fn http_conventions_flags_unversioned_fetch_and_unresolved_dynamic_fetch() {
    let consumes = vec![
        IoConsume {
            kind: "http".into(),
            key: Some("GET /authen/getUserInfo".into()),
            file: "src/api/client.ts".into(),
            line: 7,
            raw: None,
            method: None,
            body: None,
            client: None,
            retry_configured: None,
        },
        IoConsume {
            kind: "http".into(),
            key: None,
            file: "src/api/client.ts".into(),
            line: 15,
            raw: None,
            method: None,
            body: None,
            client: None,
            retry_configured: None,
        },
        IoConsume {
            kind: "http".into(),
            key: Some("GET /api/v2/orders".into()),
            file: "src/api/client.ts".into(),
            line: 22,
            raw: None,
            method: None,
            body: None,
            client: None,
            retry_configured: None,
        },
    ];
    let f = scan_io_tree(&http_conventions_pack(), vec![], consumes);
    let hits: Vec<_> = f
        .iter()
        .filter(|x| x.rule_id == "http-conventions/unversioned-fetch")
        .collect();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|x| x.line == 7));
    assert!(hits.iter().any(|x| x.line == 15));
}

#[test]
fn http_conventions_flags_exported_handler_with_bad_naming() {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: "routes/authRoutes.ts".into(),
        text: String::new(),
        symbols: vec![
            symbol("handleLogin", SourceSymbolKind::Function, 3, true),
            symbol("login", SourceSymbolKind::Function, 9, true),
        ],
        io: None,
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let f = eval_pack(&http_conventions_pack(), &ctx);
    let hits: Vec<_> = f
        .iter()
        .filter(|x| x.rule_id == "http-conventions/exported-handler-naming")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 9);
}
