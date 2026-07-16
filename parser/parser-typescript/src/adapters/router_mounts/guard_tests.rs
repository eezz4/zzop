//! Coverage for `guard::judge_guard_arg`/`is_guard_name` as exercised through
//! `extract_router_mount_fragments`'s Express `.use`/route-level middle-arg classification
//! (express-middleware-v1): `ScopedAttr`/`attr_keys` emission, the router-name veto, the
//! guard-name true/false vocabulary, `MIDDLEWARE_GUARD_CALLEES` certainty, multi-middleware
//! independent judgment, and the Hono/non-Express negative space.
use super::extract_router_mount_fragments;
use super::guard::AUTH_GUARDED_ATTR_KEY;
use super::tests_hono::frag;
use zzop_core::RouterMountEntry;

#[test]
fn use_call_with_prefix_judged_guard_emits_a_scoped_attr() {
    let src = concat!(
        "const app = express();\n",
        "app.use('/admin', requireAuth());\n",
        "app.get('/ok', h);\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![
            RouterMountEntry::ScopedAttr {
                prefix: "/admin".into(),
                key: AUTH_GUARDED_ATTR_KEY.into(),
                line: 2,
            },
            RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/ok".into(),
                handler: Some("h".into()),
                line: 3,
                attr_keys: vec![],
            },
        ]
    );
}

#[test]
fn use_call_without_prefix_judged_guard_emits_a_scoped_attr_at_root() {
    let src = concat!(
        "const app = express();\n",
        "app.use(requireAuth());\n",
        "app.get('/ok', h);\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![
            RouterMountEntry::ScopedAttr {
                prefix: "/".into(),
                key: AUTH_GUARDED_ATTR_KEY.into(),
                line: 2,
            },
            RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/ok".into(),
                handler: Some("h".into()),
                line: 3,
                attr_keys: vec![],
            },
        ]
    );
}

#[test]
fn use_ident_judged_guard_mints_a_mount_carrying_attr_keys() {
    // `authMiddleware` is a bare ident, not a call — it still mints the same conservative
    // (unresolving) `Mount` this recognizer always minted for a bare-ident `.use` argument, now
    // additionally carrying the judged `attr_keys` so the compose pass's PathScope fallback fires
    // for it.
    let src = "const app = express();\napp.use('/admin', authMiddleware);\n";
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/admin".into(),
            ident: "authMiddleware".into(),
            specifier: None,
            attr_keys: vec![AUTH_GUARDED_ATTR_KEY.to_string()],
        }]
    );
}

#[test]
fn auth_router_suffixed_ident_is_vetoed_not_judged_as_a_guard() {
    // `authRouter` matches the guard NAME pattern ("auth") but is vetoed by the router-name
    // suffix pattern — it is a sub-router, not a middleware guard, so its Mount carries no
    // attr_keys.
    let src = "const app = express();\napp.use('/admin', authRouter);\n";
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/admin".into(),
            ident: "authRouter".into(),
            specifier: None,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn guard_name_true_cases_judge_as_guards() {
    // Reviewer-pinned finding-1 true cases: each name mints a Mount whose attr_keys carries the
    // guard key via the explicit `is_guard_name` predicate (no-lookaround workaround for the
    // "auth but not author" rule, plus the suffix-only jwt/token/apikey/guard rules).
    let names = [
        "requireAuth",
        "authorize",
        "verifyToken",
        "checkJwt",
        "apiKeyAuth",
        "isAuthenticated",
    ];
    for name in names {
        let src = format!("const app = express();\napp.use('/admin', {name});\n");
        let out = extract_router_mount_fragments("app.ts", &src, &[]);
        let entries = frag(&out, "app").entries.clone();
        assert_eq!(
            entries,
            vec![RouterMountEntry::Mount {
                prefix: "/admin".into(),
                ident: name.into(),
                specifier: None,
                attr_keys: vec![AUTH_GUARDED_ATTR_KEY.to_string()],
            }],
            "{name} must judge as a guard: {entries:?}"
        );
    }
}

#[test]
fn guard_name_false_cases_do_not_judge_as_guards() {
    // Reviewer-pinned finding-1 traps (tokenizer/tokenBucket/verifyContentLength — bare
    // "token"/"verify" dropped) and finding-2 vetoes (authController/authService/authClient/
    // authRouter/authRoutes — sub-router/DI shapes, not guards) plus the pre-existing
    // session/author exclusions: every name mints a Mount with NO attr_keys.
    let names = [
        "tokenizer",
        "tokenBucket",
        "verifyContentLength",
        "session",
        "author",
        "authController",
        "authService",
        "authClient",
        "authRouter",
        "authRoutes",
    ];
    for name in names {
        let src = format!("const app = express();\napp.use('/admin', {name});\n");
        let out = extract_router_mount_fragments("app.ts", &src, &[]);
        let entries = frag(&out, "app").entries.clone();
        assert_eq!(
            entries,
            vec![RouterMountEntry::Mount {
                prefix: "/admin".into(),
                ident: name.into(),
                specifier: None,
                attr_keys: vec![],
            }],
            "{name} must NOT judge as a guard: {entries:?}"
        );
    }
}

#[test]
fn session_call_is_not_judged_as_a_guard() {
    // express-session adds session STATE, it does not reject requests — deliberately excluded
    // from the guard-name vocabulary (see `is_guard_name`'s doc).
    let src = "const app = express();\napp.use(session());\napp.get('/ok', h);\n";
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/ok".into(),
            handler: Some("h".into()),
            line: 3,
            attr_keys: vec![],
        }],
        "session() must never be judged a guard: {out:?}"
    );
}

