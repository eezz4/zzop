//! Express `.use(...)` mounts whose ROUTER ARGUMENT is an inline `require(...)` call — the CommonJS
//! half of the sub-router idiom. Split out of `tests_express.rs` on 2026-08-19 when adding these
//! pushed that file past the repo line cap; the seam is the argument shape, which is what the
//! recognizer distinguishes.
use super::extract_router_mount_fragments;
use super::tests_hono::frag;
use zzop_core::RouterMountEntry;
/// The CommonJS spelling of the test above, and the one that used to fall through to `Vec::new()`:
/// the mounted router arrives as an inline `require(...)` CALL rather than an identifier, so the
/// classifier judged it for guard vocabulary, got "no", and dropped the mount. `expressjs/express`
/// ships exactly this in `examples/multi-router/index.js`.
///
/// Losing the mount is not merely a missing fact — the child's routes then compose at their OWN
/// keys, so two routers mounted at two prefixes both emit `GET /` and collide into a false
/// `duplicate-route`. Measured on a 3-file fixture before the fix: keys were `GET /` and
/// `GET /users`; after, `GET /api/v1` and `GET /api/v1/users`.
#[test]
fn express_use_mount_accepts_an_inline_require_call() {
    let src = concat!(
        "const app = express();\n",
        "app.use('/api/v1', require('./controllers/v1'));\n"
    );
    let out = extract_router_mount_fragments("index.js", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api/v1".into(),
            // No binding exists, so the composer's by-name lookup gets the name the author WOULD
            // have bound — and falls back to "the file declares exactly one router" when it misses.
            ident: "v1".into(),
            specifier: Some("./controllers/v1".into()),
            attr_keys: vec![],
        }]
    );
}

/// The prefix-less aggregation form of the same idiom, plus the two shapes that must NOT become
/// mounts: a dynamic specifier this pass cannot read, and an ordinary middleware call. All three
/// ride one fixture so a widening that swallows the middleware fails here rather than in a corpus.
#[test]
fn inline_require_mount_is_narrow_and_leaves_middleware_alone() {
    let src = concat!(
        "const app = express();\n",
        "app.use(require('./routes'));\n",
        "app.use(require(dynamicName));\n",
        "app.use(cors());\n"
    );
    let out = extract_router_mount_fragments("index.js", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/".into(),
            ident: "routes".into(),
            specifier: Some("./routes".into()),
            attr_keys: vec![],
        }],
        "only the literal require mounts; a dynamic specifier and cors() must stay skipped"
    );
}

/// `./controllers/index` names the directory, which is what a reader would have bound it to —
/// `index` as an ident would collide across every such module in the tree.
#[test]
fn inline_require_of_an_index_module_takes_the_directory_name() {
    let src = concat!(
        "const app = express();\n",
        "app.use('/admin', require('./admin/index.js'));\n"
    );
    let out = extract_router_mount_fragments("index.js", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/admin".into(),
            ident: "admin".into(),
            specifier: Some("./admin/index.js".into()),
            attr_keys: vec![],
        }]
    );
}
