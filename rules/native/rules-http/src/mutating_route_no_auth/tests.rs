//! Unit tests for `scan_mutating_route_no_auth`'s BFS + guard-vocabulary logic in isolation (e2e coverage
//! — real handler-file fixtures — lives in `crates/engine/tests/integration/analyze_io_natives.rs`).
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
        response: None,
        body: None,
        kind: "http".to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
        symbol: Some(handler.to_string()),
        ..Default::default()
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert!(out.is_empty());
}

#[test]
fn provide_with_no_symbol_captured_is_skipped() {
    let provides = vec![zzop_core::IoProvide {
        response: None,
        body: None,
        kind: "http".to_string(),
        key: "POST /anon".to_string(),
        file: "routes/api.ts".to_string(),
        line: 3,
        symbol: None,
        ..Default::default()
    }];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &[],
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn ambiguous_handler_name_resolves_to_the_route_file_and_is_flagged() {
    // NestJS idiom: `@Delete() delete()` decorates a controller method in the route's OWN file, but the
    // bare name `delete` is also defined in three other controllers/services repo-wide. Repo-wide
    // resolution bails as ambiguous (the whole rule was inert on idiomatic NestJS); file-scoped tie-break
    // resolves to user.controller's `delete`, reaches no guard, and correctly flags the unauth route.
    let provides = vec![provide(
        "DELETE /api/users/{}",
        "src/user/user.controller.ts",
        37,
        "delete",
    )];
    let symbols = vec![
        sym("src/user/user.controller.ts", "delete", 37),
        sym("src/article/article.controller.ts", "delete", 24),
        sym("src/user/user.service.ts", "delete", 10),
        sym("src/article/article.service.ts", "delete", 12),
    ];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].file, "src/user/user.controller.ts");
}

#[test]
fn handler_ambiguous_even_within_the_route_file_stays_unresolved() {
    // Two DIFFERENT `delete` methods in the SAME route file (two controller classes in one file) —
    // file-scoping cannot disambiguate, so the rule still does not guess (do-not-guess contract).
    //
    // The fixture used to be `sym("a/ctrl.ts", "delete", 5)` twice, which is NOT this case: both calls
    // mint the same `a/ctrl.ts#delete` id, so it tested one id listed twice and called it ambiguity.
    // That is the shape `build_name_index` now dedups (an overload is one symbol id, not two candidates),
    // so the old fixture would have asserted that an overloaded handler stays unresolved — the exact
    // false negative measured on 2026-08-11. Genuine in-file ambiguity needs two distinct ids.
    let provides = vec![provide("DELETE /api/x/{}", "a/ctrl.ts", 5, "delete")];
    let symbols = vec![
        sym("a/ctrl.ts", "UserCtrl.delete", 5),
        sym("a/ctrl.ts", "AdminCtrl.delete", 20),
    ];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &decorator_guarded,
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &decorator_guarded,
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &decorator_guarded,
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].line, 7);
}

