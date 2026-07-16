//! Coverage (this file + `tests_express.rs`): chained-builder entries (verb + mount, in source
//! order), the last-arg handler rule, separate-statement style, `export default` fragments,
//! cross-file `Mount` specifier resolution, `router_names` config, typed `: Hono` params, the "no
//! structural signal" precision guard, non-literal path/prefix skip, ignored `.all`/`.use`,
//! determinism, and the Express vocabulary (bare/chained receivers, `.use` mounts, the ≥2-arg
//! verb guard, and Hono's `.use` never mounting) — plus the named-import `Router()` vocabulary
//! (bare/aliased import gate, no-import non-recognition, single-arg `.use` prefix-less mount,
//! non-identifier single-arg `.use` skip, and `Router()` as a chain root incl. `export default`).
//! This file holds the Hono-vocabulary + shared-behavior half.
use super::extract_router_mount_fragments;
use zzop_core::{RouterMountEntry, RouterMountFragment};

pub(super) fn frag<'a>(out: &'a [RouterMountFragment], name: &str) -> &'a RouterMountFragment {
    out.iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fragment named {name:?} in {out:?}"))
}

#[test]
fn auth_router_chained_builder_collects_verb_and_mount_entries() {
    let src = concat!(
        "import { sessionRoute } from './routes/session';\n",
        "export const auth = new Hono<HonoAuthContext>()\n",
        "  .use(async (c, next) => { await next(); })\n",
        "  .get('/csrf', csrfHandler)\n",
        "  .route('/', sessionRoute)\n",
        "  .route('/two-factor', twoFactorRoute);\n"
    );
    let out = extract_router_mount_fragments("auth.ts", src, &[]);
    assert_eq!(
        frag(&out, "auth").entries,
        vec![
            RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/csrf".into(),
                handler: Some("csrfHandler".into()),
                line: 4,
                attr_keys: vec![],
            },
            RouterMountEntry::Mount {
                prefix: "/".into(),
                ident: "sessionRoute".into(),
                specifier: Some("./routes/session".into()),
                attr_keys: vec![],
            },
            RouterMountEntry::Mount {
                prefix: "/two-factor".into(),
                ident: "twoFactorRoute".into(),
                specifier: None,
                attr_keys: vec![],
            },
        ]
    );
}

#[test]
fn sub_route_module_last_arg_is_the_handler() {
    let src = concat!(
        "export const twoFactorRoute = new Hono<T>()\n",
        "  .post('/setup', handler)\n",
        "  .post('/enable', sValidator('json', Schema), enableHandler);\n"
    );
    let out = extract_router_mount_fragments("two-factor.ts", src, &[]);
    assert_eq!(
        frag(&out, "twoFactorRoute").entries,
        vec![
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/setup".into(),
                handler: Some("handler".into()),
                line: 2,
                attr_keys: vec![],
            },
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/enable".into(),
                handler: Some("enableHandler".into()),
                line: 3,
                attr_keys: vec![],
            },
        ]
    );
}

#[test]
fn separate_statement_style_named_binding_wins_over_default() {
    let src = concat!(
        "const route = new Hono();\n",
        "route.get('/envelope/:envelopeId/item', h);\n",
        "export default route;\n"
    );
    let out = extract_router_mount_fragments("routes.ts", src, &[]);
    assert_eq!(
        frag(&out, "route").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/envelope/:envelopeId/item".into(),
            handler: Some("h".into()),
            line: 2,
            attr_keys: vec![],
        }]
    );
    assert!(
        out.iter().all(|f| f.name != "default"),
        "a named binding must win over a synthesized \"default\" fragment: {out:?}"
    );
    assert_eq!(out.len(), 1, "{out:?}");
}

#[test]
fn export_default_with_no_binding_yields_a_default_fragment() {
    let src = "export default new Hono().get('/y', h);\n";
    let out = extract_router_mount_fragments("default.ts", src, &[]);
    assert_eq!(
        frag(&out, "default").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/y".into(),
            handler: Some("h".into()),
            line: 1,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn top_level_mount_resolves_import_specifier() {
    let src = concat!(
        "import { auth } from '@example/auth-server';\n",
        "const app = new Hono();\n",
        "app.route('/api/auth', auth);\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api/auth".into(),
            ident: "auth".into(),
            specifier: Some("@example/auth-server".into()),
            attr_keys: vec![],
        }]
    );
}

#[test]
fn configured_router_names_receiver() {
    let src = "apiRoutes.get(\"/health\", healthHandler);\n";
    let out = extract_router_mount_fragments("health.ts", src, &["apiRoutes"]);
    assert_eq!(
        frag(&out, "apiRoutes").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/health".into(),
            handler: Some("healthHandler".into()),
            line: 1,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn hono_typed_parameter_is_a_receiver() {
    let src = concat!(
        "function register(app: Hono, handlers: AuthHandlerShape): void {\n",
        "  app.get('/x', h);\n",
        "}\n"
    );
    let out = extract_router_mount_fragments("register.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
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
fn unrelated_receivers_without_a_structural_signal_are_not_routes() {
    let src = concat!(
        "const cache = new Map();\n",
        "cache.get('key');\n",
        "axios.get('/url', handler);\n"
    );
    let out = extract_router_mount_fragments("cache.ts", src, &[]);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn non_literal_path_skips_only_that_entry_fragment_survives() {
    let src = concat!(
        "const route = new Hono();\n",
        "route.get(SOME_CONST, h).get('/ok', h2);\n"
    );
    let out = extract_router_mount_fragments("route.ts", src, &[]);
    assert_eq!(
        frag(&out, "route").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/ok".into(),
            handler: Some("h2".into()),
            line: 2,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn all_and_use_are_ignored() {
    let src = concat!(
        "const route = new Hono();\n",
        "route.all('/x', h);\n",
        "route.use(mw);\n",
        "route.get('/ok', h2);\n"
    );
    let out = extract_router_mount_fragments("route.ts", src, &[]);
    assert_eq!(
        frag(&out, "route").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/ok".into(),
            handler: Some("h2".into()),
            line: 4,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn hono_single_arg_get_no_longer_emits_a_verb() {
    // Same ≥2-arg guard applied to Hono, proving it is vocabulary-neutral, not Express-only.
    let src = concat!(
        "const route = new Hono();\n",
        "route.get('/x');\n",
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
        }]
    );
}

#[test]
fn hono_use_still_never_mounts_even_with_ident_second_arg() {
    let src = concat!(
        "const app = new Hono();\n",
        "app.use('/path', someMiddleware);\n",
        "app.get('/ok', h);\n"
    );
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
        "Hono's .use must never mint a Mount, vocabulary separation from Express: {out:?}"
    );
}

#[test]
fn deterministic_across_repeated_extractions() {
    let src = concat!(
        "import { sessionRoute } from './routes/session';\n",
        "export const auth = new Hono<HonoAuthContext>()\n",
        "  .use(async (c, next) => { await next(); })\n",
        "  .get('/csrf', csrfHandler)\n",
        "  .route('/', sessionRoute)\n",
        "  .route('/two-factor', twoFactorRoute);\n"
    );
    let a = extract_router_mount_fragments("auth.ts", src, &[]);
    let b = extract_router_mount_fragments("auth.ts", src, &[]);
    assert_eq!(a, b);
}

#[test]
fn empty_file_yields_no_fragments() {
    assert!(extract_router_mount_fragments("e.ts", "", &[]).is_empty());
}
