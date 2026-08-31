//! Receiver RECOGNITION (pass 1) for Express vocabulary — which identifiers become an app, and which
//! deliberately do not. Split out of `tests_express.rs` when that file crossed the 300-line cap, and
//! the axis differs too: those tests ask what a KNOWN receiver registers, these ask what becomes a
//! receiver at all.
use super::extract_router_mount_fragments;
use super::tests_hono::frag;
use zzop_core::RouterMountEntry;

#[test]
fn express_app_arriving_as_a_typed_parameter_is_a_receiver() {
    // A receiver the file does not CONSTRUCT is knowable only from its type, and until 2026-08-16
    // pass 1 knew construction plus a Hono annotation — so mainstream `registerRoutes(app: Express)`
    // registered nothing. Measured on a 14-route tree, 5 routes vanished and the run did not go quiet:
    // three of them came back as "no matching provide" and the mutating census read 4 of a true 7.
    let src = concat!(
        "import { Express } from 'express';\n",
        "export function registerReportRoutes(app: Express) {\n",
        "  app.get('/api/reports/monthly', h);\n",
        "  app.post('/api/reports/export', h);\n",
        "}\n"
    );
    let out = extract_router_mount_fragments("reports.ts", src, &[]);
    let entries = &frag(&out, "app").entries;
    let mut seen: Vec<String> = entries
        .iter()
        .map(|e| match e {
            RouterMountEntry::Verb { method, path, .. } => format!("{method} {path}"),
            other => panic!("expected Verb, got {other:?}"),
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        vec!["GET /api/reports/monthly", "POST /api/reports/export"]
    );
}

#[test]
fn express_application_type_is_the_same_receiver_and_router_type_is_not() {
    // `Application` is express's own name for the app, so it earns the same admission. A bare `Router`
    // does not: vue-router and react-router spell their type that way too, and a wrong receiver mints
    // wrong routes. Asserted as a pair so a later widening cannot take the refused half along.
    let admitted = concat!(
        "export function mountAdmin(app: Application) {\n",
        "  app.delete('/api/admin/purge', h);\n",
        "}\n"
    );
    let out = extract_router_mount_fragments("admin.ts", admitted, &[]);
    assert_eq!(frag(&out, "app").entries.len(), 1);

    let refused = concat!(
        "export function mountAdmin(r: Router) {\n",
        "  r.delete('/api/admin/purge', h);\n",
        "}\n"
    );
    let out = extract_router_mount_fragments("admin.ts", refused, &[]);
    assert!(
        out.iter().all(|f| f.name != "r"),
        "a bare `Router` type annotation must not register a receiver: {out:?}"
    );
}

#[test]
fn a_commonjs_export_assignment_chain_still_binds_the_receiver_and_a_compound_one_does_not() {
    // `expressjs/express`'s own `examples/multi-router/index.js` writes
    // `var app = module.exports = express();` — build the app and export it in one statement. Until
    // 2026-08-21 the initializer was an ASSIGNMENT rather than a call, so `app` never became a
    // receiver, and losing the receiver lost every mount registered on it: that example's two
    // routers, mounted at `/api/v1` and `/api/v2`, both composed at their own keys and collided
    // into a false `duplicate-route` for `GET /` and `GET /users`. Measured after: 5 routes at
    // `GET /`, `GET /api/v1`, `GET /api/v1/users`, `GET /api/v2`, `GET /api/v2/users`.
    //
    // The narrowing rides the same fixture: a COMPOUND assignment does not mean the binding holds
    // that value, so `lazyApp` must NOT become a receiver and its registration must not appear.
    let src = concat!(
        "var express = require('../..');\n",
        "var app = module.exports = express();\n",
        "app.get('/', h);\n",
        "var lazyApp;\n",
        "lazyApp ||= express();\n",
        "lazyApp.get('/never', h);\n"
    );
    let out = extract_router_mount_fragments("index.js", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "GET".into(),
            path: "/".into(),
            handler: Some("h".into()),
            line: 3,
            attr_keys: vec![],
        }]
    );
    assert!(
        out.iter().all(|f| f.name != "lazyApp"),
        "a compound assignment must not mint a receiver: {out:?}"
    );
}

#[test]
fn a_commonjs_export_assignment_written_as_a_statement_still_reaches_its_mount() {
    // The same `=` arm is reached by a THIRD caller, `FragmentBuilder::visit_expr_stmt`, and the
    // shape it sees is not the one that motivated the arm: `module.exports = app.use(...)` as a bare
    // statement, which is how a CommonJS router file exports an app it mounted onto in one line.
    // Walking it used to stop at `Other` -- an assignment is not a call and not an identifier -- so
    // the receiver went unrecognised and the `.use` mount on it was dropped, exactly the loss the
    // var-decl case measured, one statement form over. Pinned separately because the two callers
    // reach the arm by different paths, and a comment asserting this was cheaper to write than to
    // verify.
    let src = concat!(
        "var express = require('express');\n",
        "var api = express.Router();\n",
        "var app = express();\n",
        "module.exports = app.use('/api', api);\n"
    );
    let out = extract_router_mount_fragments("index.js", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api".into(),
            ident: "api".into(),
            specifier: None,
            attr_keys: vec![],
        }],
        "the mount registered inside a `module.exports =` statement must survive: {out:?}"
    );
}