#[test]
fn passport_authenticate_callee_is_guard_certain_regardless_of_argument() {
    let src = concat!(
        "const app = express();\n",
        "app.use('/api', passport.authenticate('jwt'));\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::ScopedAttr {
            prefix: "/api".into(),
            key: AUTH_GUARDED_ATTR_KEY.into(),
            line: 2,
        }]
    );
}

#[test]
fn multi_middleware_use_judges_each_call_arg_independently() {
    // `.use(prefixLit, mw1, mw2)` (3+ args) never mints a Mount; each CALL-shaped arg is judged
    // on its own, so only the guard-judged one emits a ScopedAttr.
    let src = concat!(
        "const app = express();\n",
        "app.use('/api', requireAuth(), rateLimit());\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::ScopedAttr {
            prefix: "/api".into(),
            key: AUTH_GUARDED_ATTR_KEY.into(),
            line: 2,
        }]
    );
}

#[test]
fn multi_middleware_use_with_no_literal_prefix_scopes_at_root() {
    // `.use(mw1, mw2)` — 2 args, neither a literal prefix — falls to the same multi-middleware
    // scan (never the literal-prefixed 2-arg Mount/ScopedAttr path), scoped at "/".
    let src = "const app = express();\napp.use(requireAuth(), rateLimit());\n";
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::ScopedAttr {
            prefix: "/".into(),
            key: AUTH_GUARDED_ATTR_KEY.into(),
            line: 2,
        }]
    );
}

#[test]
fn route_level_middle_arg_guard_ident_and_call_both_judge_the_verb() {
    let src = concat!(
        "const router = express.Router();\n",
        "router.post('/a', requireAuth, handlerA);\n",
        "router.post('/b', requireAuth(), handlerB);\n"
    );
    let out = extract_router_mount_fragments("router.ts", src, &[]);
    assert_eq!(
        frag(&out, "router").entries,
        vec![
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/a".into(),
                handler: Some("handlerA".into()),
                line: 2,
                attr_keys: vec![AUTH_GUARDED_ATTR_KEY.to_string()],
            },
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/b".into(),
                handler: Some("handlerB".into()),
                line: 3,
                attr_keys: vec![AUTH_GUARDED_ATTR_KEY.to_string()],
            },
        ]
    );
}

#[test]
fn two_arg_verb_never_carries_attr_keys() {
    let src = concat!(
        "const router = express.Router();\n",
        "router.post('/a', handlerA);\n"
    );
    let out = extract_router_mount_fragments("router.ts", src, &[]);
    assert_eq!(
        frag(&out, "router").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/a".into(),
            handler: Some("handlerA".into()),
            line: 2,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn non_express_receiver_use_is_unaffected_by_guard_judgment() {
    // A `router_names`-configured receiver with no Express structural signal never gets the
    // `.use` mount rule at all (same as before this batch) — guard judgment never runs for it.
    let src = "apiRoutes.use(requireAuth());\napiRoutes.get('/x', h);\n";
    let out = extract_router_mount_fragments("app.ts", src, &["apiRoutes"]);
    assert_eq!(
        frag(&out, "apiRoutes").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/x".into(),
            handler: Some("h".into()),
            line: 2,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn hono_use_never_emits_a_scoped_attr_even_for_a_guard_named_call() {
    // Hono's `.use` is always middleware, never mount/guard vocabulary — the Express-only gate
    // (`is_express`) means a guard-named call argument is never judged for a Hono receiver.
    let src = concat!(
        "const route = new Hono();\n",
        "route.use(requireAuth());\n",
        "route.get('/ok', h);\n"
    );
    let out = extract_router_mount_fragments("route.ts", src, &[]);
    assert_eq!(
        frag(&out, "route").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/ok".into(),
            handler: Some("h".into()),
            line: 3,
            attr_keys: vec![],
        }],
        "Hono's .use must never judge guard vocabulary: {out:?}"
    );
}
