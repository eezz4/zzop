//! Unit tests for `scan_mutating_route_no_auth`'s BFS + guard-vocabulary logic in isolation (e2e coverage
//! — real handler-file fixtures — lives in `crates/engine/tests/analyze_io_natives.rs`).
use super::*;
use zzop_core::callgraph::SymbolEdge;
use zzop_core::SourceSymbolKind;

fn sym(file: &str, name: &str, line: u32) -> SourceSymbol {
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.to_string(),
        name: name.to_string(),
        kind: SourceSymbolKind::Function,
        line,
        exported: true,
        is_default: false,
        body_start: Some(line),
        body_end: Some(line),
        write_sites: Vec::new(),
    }
}

fn provide(key: &str, file: &str, line: u32, handler: &str) -> zzop_core::IoProvide {
    zzop_core::IoProvide {
        body: None,
        kind: "http".to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
        symbol: Some(handler.to_string()),
    }
}

fn edge(from: &str, to: &str) -> SymbolEdge {
    SymbolEdge {
        from: from.to_string(),
        to: to.to_string(),
    }
}

#[test]
fn mutating_handler_never_reaching_a_guard_is_flagged() {
    let provides = vec![provide("POST /users", "routes/api.ts", 3, "createUser")];
    let symbols = vec![sym("routes/handlers.ts", "createUser", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].file, "routes/api.ts");
    assert_eq!(out[0].line, 3);
    assert_eq!(out[0].rule_id, "mutating-route-no-auth");
    assert_eq!(out[0].severity, Severity::Info);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "POST");
    assert_eq!(data["path"], "/users");
}

