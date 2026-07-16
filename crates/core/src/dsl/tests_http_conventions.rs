//! http-conventions fixture tests — a test-only matcher-shape demo (inlined rather than a shipped
//! `rules/dsl/*.json`) combining io-scan and symbol-scan rules.

use crate::io::IoFacts;
use crate::ir::SourceSymbolKind;

use super::test_support::{io_consume, io_provide, symbol};
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
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: "routes/authRoutes.ts".into(),
        text: String::new(),
        symbols: vec![],
        io: Some(IoFacts {
            provides: vec![io_provide("http", "GET /authen/getUserInfo", 12)],
            consumes: vec![],
        }),
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let f = eval_pack(&http_conventions_pack(), &ctx);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].rule_id, "http-conventions/endpoint-version-prefix");
    assert_eq!(f[0].line, 12);
}

#[test]
fn http_conventions_does_not_flag_versioned_endpoint() {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: "routes/authRoutes.ts".into(),
        text: String::new(),
        symbols: vec![],
        io: Some(IoFacts {
            provides: vec![io_provide("http", "GET /api/v1/authen/getUserInfo", 12)],
            consumes: vec![],
        }),
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let f = eval_pack(&http_conventions_pack(), &ctx);
    assert!(f
        .iter()
        .all(|x| x.rule_id != "http-conventions/endpoint-version-prefix"));
}

#[test]
fn http_conventions_flags_unversioned_fetch_and_unresolved_dynamic_fetch() {
    let files = vec![SourceFile {
        loop_spans: Vec::new(),
        rel: "src/api/client.ts".into(),
        text: String::new(),
        symbols: vec![],
        io: Some(IoFacts {
            provides: vec![],
            consumes: vec![
                io_consume("http", Some("GET /authen/getUserInfo"), 7),
                io_consume("http", None, 15),
                io_consume("http", Some("GET /api/v2/orders"), 22),
            ],
        }),
    }];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let f = eval_pack(&http_conventions_pack(), &ctx);
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
