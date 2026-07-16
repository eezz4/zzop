//! Express-vocabulary half of the coverage listed in `tests_hono.rs`. Middleware guard-name
//! judgment (express-middleware-v1) coverage lives in `guard_tests.rs`.
use super::extract_router_mount_fragments;
use super::tests_hono::frag;
use zzop_core::RouterMountEntry;

#[test]
fn express_router_use_mounts_a_sub_router() {
    let src = concat!(
        "const app = express();\n",
        "const router = express.Router();\n",
        "router.get('/users', listUsers);\n",
        "router.post('/users', createUser);\n",
        "app.use('/api', router);\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "router").entries,
        vec![
            RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/users".into(),
                handler: Some("listUsers".into()),
                line: 3,
                attr_keys: vec![],
            },
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/users".into(),
                handler: Some("createUser".into()),
                line: 4,
                attr_keys: vec![],
            },
        ]
    );
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api".into(),
            ident: "router".into(),
            specifier: None,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn express_use_mount_resolves_cross_file_specifier() {
    let src = concat!(
        "import { usersRouter } from './users';\n",
        "const app = express();\n",
        "app.use('/api/users', usersRouter);\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api/users".into(),
            ident: "usersRouter".into(),
            specifier: Some("./users".into()),
            attr_keys: vec![],
        }]
    );
}

#[test]
fn express_single_arg_call_is_a_config_getter_not_a_route() {
    let src = concat!(
        "const app = express();\n",
        "app.get('view engine');\n",
        "app.get('/health', h);\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/health".into(),
            handler: Some("h".into()),
            line: 3,
            attr_keys: vec![],
        }],
        "the single-arg 'view engine' getter must not be a Verb: {out:?}"
    );
}

#[test]
fn express_router_chained_builder() {
    let src = "const r = express.Router().get('/a', h);\n";
    let out = extract_router_mount_fragments("r.ts", src, &[]);
    assert_eq!(
        frag(&out, "r").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/a".into(),
            handler: Some("h".into()),
            line: 1,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn bare_router_get_with_no_qualifying_binding_is_not_recognized() {
    let src = "router.get('/a', h);\n";
    let out = extract_router_mount_fragments("r.ts", src, &[]);
    assert!(out.is_empty(), "no vocabulary leak: {out:?}");
}

// --- Express named-import `Router()` vocabulary (dogfood round 9 gaps A/B/C) ---

#[test]
fn named_import_router_controller_recognizes_verbs_with_middleware_arg() {
    // The gothinkster node-express-realworld controller idiom: a bare `Router()` receiver
    // from a named `import { Router } from 'express'`, verb registrations carrying a
    // middleware argument between the path and the handler.
    let src = concat!(
        "import { Router } from 'express';\n",
        "const router = Router();\n",
        "router.get('/articles', auth.optional, listArticles);\n",
        "router.post('/articles', auth.required, createArticle);\n",
        "export default router;\n"
    );
    let out = extract_router_mount_fragments("articleController.ts", src, &[]);
    assert_eq!(
        frag(&out, "router").entries,
        vec![
            RouterMountEntry::Verb {
                method: "GET".into(),
                path: "/articles".into(),
                handler: Some("listArticles".into()),
                line: 3,
                attr_keys: vec![],
            },
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/articles".into(),
                handler: Some("createArticle".into()),
                line: 4,
                attr_keys: vec![],
            },
        ]
    );
    assert!(
        out.iter().all(|f| f.name != "default"),
        "named binding must win over a synthesized \"default\" fragment: {out:?}"
    );
}

#[test]
fn aliased_router_import_is_recognized_bare_router_without_import_is_not() {
    let aliased = concat!(
        "import { Router as R } from 'express';\n",
        "const r = R();\n",
        "r.get('/x', h);\n"
    );
    let out = extract_router_mount_fragments("r.ts", aliased, &[]);
    assert_eq!(
        frag(&out, "r").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/x".into(),
            handler: Some("h".into()),
            line: 3,
            attr_keys: vec![],
        }],
        "an aliased `Router as R` import must still gate `R()` as an Express receiver: {out:?}"
    );

    let bare = "const router = Router();\nrouter.get('/x', h);\n";
    let out2 = extract_router_mount_fragments("r.ts", bare, &[]);
    assert!(
        out2.is_empty(),
        "a bare `Router()` with no express import must not be recognized: {out2:?}"
    );
}

#[test]
fn routes_aggregation_single_arg_use_mounts_at_root_default_export_mounts_prefix() {
    // The RealWorld `routes.ts` aggregation shape: `Router()` as a chain root bound to a var
    // (`.use(ident)` mounts each controller at "/"), and `Router()` as a chain root under
    // `export default` (`.use(prefixLit, ident)` mounts the aggregated router under a prefix).
    let src = concat!(
        "import { Router } from 'express';\n",
        "import a from './controllers/a';\n",
        "import b from './controllers/b';\n",
        "const api = Router().use(a).use(b);\n",
        "export default Router().use('/api', api);\n"
    );
    let out = extract_router_mount_fragments("routes.ts", src, &[]);
    assert_eq!(
        frag(&out, "api").entries,
        vec![
            RouterMountEntry::Mount {
                prefix: "/".into(),
                ident: "a".into(),
                specifier: Some("./controllers/a".into()),
                attr_keys: vec![],
            },
            RouterMountEntry::Mount {
                prefix: "/".into(),
                ident: "b".into(),
                specifier: Some("./controllers/b".into()),
                attr_keys: vec![],
            },
        ]
    );
    assert_eq!(
        frag(&out, "default").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api".into(),
            ident: "api".into(),
            specifier: None,
            attr_keys: vec![],
        }],
        "export default Router().use('/api', api) must mount \"api\" under \"/api\": {out:?}"
    );
}

#[test]
fn non_identifier_single_arg_use_calls_are_skipped_not_mistaken_for_mounts() {
    let src = concat!(
        "import { Router } from 'express';\n",
        "import express from 'express';\n",
        "import cors from 'cors';\n",
        "import bodyParser from 'body-parser';\n",
        "const app = Router();\n",
        "app.use(cors());\n",
        "app.use(bodyParser.json());\n",
        "app.use(express.static('/public'));\n",
        "app.get('/ok', h);\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/ok".into(),
            handler: Some("h".into()),
            line: 9,
            attr_keys: vec![],
        }],
        "cors()/bodyParser.json()/express.static(...) must not mint a Mount: {out:?}"
    );
}
