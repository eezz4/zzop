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
    // The version comes FROM the constant, never from a literal. A fixture that hardcodes it turns
    // every release bump into unrelated test failures — measured on the 0.27.0 -> 0.28.0 bump, which
    // broke four tests here that had nothing to do with the release.
    //
    // Placeholder substitution rather than `format!`: this body is JSON, so `format!` would need every
    // brace in it doubled, and one missed pair is a silently different fixture.
    r#"{
            "format": "zzop-normalized-ast",
            "version": "__CONTRACT_VERSION__",
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
    .replace("__CONTRACT_VERSION__", NORMALIZED_AST_CONTRACT_VERSION)
}

#[test]
fn valid_envelope_round_trips() {
    let envelope = validate_envelope(&valid_envelope_json()).expect("should validate");
    assert_eq!(envelope.format, NORMALIZED_AST_FORMAT);
    assert_eq!(envelope.version, NORMALIZED_AST_CONTRACT_VERSION);
    assert_eq!(envelope.files.len(), 1);
    assert_eq!(envelope.files[0].path, "legacy/user.jsp");
}

#[test]
fn minimal_envelope_with_defaulted_fields_round_trips() {
    // A minimal/degraded producer omits every optional field.
    let json = r#"{
            "format": "zzop-normalized-ast",
            "version": "__CONTRACT_VERSION__",
            "parser": "min/1",
            "source": "s",
            "files": [ { "path": "a.ext", "loc": 1 } ]
        }"#
    .replace("__CONTRACT_VERSION__", NORMALIZED_AST_CONTRACT_VERSION);
    let envelope = validate_envelope(&json).expect("should validate");
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
        version: NORMALIZED_AST_CONTRACT_VERSION.to_string(),
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
            overrides: Default::default(),
            attributes: Vec::new(),
            loop_spans: vec![],
            function_spans: vec![],
            test_spans: Vec::new(),
            calls: Vec::new(),
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
    let json = valid_envelope_json().replace(
        NORMALIZED_AST_CONTRACT_VERSION,
        &one_past_supported_version(),
    );
    let errors = validate_envelope(&json).unwrap_err();
    assert!(errors.iter().any(|e| e.contains("unsupported version")));
}

