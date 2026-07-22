use super::*;
use serde_json::json;

fn store(attrs: Vec<Attribute>) -> AttributeStore {
    AttributeStore::from_attrs(attrs)
}

#[test]
fn exact_iokey_match_returns_value() {
    let s = store(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /api/users".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }]);
    assert_eq!(
        s.route_attr("http", "POST /api/users", "auth-guarded"),
        Some(&json!(true))
    );
    assert_eq!(
        s.route_attr("http", "POST /api/other", "auth-guarded"),
        None
    );
}

#[test]
fn pathscope_covers_routes_under_prefix_on_segment_boundaries() {
    let s = store(vec![Attribute {
        target: EntityRef::PathScope {
            prefix: "/admin".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }]);
    assert_eq!(
        s.route_attr("http", "DELETE /admin/users/{}", "auth-guarded"),
        Some(&json!(true))
    );
    assert_eq!(
        s.route_attr("http", "POST /admin", "auth-guarded"),
        Some(&json!(true))
    );
    // segment boundary: /administrators is NOT under /admin
    assert_eq!(
        s.route_attr("http", "POST /administrators", "auth-guarded"),
        None
    );
}

#[test]
fn exact_iokey_wins_over_pathscope_even_when_it_says_not_guarded() {
    let s = store(vec![
        Attribute {
            target: EntityRef::PathScope {
                prefix: "/admin".into(),
            },
            key: "auth-guarded".into(),
            value: json!(true),
        },
        Attribute {
            target: EntityRef::IoKey {
                kind: "http".into(),
                key: "POST /admin/webhook".into(),
            },
            key: "auth-guarded".into(),
            value: json!(false),
        },
    ]);
    assert_eq!(
        s.route_attr("http", "POST /admin/webhook", "auth-guarded"),
        Some(&json!(false))
    );
}

#[test]
fn longest_pathscope_wins() {
    let s = store(vec![
        Attribute {
            target: EntityRef::PathScope {
                prefix: "/api".into(),
            },
            key: "tier".into(),
            value: json!("public"),
        },
        Attribute {
            target: EntityRef::PathScope {
                prefix: "/api/admin".into(),
            },
            key: "tier".into(),
            value: json!("private"),
        },
    ]);
    assert_eq!(
        s.route_attr("http", "GET /api/admin/x", "tier"),
        Some(&json!("private"))
    );
}

#[test]
fn unknown_attr_key_or_non_http_scope_returns_none() {
    let s = store(vec![Attribute {
        target: EntityRef::PathScope {
            prefix: "/admin".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }]);
    assert_eq!(s.route_attr("http", "POST /admin/x", "other-key"), None);
    assert_eq!(s.route_attr("queue", "POST /admin/x", "auth-guarded"), None);
}

// --- symbol_attr ---

#[test]
fn symbol_attr_name_only_match() {
    let s = store(vec![Attribute {
        target: EntityRef::Symbol {
            name: "User".into(),
            file: None,
        },
        key: "bound-model".into(),
        value: json!(true),
    }]);
    assert_eq!(
        s.symbol_attr("User", None, "bound-model"),
        Some(&json!(true))
    );
    assert_eq!(
        s.symbol_attr("User", Some("src/models/user.ts"), "bound-model"),
        Some(&json!(true))
    );
}

#[test]
fn symbol_attr_file_disambiguated_match() {
    let s = store(vec![Attribute {
        target: EntityRef::Symbol {
            name: "User".into(),
            file: Some("src/models/user.ts".into()),
        },
        key: "bound-model".into(),
        value: json!(true),
    }]);
    assert_eq!(
        s.symbol_attr("User", Some("src/models/user.ts"), "bound-model"),
        Some(&json!(true))
    );
    // caller passes no file — still matches, the file constraint only applies when both sides have one.
    assert_eq!(
        s.symbol_attr("User", None, "bound-model"),
        Some(&json!(true))
    );
}

#[test]
fn symbol_attr_file_mismatch_misses() {
    let s = store(vec![Attribute {
        target: EntityRef::Symbol {
            name: "User".into(),
            file: Some("src/models/user.ts".into()),
        },
        key: "bound-model".into(),
        value: json!(true),
    }]);
    assert_eq!(
        s.symbol_attr("User", Some("src/models/other.ts"), "bound-model"),
        None
    );
}

#[test]
fn symbol_attr_wrong_key_misses() {
    let s = store(vec![Attribute {
        target: EntityRef::Symbol {
            name: "User".into(),
            file: None,
        },
        key: "bound-model".into(),
        value: json!(true),
    }]);
    assert_eq!(s.symbol_attr("User", None, "model-churn"), None);
}

// --- file_attr ---

#[test]
fn file_attr_match() {
    let s = store(vec![Attribute {
        target: EntityRef::File {
            path: "src/index.ts".into(),
        },
        key: "is-entry".into(),
        value: json!(true),
    }]);
    assert_eq!(s.file_attr("src/index.ts", "is-entry"), Some(&json!(true)));
}

#[test]
fn file_attr_miss_on_different_path_or_key() {
    let s = store(vec![Attribute {
        target: EntityRef::File {
            path: "src/index.ts".into(),
        },
        key: "is-entry".into(),
        value: json!(true),
    }]);
    assert_eq!(s.file_attr("src/other.ts", "is-entry"), None);
    assert_eq!(s.file_attr("src/index.ts", "other-key"), None);
}

// --- from_parts ---

fn overlay_with_attrs(attrs: Vec<Attribute>) -> crate::NormalizedEnvelope {
    crate::NormalizedEnvelope {
        format: crate::NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "test-overlay/1".to_string(),
        source: "test".to_string(),
        files: vec![crate::FileProjection {
            path: "overlay/attrs.json".to_string(),
            loc: 1,
            symbols: Vec::new(),
            imports: crate::ImportMap::new(),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            used_names: Vec::new(),
            const_map_fragment: std::collections::HashMap::new(),
            procedure_router_fragments: Vec::new(),
            router_mount_fragments: Vec::new(),
            class_shape_fragments: Vec::new(),
            io: crate::IoFacts::default(),
            degraded: false,
            is_entry: false,
            attributes: attrs,
            loop_spans: Vec::new(),
        }],
    }
}

#[test]
fn from_parts_merges_native_and_overlay_attributes() {
    let native = vec![Attribute {
        target: EntityRef::PathScope {
            prefix: "/admin".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }];
    let overlays = vec![overlay_with_attrs(vec![Attribute {
        target: EntityRef::File {
            path: "src/index.ts".into(),
        },
        key: "is-entry".into(),
        value: json!(true),
    }])];
    let s = AttributeStore::from_parts(native, &overlays);
    assert_eq!(
        s.route_attr("http", "POST /admin/x", "auth-guarded"),
        Some(&json!(true))
    );
    assert_eq!(s.file_attr("src/index.ts", "is-entry"), Some(&json!(true)));
}

#[test]
fn from_parts_overlay_wins_over_native_for_the_same_target_and_key() {
    let native = vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /admin/webhook".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }];
    let overlays = vec![overlay_with_attrs(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /admin/webhook".into(),
        },
        key: "auth-guarded".into(),
        value: json!(false),
    }])];
    let s = AttributeStore::from_parts(native, &overlays);
    // Overlay entries are appended after native ones, and every lookup returns the FIRST match —
    // so the overlay's explicit `false` must win over the native `true`.
    assert_eq!(
        s.route_attr("http", "POST /admin/webhook", "auth-guarded"),
        Some(&json!(false))
    );
}

#[test]
fn from_overlays_still_delegates_with_no_native_attributes() {
    let overlays = vec![overlay_with_attrs(vec![Attribute {
        target: EntityRef::PathScope {
            prefix: "/admin".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }])];
    let s = AttributeStore::from_overlays(&overlays);
    assert_eq!(
        s.route_attr("http", "POST /admin/x", "auth-guarded"),
        Some(&json!(true))
    );
}

// --- extended ---

#[test]
fn extended_appends_after_existing_so_existing_keeps_first_match_priority() {
    let s = store(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /admin/webhook".into(),
        },
        key: "auth-guarded".into(),
        value: json!(false),
    }]);
    let extended = s.extended(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /admin/webhook".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }]);
    // The pre-existing explicit `false` still wins — the minted `true` only fills gaps.
    assert_eq!(
        extended.route_attr("http", "POST /admin/webhook", "auth-guarded"),
        Some(&json!(false))
    );
}

#[test]
fn extended_fills_gaps_for_targets_the_existing_store_has_no_entry_for() {
    let s = store(Vec::new());
    let extended = s.extended(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /admin/tasks".into(),
        },
        key: "auth-guarded".into(),
        value: json!(true),
    }]);
    assert_eq!(
        extended.route_attr("http", "POST /admin/tasks", "auth-guarded"),
        Some(&json!(true))
    );
    // The original store is untouched — `extended` returns a copy.
    assert!(s.is_empty());
}

// --- attr_is_truthy ---

#[test]
fn attr_is_truthy_cases() {
    assert!(attr_is_truthy(&json!(true)));
    assert!(!attr_is_truthy(&json!(false)));
    assert!(!attr_is_truthy(&json!(0)));
    assert!(!attr_is_truthy(&json!("")));
    assert!(attr_is_truthy(&json!(5)));
    assert!(attr_is_truthy(&json!("nonempty")));
    assert!(!attr_is_truthy(&serde_json::Value::Null));
}
