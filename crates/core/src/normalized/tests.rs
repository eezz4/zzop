use super::*;
use crate::ir::{SourceSymbol, SourceSymbolKind};

fn symbol(name: &str, body_start: Option<u32>, body_end: Option<u32>) -> SourceSymbol {
    SourceSymbol {
        id: format!("f.ext#{name}"),
        file: "f.ext".into(),
        name: name.into(),
        kind: SourceSymbolKind::Function,
        line: 1,
        exported: true,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

fn valid_envelope_json() -> String {
    r#"{
            "format": "zzop-normalized-ast",
            "version": 1,
            "parser": "jsp-lexical/1",
            "source": "legacy",
            "files": [
                {
                    "path": "legacy/user.jsp",
                    "loc": 42,
                    "symbols": [],
                    "imports": {},
                    "re_exports": [],
                    "used_names": [],
                    "io": { "provides": [], "consumes": [] },
                    "degraded": false
                }
            ]
        }"#
    .to_string()
}

#[test]
fn valid_envelope_round_trips() {
    let envelope = validate_envelope(&valid_envelope_json()).expect("should validate");
    assert_eq!(envelope.format, NORMALIZED_AST_FORMAT);
    assert_eq!(envelope.version, 1);
    assert_eq!(envelope.files.len(), 1);
    assert_eq!(envelope.files[0].path, "legacy/user.jsp");
}

#[test]
fn minimal_envelope_with_defaulted_fields_round_trips() {
    // A minimal/degraded producer omits every optional field.
    let json = r#"{
            "format": "zzop-normalized-ast",
            "version": 1,
            "parser": "min/1",
            "source": "s",
            "files": [ { "path": "a.ext", "loc": 1 } ]
        }"#;
    let envelope = validate_envelope(json).expect("should validate");
    let file = &envelope.files[0];
    assert!(file.symbols.is_empty());
    assert!(file.imports.is_empty());
    assert!(file.re_exports.is_empty());
    assert!(file.used_names.is_empty());
    assert!(file.io.provides.is_empty());
    assert!(file.io.consumes.is_empty());
    assert!(!file.degraded);
    // A producer that only knows plain io facts may omit the fragment channels entirely — absent
    // still means empty, and this remains a fully valid, non-degraded projection.
    assert!(file.const_map_fragment.is_empty());
    assert!(file.procedure_router_fragments.is_empty());
    assert!(file.router_mount_fragments.is_empty());
    // `is_entry` defaults to `false` — a producer that knows nothing about framework entry
    // conventions makes no exemption claim, same "absent means the least-privileged value" rule as
    // every other optional field here.
    assert!(!file.is_entry);
}

#[test]
fn fragment_channels_round_trip_when_present() {
    use crate::{
        ProcedureRouterEntry, ProcedureRouterFragment, RouterMountEntry, RouterMountFragment,
    };

    let mut const_map_fragment = std::collections::HashMap::new();
    const_map_fragment.insert("USERS_TABLE".to_string(), "users".to_string());

    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "custom-router/1".to_string(),
        source: "s".to_string(),
        files: vec![FileProjection {
            class_shape_fragments: Vec::new(),
            path: "a.ext".to_string(),
            loc: 10,
            symbols: vec![],
            imports: ImportMap::new(),
            re_exports: vec![],
            dynamic_imports: vec![],
            used_names: vec![],
            const_map_fragment,
            procedure_router_fragments: vec![ProcedureRouterFragment {
                name: "viewerRouter".to_string(),
                entries: vec![ProcedureRouterEntry::Leaf {
                    key: "get".to_string(),
                    verb: "QUERY".to_string(),
                    line: 3,
                }],
            }],
            router_mount_fragments: vec![RouterMountFragment {
                name: "auth".to_string(),
                entries: vec![RouterMountEntry::Verb {
                    method: "POST".to_string(),
                    path: "/setup".to_string(),
                    handler: Some("handler".to_string()),
                    line: 7,
                    attr_keys: vec![],
                }],
            }],
            io: IoFacts::default(),
            degraded: false,
            is_entry: false,
            attributes: Vec::new(),
            loop_spans: vec![],
        }],
    };
    let json = serde_json::to_string(&envelope).unwrap();

    let round_tripped = validate_envelope(&json).expect("should validate");
    let file = &round_tripped.files[0];
    assert_eq!(
        file.const_map_fragment.get("USERS_TABLE"),
        Some(&"users".to_string())
    );
    assert_eq!(file.procedure_router_fragments.len(), 1);
    assert_eq!(file.procedure_router_fragments[0].name, "viewerRouter");
    assert_eq!(file.router_mount_fragments.len(), 1);
    assert_eq!(file.router_mount_fragments[0].name, "auth");

    // Re-serializing keeps the fragment channels present (not silently dropped on round-trip).
    assert!(json.contains("const_map_fragment"));
    assert!(json.contains("procedure_router_fragments"));
    assert!(json.contains("router_mount_fragments"));
}

#[test]
fn rejects_invalid_json() {
    let errors = validate_envelope("not json").unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("invalid JSON"));
}