#[test]
fn non_http_provides_are_ignored() {
    let provides = vec![zzop_core::IoProvide {
        response: None,
        body: None,
        kind: "queue".to_string(),
        key: "POST /topic".to_string(),
        file: "routes/api.ts".to_string(),
        line: 3,
        symbol: Some("publish".to_string()),
        ..Default::default()
    }];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &[sym("routes/handlers.ts", "publish", 1)],
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &store,
        unresolved_callees: &Default::default(),
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
    //
    // The `AuthorizationService` CLASS symbol below is not decoration: the qualifier arm's existence
    // gate (`qualifier.rs`, Gate 1) requires the receiver name to be declared somewhere in the tree,
    // and `corpus/oss/be-spring` declares it in `io/spring/core/service/AuthorizationService.java`.
    // Omitting it — as this fixture originally did — models a tree the guard class does not live in.
    let provides = vec![provide(
        "DELETE /articles/{}/comments/{}",
        "src/main/java/io/spring/api/CommentsApi.java",
        67,
        "deleteComment",
    )];
    let symbols = vec![
        sym(
            "src/main/java/io/spring/api/CommentsApi.java",
            "CommentsApi.deleteComment",
            67,
        ),
        sym(
            "src/main/java/io/spring/core/service/AuthorizationService.java",
            "AuthorizationService",
            8,
        ),
    ];
    let graph = vec![edge(
        "src/main/java/io/spring/api/CommentsApi.java#CommentsApi.deleteComment",
        "io.spring.core.service.AuthorizationService#AuthorizationService.canWriteComment",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn java_route_reaching_only_a_domain_noun_class_stays_flagged() {
    // The opus-review recall-regression class: a qualifier that merely CONTAINS a guard substring
    // (`AuthorRepository` ⊃ `auth`) must NOT clear the route — exact-token qualifier matching
    // (`qualifier::is_guard`) sees [author, repository], no vocabulary hit, and the unguarded mutating
    // route stays a finding. `AuthorRepository` is DECLARED below on purpose: that makes the existence
    // gate (Gate 1) pass, so the exact-token gate is the only thing left keeping this route flagged —
    // without the declaration this test would go green for the wrong reason.
    let provides = vec![provide(
        "POST /articles",
        "src/main/java/io/spring/api/ArticlesApi.java",
        28,
        "createArticle",
    )];
    let symbols = vec![
        sym(
            "src/main/java/io/spring/api/ArticlesApi.java",
            "ArticlesApi.createArticle",
            28,
        ),
        sym(
            "src/main/java/io/spring/core/author/AuthorRepository.java",
            "AuthorRepository",
            6,
        ),
    ];
    let graph = vec![edge(
        "src/main/java/io/spring/api/ArticlesApi.java#ArticlesApi.createArticle",
        "io.spring.core.author.AuthorRepository#AuthorRepository.save",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(
        out[0].file,
        "src/main/java/io/spring/api/CurrentUserApi.java"
    );
}

#[test]
fn java_handler_unique_in_the_route_file_resolves_past_an_unrelated_collision() {
    // A `@RestController`'s `@DeleteMapping deleteArticle()` is a method of the class in the route's own
    // file; an unrelated `Other.deleteArticle` elsewhere makes the bare tail-name ambiguous repo-wide.
    // File-scoped tie-break resolves to ArticleApi's handler (evidence: the route provide points at that
    // file), reaches no guard here, and fires — the same NestJS-style improvement, on a Java shape.
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].file, "src/main/java/io/spring/api/ArticleApi.java");
}

#[test]
fn fastapi_di_alias_receiver_is_not_guard_evidence_and_the_route_still_fires() {
    // SEALS: a receiver whose "class" NO SYMBOL DECLARES can never clear a mutating route (the
    // phantom-node false suppression). Shaped on `corpus/oss/be-fastapi-fs` verbatim, including the
    // import line that MINTS the phantom — `backend/app/api/routes/items.py` line 7 reads
    //     from app.api.deps import CurrentUser, SessionDep
    // so `SessionDep` is in that file's ImportMap; `def create_item(*, session: SessionDep, ...)`
    // types the receiver as `SessionDep`, and `zzop_core::callgraph::resolve_method` mints
    // `backend/app/api/deps.py#SessionDep.add` WITHOUT checking any symbol answers to `SessionDep`.
    // It does not: `SessionDep = Annotated[Session, Depends(get_db)]` is a type alias, which
    // `parser-python-3`'s `const_symbol` (uppercase-literal assignments only) never extracts — hence
    // the symbol list below, which is exactly what that parser produces for these two files. Its camel
    // tokens are [Session, Dep], and `session` was in `QUALIFIER_GUARD_TOKENS`, so this route cleared
    // ITSELF on a symbol that does not exist. Two independent gates now refuse it — the existence gate
    // (nothing declares `SessionDep`) and the narrowed vocabulary (`session` is an entity noun, gone) —
    // which is why this test asserts only the OUTCOME; `qualifier.rs`'s own tests isolate each gate.
    let provides = vec![provide(
        "POST /items/",
        "backend/app/api/routes/items.py",
        61,
        "create_item",
    )];
    let symbols = vec![
        sym("backend/app/api/routes/items.py", "create_item", 61),
        // deps.py's REAL symbols — every module-level `def`. `SessionDep`/`TokenDep`/`CurrentUser`
        // are absent because they are `Annotated[...]` aliases, not declarations this parser emits.
        sym("backend/app/api/deps.py", "get_db", 21),
        sym("backend/app/api/deps.py", "get_current_user", 30),
        sym(
            "backend/app/api/deps.py",
            "get_current_active_superuser",
            51,
        ),
    ];
    let graph = vec![edge(
        "backend/app/api/routes/items.py#create_item",
        "backend/app/api/deps.py#SessionDep.add",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(
        out.len(),
        1,
        "a receiver no symbol declares is not guard evidence: {:?}",
        out
    );
    assert_eq!(out[0].file, "backend/app/api/routes/items.py");
}

#[test]
fn a_python_module_receiver_is_not_qualifier_evidence_on_its_own() {
    // Isolates Gate 1 with a token the narrowed vocabulary KEEPS, so the existence gate is the only
    // thing that can reject it. Shape is `corpus/oss/be-fastapi` verbatim: `app/api/routes/users.py`
    // line 16 reads `from app.services import jwt`, which makes `jwt` an ImportMap entry, and a
    // `jwt.<fn>(...)` call resolves to `app/services/jwt.py#jwt.<fn>`. `jwt` there is a MODULE — a
    // file — so no `SourceSymbol` is named `jwt`, and `resolve_method` never checked. The tail is
    // deliberately `encode` (no `DEFAULT_AUTH_GUARD_PATTERN` hit) so the tail arm cannot decide this;
    // the corpus's real tails DO match it, which is why this narrowing costs the corpus nothing.
    let provides = vec![provide(
        "POST /articles",
        "app/api/routes/articles.py",
        30,
        "create_article",
    )];
    let symbols = vec![
        sym("app/api/routes/articles.py", "create_article", 30),
        sym("app/services/jwt.py", "create_access_token", 12),
    ];
    let graph = vec![edge(
        "app/api/routes/articles.py#create_article",
        "app/services/jwt.py#jwt.encode",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1, "a module receiver is not a class: {:?}", out);
}

#[test]
fn a_petclinic_domain_controller_no_longer_clears_its_own_unauthenticated_route() {
    // SEALS the vocabulary narrowing END TO END, on the exact shape it was measured on:
    // `corpus/oss/spring-petclinic` has no authentication anywhere, yet `POST /owners/new` was silent
    // because its handler's OWN depth-0 symbol id carries the qualifier `OwnerController` ([Owner,
    // Controller]) and `owner` used to be guard vocabulary. `Owner` there is a PET owner — a domain
    // entity, not an authorization concept. Empty graph: the depth-0 self-match is the whole story.
    let provides = vec![provide(
        "POST /owners/new",
        "src/main/java/org/springframework/samples/petclinic/owner/OwnerController.java",
        84,
        "processCreationForm",
    )];
    // BOTH symbols, and the CLASS one is what makes this test test what its name says. `build_name_index`
    // keys on the id's last dot-segment, so the method symbol alone registers `processCreationForm` and
    // nothing named `OwnerController` — Gate 1 (existence) would then reject the qualifier before Gate 2
    // (vocabulary) is ever consulted, and this test would stay green with `owner` put back in
    // `QUALIFIER_GUARD_TOKENS`. The real petclinic tree declares the class, so declaring it here is also
    // the accurate fixture; the sibling two functions above needed the same correction.
    let symbols = vec![
        sym(
            "src/main/java/org/springframework/samples/petclinic/owner/OwnerController.java",
            "OwnerController",
            30,
        ),
        sym(
            "src/main/java/org/springframework/samples/petclinic/owner/OwnerController.java",
            "OwnerController.processCreationForm",
            84,
        ),
    ];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].data.as_ref().unwrap()["path"], "/owners/new");
}

#[test]
fn a_declared_python_class_receiver_still_carries_qualifier_evidence() {
    // The other side of the same gate, so it is a filter and not a Python kill-switch: when the
    // receiver IS a class the tree declares (`self.check_perm()` inside a Django view, which
    // `parser-python-3` rewrites to the enclosing class name), the qualifier arm still clears the
    // route. Only UNDECLARED receiver names lost their vote.
    let provides = vec![provide(
        "POST /articles/",
        "articles/views.py",
        14,
        "create",
    )];
    let symbols = vec![
        sym("articles/views.py", "ArticleViewSet.create", 14),
        sym("articles/views.py", "ArticleViewSet", 9),
        sym("app/core/guards.py", "SessionGuard", 5),
    ];
    // `enforce` is not in `DEFAULT_AUTH_GUARD_PATTERN`, and neither is the handler's own qualifier
    // (`ArticleViewSet`), so the DECLARED `SessionGuard` receiver is the only possible clearing reason.
    let graph = vec![edge(
        "articles/views.py#ArticleViewSet.create",
        "app/core/guards.py#SessionGuard.enforce",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn a_typescript_class_receiver_guard_is_unaffected_by_the_existence_gate() {
    // TS true-suppression regression: `zzop_parser_typescript` only ever sets `receiver_type` from a
    // `new X()` initializer or a `: X` annotation, so a TS qualifier is always a real class/interface
    // name the tree declares — the gate must be a no-op there. `verifyCaller` is deliberately named so
    // the TAIL arm cannot clear this on its own (no `DEFAULT_AUTH_GUARD_PATTERN` hit would be needed
    // if it could), leaving the qualifier arm as the only reason the route is silent.
    let provides = vec![provide(
        "POST /articles",
        "src/article/article.controller.ts",
        22,
        "create",
    )];
    let symbols = vec![
        sym("src/article/article.controller.ts", "create", 22),
        sym("src/auth/permission.service.ts", "PermissionService", 4),
    ];
    let graph = vec![edge(
        "src/article/article.controller.ts#create",
        "src/auth/permission.service.ts#PermissionService.ensureFresh",
    )];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
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
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &store,
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1, "{:?}", out);
    assert_eq!(out[0].data.as_ref().unwrap()["path"], "/public/signup-lite");
}

/// The message may not claim an UNBOUNDED search. `anywhere in its call graph` is literal for the JS/TS
/// extensions and — since 2026-09-07, review ledger V30 — for `.java` too; for a Python
/// module-attribute receiver the walk still stops one hop out, so in THAT case a finding means "no
/// guard within one hop".
///
/// 🔴 The Java half of this bound moved, and why it had to is worth keeping here: the bound was never a
/// missing finding, it was a FALSE one — a Java route whose guard sat two hops away FIRED. Keeping the
/// message honest WHILE the bound existed is what let it be removed cleanly instead of leaving a
/// disclosure that outlived its subject.
///
/// Pinned because the phrase read as an unbounded claim for the whole of 2026-07 while the bound was
/// stated only in `callgraph::run_callgraph_rules`' doc comment — visible to maintainers, invisible to
/// the person holding the finding. That is the name-vs-computation class this rule's own prose is
/// otherwise careful about, and a future edit that trims the qualification must fail here.
#[test]
fn the_message_never_claims_an_unbounded_walk_without_naming_the_hop_bound() {
    let msg =
        super::message::missing_auth_hint("POST", "/api/x", "handler", Some("requireAuth"), &[]);
    assert!(
        msg.contains("anywhere in its call graph"),
        "the phrase this test qualifies must still be there, or the pin is checking nothing: {msg}"
    );
    // Every token here must be UNIQUE to the bound that REMAINS — the Python module-attribute one.
    // `resolves to ITSELF` was in this list until 2026-09-07 and had to leave with the Java bound it
    // described: a token that survives its own subject pins the SENTENCE, not the claim.
    for token in [
        "module-attribute",
        "stops one hop out",
        "within one hop",
        "no guard anywhere",
    ] {
        assert!(
            msg.contains(token),
            "an unbounded-sounding claim must ship WITH its hop bound, missing {token:?}: {msg}"
        );
    }
}

// --- Unresolved callees: a call the resolver could not place is still a name this rule can read ---

/// The measured false positive, in miniature. A guard declared inside a factory is not a top-level
/// symbol, so the resolver draws no edge for a handler that calls it — and this rule, walking edges
/// alone, used to report the route as reaching no guard. Its own message says it looks for a call whose
/// NAME looks like a guard, so asserting the absence from a missing edge was the rule contradicting its
/// own stated criterion. Measured at 8 false positives against 1 true one on a real monorepo.
#[test]
fn an_unresolved_call_whose_name_matches_the_guard_pattern_clears_the_route() {
    let provides = vec![provide(
        "POST /api/spaces/{}",
        "routes/api.ts",
        3,
        "updateSpace",
    )];
    let symbols = vec![sym("routes/api.ts", "updateSpace", 1)];
    let mut unresolved = std::collections::BTreeMap::new();
    unresolved.insert(
        "routes/api.ts#updateSpace".to_string(),
        vec!["requireSpaceOwner".to_string()],
    );
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &unresolved,
    });
    assert!(
        out.is_empty(),
        "`requireSpaceOwner` matches the declared guard pattern — an edge the resolver could not draw \
         is not evidence that the call is absent: {out:?}"
    );
}

/// INVALIDATION, on the axis that matters: same unresolvable call, a name the pattern does NOT match,
/// and the route must still fire. Without this, "treat unresolved callees as guards" would pass the
/// test above while silencing the rule everywhere.
#[test]
fn an_unresolved_call_whose_name_is_not_guard_shaped_still_fires_and_names_it() {
    let provides = vec![provide(
        "POST /api/things/{}",
        "routes/api.ts",
        3,
        "updateThing",
    )];
    let symbols = vec![sym("routes/api.ts", "updateThing", 1)];
    let mut unresolved = std::collections::BTreeMap::new();
    unresolved.insert(
        "routes/api.ts#updateThing".to_string(),
        vec!["doTheThing".to_string(), "formatRow".to_string()],
    );
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &unresolved,
    });
    assert_eq!(out.len(), 1, "neither name is guard-shaped: {out:?}");
    // And the residue rides the finding: this is what lets a reader dismiss the remaining case in
    // seconds when the project's guard is spelled outside its own declared pattern.
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(
        data["unresolvedCallees"],
        serde_json::json!(["doTheThing", "formatRow"]),
        "the names that drew no edge must be visible on the finding: {data}"
    );
}

/// The disclosure is additive-only: a handler whose every call resolved carries no `unresolvedCallees`
/// key at all. An always-present empty array would claim "the resolver placed every call in this run",
/// which is a wider statement than "this handler had none it could not place".
#[test]
fn a_handler_with_no_unresolved_calls_carries_no_residue_key() {
    let provides = vec![provide("POST /users", "routes/api.ts", 3, "createUser")];
    let symbols = vec![sym("routes/api.ts", "createUser", 1)];
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &Default::default(),
    });
    assert_eq!(out.len(), 1);
    assert!(
        out[0].data.as_ref().unwrap()["unresolvedCallees"].is_null(),
        "nothing to say, so nothing is said: {:?}",
        out[0].data
    );
}