#[test]
fn auth_acquisition_route_is_exempt_even_when_never_guarded() {
    let provides = vec![provide(
        "POST /api/auth/register",
        "routes/api.ts",
        3,
        "register",
    )];
    let symbols = vec![sym("routes/handlers.ts", "register", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn standalone_exempt_segment_is_exempt_alone_with_no_auth_family_segment_present() {
    let provides = vec![provide("POST /signup", "routes/api.ts", 3, "signup")];
    let symbols = vec![sym("routes/handlers.ts", "signup", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn conditional_segment_paired_with_an_auth_family_segment_is_exempt() {
    // /auth/register — "register" (conditional tier) paired with "auth" (auth-family) elsewhere in the
    // same path is exempt.
    let provides = vec![provide(
        "POST /auth/register",
        "routes/api.ts",
        3,
        "register",
    )];
    let symbols = vec![sym("routes/handlers.ts", "register", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn conditional_segment_alone_with_no_auth_family_segment_is_not_exempt() {
    // /devices/register — "register" alone (no auth-family segment anywhere in the path) is over-broad
    // to exempt: a device-registration endpoint has nothing to do with authentication, so this route is
    // checked normally.
    let provides = vec![provide(
        "POST /devices/register",
        "routes/api.ts",
        3,
        "registerDevice",
    )];
    let symbols = vec![sym("routes/handlers.ts", "registerDevice", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["path"], "/devices/register");
}

#[test]
fn conditional_segment_token_refresh_with_no_auth_family_segment_is_not_exempt() {
    // /token/refresh — both segments are conditional-tier; with no auth-family segment present, this
    // route is checked normally rather than assumed to be the auth-acquisition surface. Handler name
    // deliberately avoids any guard-vocabulary substring (`auth`/`guard`/`verify`/`session`/`token`/
    // `permission`/`acl`) so this isolates the PATH exemption from the separate guard-NAME match.
    let provides = vec![provide(
        "POST /token/refresh",
        "routes/api.ts",
        3,
        "renewCredentials",
    )];
    let symbols = vec![sym("routes/handlers.ts", "renewCredentials", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
}

#[test]
fn a_path_segment_that_only_contains_auth_as_a_substring_is_not_exempt() {
    // Handler name deliberately avoids an "auth" substring (unlike the path) so this isolates the
    // PATH-segment exemption from the separate, unrelated guard-vocabulary name match that would
    // independently clear a handler literally named e.g. `updateAuthorProfile` at BFS depth 0.
    let provides = vec![provide(
        "POST /author/profile",
        "routes/api.ts",
        3,
        "patchWriterBio",
    )];
    let symbols = vec![sym("routes/handlers.ts", "patchWriterBio", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
}

#[test]
fn handler_reaching_a_guard_call_across_an_edge_is_not_flagged() {
    let provides = vec![provide("POST /users", "routes/api.ts", 3, "createUser")];
    let symbols = vec![
        sym("routes/handlers.ts", "createUser", 1),
        sym("routes/handlers.ts", "requireAuth", 2),
    ];
    let graph = vec![edge(
        "routes/handlers.ts#createUser",
        "routes/handlers.ts#requireAuth",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn handler_named_like_a_guard_itself_clears_at_depth_zero() {
    let provides = vec![provide(
        "DELETE /users/{}",
        "routes/api.ts",
        4,
        "deleteUserWithAuthCheck",
    )];
    let symbols = vec![sym("routes/handlers.ts", "deleteUserWithAuthCheck", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn safe_methods_are_never_checked() {
    let provides = vec![provide("GET /users", "routes/api.ts", 3, "listUsers")];
    let symbols = vec![sym("routes/handlers.ts", "listUsers", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn ambiguous_handler_name_defined_in_two_files_is_skipped() {
    let provides = vec![provide("POST /dup", "routes/api.ts", 3, "dup")];
    let symbols = vec![sym("a.ts", "dup", 1), sym("b.ts", "dup", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn provide_with_no_symbol_captured_is_skipped() {
    let provides = vec![zzop_core::IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "POST /anon".to_string(),
        file: "routes/api.ts".to_string(),
        line: 3,
        symbol: None,
    }];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &[],
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn route_registered_in_a_test_fixture_file_is_skipped() {
    let provides = vec![provide(
        "POST /users",
        "routes/__tests__/api.test.ts",
        3,
        "createUser",
    )];
    let symbols = vec![sym("routes/__tests__/api.test.ts", "createUser", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn handler_reaching_a_require_prefixed_ownership_guard_is_not_flagged() {
    let provides = vec![provide(
        "DELETE /guilds/{}",
        "routes/api.ts",
        3,
        "deleteGuild",
    )];
    let symbols = vec![
        sym("routes/handlers.ts", "deleteGuild", 1),
        sym("routes/handlers.ts", "requireGuildOwner", 2),
    ];
    let graph = vec![edge(
        "routes/handlers.ts#deleteGuild",
        "routes/handlers.ts#requireGuildOwner",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn handler_reaching_only_input_validation_require_helpers_is_still_flagged() {
    // `requireBody`/`requireJson` are input-validation middleware, not auth — a blanket
    // `require[A-Z]\w*` recognizer would silently clear this genuine missing-auth case.
    // Only auth-stemmed names may clear.
    let provides = vec![provide("POST /users", "routes/api.ts", 3, "createUser")];
    let symbols = vec![
        sym("routes/handlers.ts", "createUser", 1),
        sym("routes/handlers.ts", "requireBody", 2),
        sym("routes/handlers.ts", "requireJson", 3),
    ];
    let graph = vec![
        edge(
            "routes/handlers.ts#createUser",
            "routes/handlers.ts#requireBody",
        ),
        edge(
            "routes/handlers.ts#createUser",
            "routes/handlers.ts#requireJson",
        ),
    ];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].file, "routes/api.ts");
}

#[test]
fn handler_reaching_only_an_env_gate_is_flagged() {
    // An env gate (`isLocal`/`isProduction`/`isDev`) decides WHERE code runs, not WHO may call it — it is
    // NOT authorization. A mutating route whose only guard-reachable name is an env gate must still fire
    // (clearing on it was a silent missing-auth suppression — the env-gate=auth category error).
    let provides = vec![provide(
        "POST /debug/reset",
        "routes/api.ts",
        3,
        "resetDebugState",
    )];
    let symbols = vec![
        sym("routes/handlers.ts", "resetDebugState", 1),
        sym("config.ts", "isLocal", 2),
    ];
    let graph = vec![edge(
        "routes/handlers.ts#resetDebugState",
        "config.ts#isLocal",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(
        out.len(),
        1,
        "env gate is not authorization — must fire: {:?}",
        out
    );
    assert_eq!(out[0].file, "routes/api.ts");
}

#[test]
fn require_lowercase_substring_in_an_unrelated_word_does_not_false_clear() {
    // "checkUnrequiredParam" contains "require" as a lowercase substring ("unREQUIREd"), but it is
    // never followed by a capital letter — the case-sensitive `(?-i:require[A-Z]\w*)` branch must NOT
    // match it, so this handler, which reaches only this call, is still flagged as unguarded.
    let provides = vec![provide("POST /setup", "routes/api.ts", 3, "runSetup")];
    let symbols = vec![
        sym("routes/handlers.ts", "runSetup", 1),
        sym("routes/handlers.ts", "checkUnrequiredParam", 2),
    ];
    let graph = vec![edge(
        "routes/handlers.ts#runSetup",
        "routes/handlers.ts#checkUnrequiredParam",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
}

#[test]
fn decorator_guarded_line_is_exempt_before_entering_the_bfs() {
    // Empty symbol_graph and a handler name with no guard-vocabulary substring — the BFS alone would
    // find nothing. The provide's own (file, line) is in `decorator_guarded`, so it must never be flagged,
    // proving the exemption applies BEFORE/INSTEAD of the BFS.
    let provides = vec![provide(
        "POST /items",
        "items.controller.ts",
        5,
        "handleApiPost",
    )];
    let symbols = vec![sym("items.controller.ts", "handleApiPost", 5)];
    let mut decorator_guarded = std::collections::HashSet::new();
    decorator_guarded.insert(("items.controller.ts".to_string(), 5));
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &decorator_guarded,
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn a_provide_whose_line_is_not_in_decorator_guarded_is_still_flagged_normally() {
    // Regression guard: `decorator_guarded` containing some OTHER line does not blanket-suppress the rule —
    // only the exact (file, line) pairs it names are exempt.
    let provides = vec![provide(
        "POST /items",
        "items.controller.ts",
        5,
        "handleApiPost",
    )];
    let symbols = vec![sym("items.controller.ts", "handleApiPost", 5)];
    let mut decorator_guarded = std::collections::HashSet::new();
    decorator_guarded.insert(("items.controller.ts".to_string(), 99));
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &decorator_guarded,
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
}

#[test]
fn decorator_guarded_exemption_is_precise_per_route_in_a_shared_controller() {
    // End-to-end-flavored: two routes in one controller file, only one line present in
    // `decorator_guarded` (simulating method-level-only guarding) — the guarded one is exempt, the other
    // still fires. Neither handler name nor the empty symbol_graph offers the BFS anything to find.
    let provides = vec![
        provide("POST /items/a", "items.controller.ts", 4, "createA"),
        provide("POST /items/b", "items.controller.ts", 7, "createB"),
    ];
    let symbols = vec![
        sym("items.controller.ts", "createA", 4),
        sym("items.controller.ts", "createB", 7),
    ];
    let mut decorator_guarded = std::collections::HashSet::new();
    decorator_guarded.insert(("items.controller.ts".to_string(), 4));
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &decorator_guarded,
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].line, 7);
}

#[test]
fn non_http_provides_are_ignored() {
    let provides = vec![zzop_core::IoProvide {
        body: None,
        kind: "queue".to_string(),
        key: "POST /topic".to_string(),
        file: "routes/api.ts".to_string(),
        line: 3,
        symbol: Some("publish".to_string()),
    }];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &[sym("routes/handlers.ts", "publish", 1)],
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn injected_auth_guarded_attribute_on_the_route_iokey_exempts_it() {
    // Empty graph + a handler name with no guard vocabulary — the BFS alone clears nothing. An
    // injected `auth-guarded` attribute on the route's exact IoKey (middleware the BFS can't see)
    // exempts it, the injection completion of the middleware precision limit.
    let provides = vec![provide("POST /items", "routes/api.ts", 3, "createItem")];
    let symbols = vec![sym("routes/handlers.ts", "createItem", 1)];
    let store = zzop_core::AttributeStore::from_attrs(vec![zzop_core::Attribute {
        target: zzop_core::EntityRef::IoKey {
            kind: "http".to_string(),
            key: "POST /items".to_string(),
        },
        key: AUTH_GUARDED_ATTR.to_string(),
        value: serde_json::json!(true),
    }]);
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &store,
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn java_route_reaching_an_authorization_service_static_guard_is_not_flagged() {
    // Real-shape regression for corpus/oss/be-spring's CommentsApi.deleteComment now that `.java` is
    // inside `CALL_GRAPH_COVERED_EXTENSIONS` (`java21/tree-sitter-java-0.23.5/v2`'s `RawCall` extractor +
    // `run_callgraph_rules`'s Java wiring). The inline guard `AuthorizationService.canWriteComment(...)`
    // resolves to an id whose TAIL alone (`canWriteComment`) does NOT match `DEFAULT_AUTH_GUARD_PATTERN`
    // — it's the QUALIFIER (`AuthorizationService`, containing `auth`) that carries the guard evidence,
    // which `is_guard_id`'s two-segment check (module doc "Match granularity") now reaches. This edge
    // shape (`<opaque-specifier>#AuthorizationService.canWriteComment`) is exactly what
    // `run_callgraph_rules`'s Java `resolve_file_fn` produces for a statically-imported guard call.
    let provides = vec![provide(
        "DELETE /articles/{}/comments/{}",
        "src/main/java/io/spring/api/CommentsApi.java",
        67,
        "deleteComment",
    )];
    let symbols = vec![sym(
        "src/main/java/io/spring/api/CommentsApi.java",
        "CommentsApi.deleteComment",
        67,
    )];
    let graph = vec![edge(
        "src/main/java/io/spring/api/CommentsApi.java#CommentsApi.deleteComment",
        "io.spring.core.service.AuthorizationService#AuthorizationService.canWriteComment",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn java_route_reaching_only_a_domain_noun_class_stays_flagged() {
    // The opus-review recall-regression class: a qualifier that merely CONTAINS a guard substring
    // (`AuthorRepository` ⊃ `auth`) must NOT clear the route — exact-token qualifier matching
    // (`qualifier::qualifier_is_guard`) sees [author, repository], no vocabulary hit, and the
    // unguarded mutating route stays a finding.
    let provides = vec![provide(
        "POST /articles",
        "src/main/java/io/spring/api/ArticlesApi.java",
        28,
        "createArticle",
    )];
    let symbols = vec![sym(
        "src/main/java/io/spring/api/ArticlesApi.java",
        "ArticlesApi.createArticle",
        28,
    )];
    let graph = vec![edge(
        "src/main/java/io/spring/api/ArticlesApi.java#ArticlesApi.createArticle",
        "io.spring.core.author.AuthorRepository#AuthorRepository.save",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
}

#[test]
fn java_route_with_no_reachable_guard_is_flagged_now_that_java_is_covered() {
    // Real-shape regression for corpus/oss/be-spring's CurrentUserApi.updateProfile: no guard anywhere
    // reachable in the call graph (it calls only `userService.updateUser(...)`, an unresolvable instance
    // receiver — dropped, never guessed). `.java` being inside `CALL_GRAPH_COVERED_EXTENSIONS` means this
    // is now a genuine finding rather than an accidental exemption.
    let provides = vec![provide(
        "PUT /user",
        "src/main/java/io/spring/api/CurrentUserApi.java",
        40,
        "updateProfile",
    )];
    let symbols = vec![sym(
        "src/main/java/io/spring/api/CurrentUserApi.java",
        "CurrentUserApi.updateProfile",
        40,
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(
        out[0].file,
        "src/main/java/io/spring/api/CurrentUserApi.java"
    );
}

#[test]
fn java_ambiguous_handler_name_is_still_skipped_not_guessed() {
    // Same "do not guess" ambiguity bailout as `ambiguous_handler_name_defined_in_two_files_is_skipped`,
    // exercised on a Java-flavored shape: `.java` becoming call-graph-covered doesn't touch the
    // `resolve_handler` ambiguity gate, which runs BEFORE the BFS ever sees the (here, empty) graph.
    let provides = vec![provide(
        "DELETE /articles/{}",
        "src/main/java/io/spring/api/ArticleApi.java",
        66,
        "deleteArticle",
    )];
    let symbols = vec![
        sym(
            "src/main/java/io/spring/api/ArticleApi.java",
            "ArticleApi.deleteArticle",
            66,
        ),
        sym(
            "src/main/java/io/spring/other/Other.java",
            "Other.deleteArticle",
            10,
        ),
    ];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn injected_pathscope_auth_guarded_exempts_every_route_under_the_prefix() {
    // A router-level middleware guards `/admin/*`; injected as a PathScope, it clears both routes
    // under it without naming each — while a route OUTSIDE the scope still fires.
    let provides = vec![
        provide("DELETE /admin/users/{}", "routes/api.ts", 3, "deleteUser"),
        provide("POST /public/signup-lite", "routes/api.ts", 5, "createLite"),
    ];
    let symbols = vec![
        sym("routes/handlers.ts", "deleteUser", 1),
        sym("routes/handlers.ts", "createLite", 2),
    ];
    let store = zzop_core::AttributeStore::from_attrs(vec![zzop_core::Attribute {
        target: zzop_core::EntityRef::PathScope {
            prefix: "/admin".to_string(),
        },
        key: AUTH_GUARDED_ATTR.to_string(),
        value: serde_json::json!(true),
    }]);
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &store,
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].data.as_ref().unwrap()["path"], "/public/signup-lite");
}