#[test]
fn rejects_unknown_format() {
    let json = valid_envelope_json().replace("zzop-normalized-ast", "some-other-format");
    let errors = validate_envelope(&json).unwrap_err();
    assert!(errors.iter().any(|e| e.contains("unknown format")));
}

#[test]
fn rejects_version_greater_than_supported() {
    let json = valid_envelope_json().replace("\"version\": 1", "\"version\": 2");
    let errors = validate_envelope(&json).unwrap_err();
    assert!(errors.iter().any(|e| e.contains("unsupported version")));
}

#[test]
fn accepts_version_equal_to_supported() {
    assert!(validate_envelope(&valid_envelope_json()).is_ok());
}

#[test]
fn rejects_empty_path() {
    let json = valid_envelope_json().replace("legacy/user.jsp", "");
    let errors = validate_envelope(&json).unwrap_err();
    assert!(errors.iter().any(|e| e.contains("empty path")));
}

#[test]
fn rejects_duplicate_paths() {
    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "p/1".to_string(),
        source: "s".to_string(),
        files: vec![
            FileProjection {
                class_shape_fragments: Vec::new(),
                path: "a.ext".to_string(),
                loc: 1,
                symbols: vec![],
                imports: ImportMap::new(),
                re_exports: vec![],
                dynamic_imports: vec![],
                used_names: vec![],
                const_map_fragment: std::collections::HashMap::new(),
                procedure_router_fragments: vec![],
                router_mount_fragments: vec![],
                io: IoFacts::default(),
                degraded: false,
                is_entry: false,
                attributes: Vec::new(),
                loop_spans: vec![],
            },
            FileProjection {
                class_shape_fragments: Vec::new(),
                path: "a.ext".to_string(),
                loc: 2,
                symbols: vec![],
                imports: ImportMap::new(),
                re_exports: vec![],
                dynamic_imports: vec![],
                used_names: vec![],
                const_map_fragment: std::collections::HashMap::new(),
                procedure_router_fragments: vec![],
                router_mount_fragments: vec![],
                io: IoFacts::default(),
                degraded: false,
                is_entry: false,
                attributes: Vec::new(),
                loop_spans: vec![],
            },
        ],
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let errors = validate_envelope(&json).unwrap_err();
    assert!(errors.iter().any(|e| e.contains("duplicate path")));
}

#[test]
fn rejects_body_end_less_than_body_start() {
    let mut envelope: NormalizedEnvelope = serde_json::from_str(&valid_envelope_json()).unwrap();
    envelope.files[0]
        .symbols
        .push(symbol("m", Some(10), Some(5)));
    let json = serde_json::to_string(&envelope).unwrap();
    let errors = validate_envelope(&json).unwrap_err();
    assert!(errors.iter().any(|e| e.contains("body_end")));
}

#[test]
fn body_end_equal_to_body_start_is_accepted() {
    let mut envelope: NormalizedEnvelope = serde_json::from_str(&valid_envelope_json()).unwrap();
    envelope.files[0]
        .symbols
        .push(symbol("m", Some(5), Some(5)));
    let json = serde_json::to_string(&envelope).unwrap();
    assert!(validate_envelope(&json).is_ok());
}

#[test]
fn symbol_with_no_body_span_is_never_flagged() {
    let mut envelope: NormalizedEnvelope = serde_json::from_str(&valid_envelope_json()).unwrap();
    envelope.files[0]
        .symbols
        .push(symbol("no-span", None, None));
    let json = serde_json::to_string(&envelope).unwrap();
    assert!(validate_envelope(&json).is_ok());
}

/// `examples/jsp-envelope.example.json` — the JSP contract example `docs/NORMALIZED_AST.md`'s
/// Validation section points at: a hand-written, crude-parser-shaped envelope (symbols with no body
/// spans, one `http` provide + one `db-table` consume, no imports) that must still validate cleanly
/// against this exact contract.
#[test]
fn jsp_contract_example_validates() {
    let json = include_str!("../../../../examples/jsp-envelope.example.json");
    let envelope = validate_envelope(json).expect("jsp-envelope.example.json should validate");
    assert_eq!(envelope.parser, "jsp-lexical/1");
    assert_eq!(envelope.files.len(), 1);
    let file = &envelope.files[0];
    assert_eq!(file.path, "webapp/legacy/user.jsp");
    assert_eq!(file.symbols.len(), 2);
    assert!(file.symbols.iter().all(|s| s.body_start.is_none()));
    assert_eq!(file.io.provides.len(), 1);
    assert_eq!(file.io.provides[0].key, "GET /legacy/user.jsp");
    assert_eq!(file.io.consumes.len(), 1);
    assert_eq!(file.io.consumes[0].key.as_deref(), Some("table:users"));
}

#[test]
fn collects_multiple_errors_at_once() {
    let json = valid_envelope_json()
        .replace("zzop-normalized-ast", "bogus")
        .replace("\"version\": 1", "\"version\": 99");
    let errors = validate_envelope(&json).unwrap_err();
    assert_eq!(errors.len(), 2);
}