/// An unresolved guard-shaped name reached through ANOTHER symbol clears the route too — the check
/// rides the same BFS traversal, so every reachable node's dropped calls count, not just the handler's.
#[test]
fn an_unresolved_guard_one_hop_from_the_handler_also_clears() {
    let provides = vec![provide(
        "POST /api/spaces/{}",
        "routes/api.ts",
        3,
        "updateSpace",
    )];
    let symbols = vec![
        sym("routes/api.ts", "updateSpace", 1),
        sym("routes/api.ts", "applyUpdate", 8),
    ];
    let graph = vec![edge(
        "routes/api.ts#updateSpace",
        "routes/api.ts#applyUpdate",
    )];
    let mut unresolved = std::collections::BTreeMap::new();
    unresolved.insert(
        "routes/api.ts#applyUpdate".to_string(),
        vec!["requireSpaceOwner".to_string()],
    );
    let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
        io_provides: &provides,
        symbols: &symbols,
        symbol_graph: &graph,
        auth_guard_pattern: Some(DEFAULT_AUTH_GUARD_PATTERN),
        qualifier_guard_tokens: crate::QUALIFIER_GUARD_TOKENS,
        auth_acquisition_standalone_pattern: Some(AUTH_ACQUISITION_STANDALONE_PATTERN),
        auth_acquisition_conditional_pattern: Some(AUTH_ACQUISITION_CONDITIONAL_PATTERN),
        auth_family_path_pattern: Some(AUTH_FAMILY_PATH_PATTERN),
        decorator_guarded: &std::collections::HashSet::new(),
        route_attr_store: &zzop_core::AttributeStore::default(),
        unresolved_callees: &unresolved,
    });
    assert!(
        out.is_empty(),
        "the guard is one hop away and unresolvable there — same evidence, one edge further: {out:?}"
    );
}