/// The shape a pre-0.27 producer emits: `version` as a bare integer. It must fail rather than be
/// coerced — a number is not a release, and reading `1` as `0.0.1` would silently accept bytes written
/// against a contract this engine cannot actually reason about.
#[test]
fn rejects_a_bare_integer_version() {
    let json =
        valid_envelope_json().replace(&format!("\"{NORMALIZED_AST_CONTRACT_VERSION}\""), "1");
    let errors = validate_envelope(&json).unwrap_err();
    assert!(
        errors.iter().any(|e| e.contains("invalid JSON")),
        "an integer cannot deserialize into the `String` field at all: {errors:?}"
    );
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
        version: NORMALIZED_AST_CONTRACT_VERSION.to_string(),
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
                overrides: Default::default(),
                attributes: Vec::new(),
                loop_spans: vec![],
                function_spans: vec![],
                test_spans: Vec::new(),
                calls: Vec::new(),
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
                overrides: Default::default(),
                attributes: Vec::new(),
                loop_spans: vec![],
                function_spans: vec![],
                test_spans: Vec::new(),
                calls: Vec::new(),
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

/// `docs/contracts/example-envelope.json` — the JSP contract example `docs/NORMALIZED_AST.md`'s
/// Validation section points at: a hand-written, crude-parser-shaped envelope (symbols with no body
/// spans, one `http` provide + one `db-table` consume, no imports) that must still validate cleanly
/// against this exact contract.
#[test]
fn jsp_contract_example_validates() {
    let json = include_str!("../../../../docs/contracts/example-envelope.json");
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
    // The calls channel: one same-file edge, attributed to the emitting file (the attribution rule
    // `validate_envelope` enforces — this example must demonstrate the conforming shape).
    assert_eq!(file.calls.len(), 1);
    assert_eq!(file.calls[0].from_symbol, "webapp/legacy/user.jsp#getUser");
    assert_eq!(file.calls[0].callee_name, "renderProfile");
    // The declared-response channel, in the fields-direct supply mode (an external producer has no
    // class map to defer to, so it fills `fields` and leaves `dtoRef` unset) — the example must
    // demonstrate the shape the schema documents for adapters, not only the calls channel.
    let response = file.io.provides[0]
        .response
        .as_ref()
        .expect("the example demonstrates a declared response");
    assert!(response.dto_ref.is_none());
    assert_eq!(response.fields.len(), 2);
    assert!(response.complete);
}

#[test]
fn collects_multiple_errors_at_once() {
    let json = valid_envelope_json()
        .replace("zzop-normalized-ast", "bogus")
        .replace(
            NORMALIZED_AST_CONTRACT_VERSION,
            &one_past_supported_version(),
        );
    let errors = validate_envelope(&json).unwrap_err();
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[test]
fn a_json_array_root_reports_the_array_diagnosis_not_a_field_type_mismatch() {
    // Before the fix, a JSON array root fell into serde's struct-from-sequence fallback, reporting a
    // misleading field-level message ("invalid type: integer `1`, expected a string") instead of
    // naming the real problem — the root itself is the wrong shape.
    let errors = validate_envelope("[1,2,3]").unwrap_err();
    assert_eq!(
        errors,
        vec!["expected a JSON object envelope, got an array"]
    );
}

#[test]
fn non_array_non_object_roots_keep_their_already_clear_serde_message() {
    // string/number/bool/null all already hit serde's "invalid type: X, expected struct
    // NormalizedEnvelope" branch (unlike an array, they are never accepted as a positional fallback),
    // so these must NOT be rerouted through the array-only diagnosis.
    for json in ["\"hello\"", "42", "true", "null"] {
        let errors = validate_envelope(json).unwrap_err();
        assert!(
            errors[0].contains("expected struct NormalizedEnvelope"),
            "expected the original clear serde message for {json}, got: {errors:?}"
        );
    }
}

/// The `overrides` contract (introduced in `MIN_VERSION_FOR_OVERRIDES`) — three rules, one per way a
/// declaration could be believed without being honoured. Built as raw JSON rather than through
/// `FileProjection` so these exercise the same path a real adapter's bytes take.
mod overrides {
    use super::*;

    fn envelope_json(version: &str, imports: &str, overrides: &str) -> String {
        format!(
            r#"{{
              "format": "{NORMALIZED_AST_FORMAT}",
              "version": "{version}",
              "parser": "t/1",
              "source": "s",
              "files": [{{
                "path": "src/app.py",
                "loc": 4,
                "imports": {imports},
                "overrides": {{ "imports": {overrides} }}
              }}]
            }}"#
        )
    }

    const BINDS_UTIL: &str =
        r#"{ "util.config": { "specifier": "src.util.config", "original": "*" } }"#;

    #[test]
    fn a_declared_override_with_its_replacement_is_valid_at_the_floor() {
        let json = envelope_json(MIN_VERSION_FOR_OVERRIDES, BINDS_UTIL, r#"["util.config"]"#);
        assert!(
            validate_envelope(&json).is_ok(),
            "the whole point of the field: {:?}",
            validate_envelope(&json).unwrap_err()
        );
    }

    /// RULE 1 — the version floor. Without it, `FileProjection` has no `deny_unknown_fields`, so this
    /// exact envelope handed to an engine built before `overrides` existed would deserialize, be
    /// ignored, and produce a run where the adapter believes it displaced a native fact and the engine
    /// quietly did not. Rejecting it here is what makes the same bytes mean the same thing everywhere.
    ///
    /// Note this is NOT subsumed by the "reject newer than me" comparison: the envelope below declares
    /// an OLDER version, which that comparison accepts. Only the floor catches a mislabel.
    #[test]
    fn an_override_declared_below_the_floor_is_rejected_rather_than_silently_ignored() {
        let json = envelope_json("0.20.0", BINDS_UTIL, r#"["util.config"]"#);
        let errors = validate_envelope(&json).unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("requires version >=")),
            "{errors:?}"
        );
    }

    /// RULE 2 — deletion is not on offer. A name declared with nothing to put in its place asks the
    /// engine to forget a fact it extracted, which has no honest output form (there is no replacement
    /// to disclose) and would let an adapter blind the engine without leaving a trace.
    #[test]
    fn an_override_without_a_replacement_binding_is_a_deletion_and_is_refused() {
        let json = envelope_json(MIN_VERSION_FOR_OVERRIDES, "{}", r#"["util.config"]"#);
        let errors = validate_envelope(&json).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("deletion")), "{errors:?}");
    }

    /// RULE 3 — a repeated name would make the displacement disclosure (one line per declaration)
    /// disagree with the declaration it reports on.
    #[test]
    fn a_duplicate_override_name_is_rejected() {
        let json = envelope_json(
            MIN_VERSION_FOR_OVERRIDES,
            BINDS_UTIL,
            r#"["util.config", "util.config"]"#,
        );
        let errors = validate_envelope(&json).unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("more than once")),
            "{errors:?}"
        );
    }

    /// The default is the contract for everyone else: an envelope that declares no override keeps
    /// working below the floor. The floor is per-feature precisely so that gap-filling adapters pay
    /// nothing for a feature they do not use.
    #[test]
    fn an_envelope_declaring_no_overrides_stays_valid_below_the_floor() {
        let json = envelope_json("0.20.0", BINDS_UTIL, "[]");
        assert!(validate_envelope(&json).is_ok());
    }
}