/// The residue is disclosed in PROSE as well as in `data`. A reader who only sees the message must
/// still learn that the walk was short some calls — the `data` key alone reaches a machine, not a
/// person reading a terminal.
#[test]
fn the_message_names_the_unresolved_calls_when_there_are_any() {
    let with = super::message::missing_auth_hint(
        "POST",
        "/api/x",
        "handler",
        Some("requireAuth"),
        &["doTheThing", "formatRow"],
    );
    assert!(
        with.contains("doTheThing, formatRow") && with.contains("could NOT place"),
        "{with}"
    );
    let without =
        super::message::missing_auth_hint("POST", "/api/x", "handler", Some("requireAuth"), &[]);
    assert!(
        !without.contains("could NOT place"),
        "nothing to say, so nothing is said: {without}"
    );
}

// --- Third-party callback receivers: the one place this rule's own remedy breaks the code ---

/// The remedy sentence ("add a named guard call") is a PRESCRIPTION, and there is a route shape where
/// following it breaks a working system: an inbound callback receiver that a third party's SERVER calls
/// — a payment notification or a webhook delivery. That caller holds no session and carries none of this
/// project's credentials, so an authentication guard on that route answers it 401, the delivery is
/// dropped, and the state it was meant to settle never lands. The message therefore has to talk the
/// reader out of the edit BEFORE it asks for it, and name the control that does belong there (payload
/// signature/HMAC verification).
///
/// TWO assertions, and the second is the one that matters. An external auditor's surviving complaint
/// about the sibling rule that closed this same class of veto was NOT that the caveat was missing — it
/// was that the imperative is the first code token and the caveat sits thousands of characters later,
/// where a reader acting on the first instruction never reaches it. So existence is not enough: the
/// counter-indication must PRECEDE the imperative in the delivered bytes.
///
/// Deliberately target-independent: the shape is named by role ("callback receiver", "webhook",
/// "notification"), never by any vendor, path or corpus tree — the finding that motivated this is one
/// route in one measured repo, and a pin that quotes it would pass for the wrong reason.
#[test]
fn the_remedy_warns_off_third_party_callback_receivers_before_it_asks_for_a_guard() {
    let msg =
        super::message::missing_auth_hint("POST", "/api/x", "handler", Some("requireAuth"), &[]);

    // The imperative this rule leads with, verbatim from the message builder.
    let imperative = msg
        .find("Add an explicit, named guard call")
        .expect("the imperative remedy must still be in the message, or this pin checks nothing");

    // Each token must carry one leg of the counter-indication: the shape, the mechanism that breaks,
    // and the control that belongs there instead. A token that is merely topical would let a future
    // edit keep the word and drop the warning.
    for token in [
        "callback",
        "webhook",
        "no session",
        "401",
        "signature",
        "HMAC",
    ] {
        let at = msg.find(token).unwrap_or_else(|| {
            panic!("the third-party-callback counter-indication must name {token:?}: {msg}")
        });
        assert!(
            at < imperative,
            "the counter-indication ({token:?} at {at}) must PRECEDE the imperative remedy (at \
             {imperative}) — a reader who acts on the first instruction never reaches a caveat that \
             comes after it: {msg}"
        );
    }
}

/// The second and third counter-indications, and the reason the closing sentence had to change.
///
/// An uncontaminated auditor read a real finding on a route whose caller is a person with NO ACCOUNT
/// arriving by a high-entropy link from a confirmation email. The message's exception list named one
/// shape (a third-party callback receiver) and then closed with "Everywhere else, the remedy is the
/// direct one" — and that closer is the defect, not the short list: it turns an INCOMPLETE enumeration
/// into a PUSH, promoting every unlisted legitimate shape to "confirmed". A session guard on that route
/// 401s every attendee cancelling from their email link; the edit succeeds, the code is correct, and a
/// live product flow goes down.
///
/// Separately, the rule's central claim can be outright FALSE on a shape it never sees into: the walk
/// begins at the exported route symbol, and a handler passed as an ARGUMENT to a wrapper draws no call
/// edge, so the body is never entered. That is not a footnote about reach — it is a reason the finding
/// is wrong — so it too must land before the imperative.
///
/// Same discipline as the sibling pin above: POSITION, not presence, and every shape named by ROLE.
#[test]
fn the_remedy_warns_off_account_less_callers_and_wrapped_handlers_before_it_asks_for_a_guard() {
    let msg =
        super::message::missing_auth_hint("POST", "/api/x", "handler", Some("requireAuth"), &[]);

    let imperative = msg
        .find("Add an explicit, named guard call")
        .expect("the imperative remedy must still be in the message, or this pin checks nothing");

    // The unconditional closer is the defect itself. Its absence is asserted directly, because a
    // future edit could satisfy every token below and still re-add the sentence that undoes them.
    assert!(
        !msg.contains("Everywhere else, the remedy is the direct one"),
        "the unconditional closer promotes every UNLISTED legitimate shape to a confirmed finding; \
         the enumeration must close by saying it is examples and handing over the discriminator: {msg}"
    );

    for token in [
        // (1) the walk may never have entered the handler body at all
        "WRAPPED HANDLER",
        "as an ARGUMENT",
        "no call edge",
        "never enters the handler body",
        // (3) a legitimate caller who cannot hold a session and never had an account
        "NO ACCOUNT",
        "high-entropy",
        "magic-link",
        "locks out",
        "CSRF",
        "rate-limit",
        // the enumeration must admit it is an enumeration, and hand over the discriminator
        "NOT THE WHOLE LIST",
        "BY-DESIGN",
        "intended caller",
    ] {
        let at = msg
            .find(token)
            .unwrap_or_else(|| panic!("the counter-indication block must name {token:?}: {msg}"));
        assert!(
            at < imperative,
            "the counter-indication ({token:?} at {at}) must PRECEDE the imperative remedy (at \
             {imperative}) — a reader who acts on the first instruction never reaches a caveat that \
             comes after it: {msg}"
        );
    }
}