/// The `calls` channel (introduced in `MIN_VERSION_FOR_CALLS`) — an external submission of call-graph
/// edges, so it carries its own validation the native `RawCall` producers never need. Raw JSON, same
/// rationale as the `overrides` module above.
mod calls {
    use super::*;

    fn envelope_json(version: &str, calls: &str) -> String {
        format!(
            r#"{{
              "format": "{NORMALIZED_AST_FORMAT}",
              "version": "{version}",
              "parser": "t/1",
              "source": "s",
              "files": [{{
                "path": "src/app.rb",
                "loc": 9,
                "calls": {calls}
              }}]
            }}"#
        )
    }

    #[test]
    fn a_well_attributed_call_is_valid_at_the_floor_and_round_trips() {
        let json = envelope_json(
            MIN_VERSION_FOR_CALLS,
            r#"[{ "from_symbol": "src/app.rb#create_user", "callee_name": "require_auth", "line": 12 }]"#,
        );
        let envelope = validate_envelope(&json).unwrap_or_else(|e| panic!("{e:?}"));
        let call = &envelope.files[0].calls[0];
        assert_eq!(call.from_symbol, "src/app.rb#create_user");
        assert_eq!(call.callee_name, "require_auth");
        assert_eq!(call.line, 12);
        assert_eq!(call.receiver_type, None);
        assert!(!call.is_heritage);
        // Optional refinements deserialize too (typed receiver / heritage — `RawCall`'s own fields).
        let json2 = envelope_json(
            MIN_VERSION_FOR_CALLS,
            r#"[{ "from_symbol": "src/app.rb#create_user", "callee_name": "save", "line": 13, "receiver_type": "UserRepo", "is_heritage": false }]"#,
        );
        let envelope2 = validate_envelope(&json2).expect("refined call should validate");
        assert_eq!(
            envelope2.files[0].calls[0].receiver_type.as_deref(),
            Some("UserRepo")
        );
    }

    /// The channel is OPTIONAL — absent deserializes to empty, and the projection stays fully valid.
    #[test]
    fn an_envelope_without_calls_deserializes_to_an_empty_channel() {
        let envelope = validate_envelope(&valid_envelope_json()).expect("should validate");
        assert!(envelope.files[0].calls.is_empty());
    }

    /// The version floor — same mechanism as `overrides`: a mislabelled envelope (old declared
    /// version, populated `calls`) would be silently dropped by an engine predating the field,
    /// leaving every call-graph rule quiet while the producer believes its graph applied.
    #[test]
    fn calls_declared_below_the_floor_are_rejected_rather_than_silently_ignored() {
        let json = envelope_json(
            "0.20.0",
            r#"[{ "from_symbol": "src/app.rb#create_user", "callee_name": "require_auth", "line": 12 }]"#,
        );
        let errors = validate_envelope(&json).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("`calls` requires version >=")),
            "{errors:?}"
        );
    }

    /// Attribution — `from_symbol` must be `<this file's path>#<symbol>`. The whole-graph resolver
    /// buckets calls by that prefix, so a foreign-file prefix would resolve this call against another
    /// file's imports. All three malformed shapes are rejected: a foreign prefix, a missing `#`, and
    /// a bare `"<path>#"` with no symbol name.
    #[test]
    fn a_call_attributed_to_another_file_or_malformed_is_rejected() {
        for bad in [
            r#"[{ "from_symbol": "src/other.rb#helper", "callee_name": "x", "line": 1 }]"#,
            r#"[{ "from_symbol": "src/app.rb", "callee_name": "x", "line": 1 }]"#,
            r#"[{ "from_symbol": "src/app.rb#", "callee_name": "x", "line": 1 }]"#,
        ] {
            let json = envelope_json(MIN_VERSION_FOR_CALLS, bad);
            let errors = validate_envelope(&json).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|e| e.contains("must be '<this file's path>#<symbol>'")),
                "{bad}: {errors:?}"
            );
        }
    }

    /// A `#` inside the PATH of a calls-carrying file passes the attribution check (`strip_prefix`)
    /// but breaks the whole-graph resolver's bucketing (`build_symbol_graph` splits `from_symbol` at
    /// its FIRST `#`), so such a file's calls would resolve under a truncated path it never declared.
    /// The two machines cannot agree on that shape, so the boundary rejects it — and ONLY for files
    /// that actually carry calls (a `#` path with no calls has no bucketing to disagree with).
    #[test]
    fn calls_on_a_path_containing_hash_are_rejected() {
        let json = format!(
            r#"{{
              "format": "{NORMALIZED_AST_FORMAT}",
              "version": "{MIN_VERSION_FOR_CALLS}",
              "parser": "t/1",
              "source": "s",
              "files": [{{
                "path": "src/a#b.rb",
                "loc": 9,
                "calls": [{{ "from_symbol": "src/a#b.rb#create_user", "callee_name": "save", "line": 3 }}]
              }}]
            }}"#
        );
        let errors = validate_envelope(&json).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("'#'") && e.contains("calls")),
            "a calls-carrying '#' path must be rejected naming the mismatch: {errors:?}"
        );

        // The same path WITHOUT calls stays accepted — the rejection is scoped to the one channel
        // whose bucketing the '#' actually breaks, never a blanket path rule.
        let no_calls = format!(
            r#"{{
              "format": "{NORMALIZED_AST_FORMAT}",
              "version": "{MIN_VERSION_FOR_CALLS}",
              "parser": "t/1",
              "source": "s",
              "files": [{{ "path": "src/a#b.rb", "loc": 9 }}]
            }}"#
        );
        assert!(
            validate_envelope(&no_calls).is_ok(),
            "a '#' path with no calls must stay accepted"
        );
    }

    #[test]
    fn an_empty_callee_name_is_rejected() {
        let json = envelope_json(
            MIN_VERSION_FOR_CALLS,
            r#"[{ "from_symbol": "src/app.rb#create_user", "callee_name": "", "line": 1 }]"#,
        );
        let errors = validate_envelope(&json).unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("empty callee_name")),
            "{errors:?}"
        );
    }
}
