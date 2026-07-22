//! Coverage for `idempotency::inline_handler_reads_idempotency_key` as exercised through
//! `extract_router_mount_fragments`'s Verb-arm classification: literal-shape variety (string
//! member, computed member, casing, template literal), the inline-only veto for named-reference
//! handlers, no-leak across sibling routes, Hono's chain vocabulary, and co-occurrence with the
//! `auth-guarded` judgment from `guard.rs`.
use super::extract_router_mount_fragments;
use super::guard::AUTH_GUARDED_ATTR_KEY;
use super::idempotency::IDEMPOTENCY_GUARDED_ATTR_KEY;
use super::tests_hono::frag;
use zzop_core::RouterMountEntry;

#[test]
fn express_get_header_call_tags_the_route_idempotency_guarded() {
    let src = concat!(
        "const app = express();\n",
        "app.post('/orders', async (req, res) => {\n",
        "  const k = req.get('Idempotency-Key');\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/orders".into(),
            handler: None,
            line: 2,
            attr_keys: vec![IDEMPOTENCY_GUARDED_ATTR_KEY.to_string()],
        }]
    );
}

#[test]
fn computed_member_lowercase_header_literal_tags_the_route() {
    let src = concat!(
        "const app = express();\n",
        "app.post('/orders', (req, res) => {\n",
        "  const k = req.headers['idempotency-key'];\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/orders".into(),
            handler: None,
            line: 2,
            attr_keys: vec![IDEMPOTENCY_GUARDED_ATTR_KEY.to_string()],
        }]
    );
}

#[test]
fn x_prefixed_casing_variant_literal_tags_the_route() {
    let src = concat!(
        "const app = express();\n",
        "app.post('/orders', (req, res) => {\n",
        "  const k = req.get('X-Idempotency-Key');\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/orders".into(),
            handler: None,
            line: 2,
            attr_keys: vec![IDEMPOTENCY_GUARDED_ATTR_KEY.to_string()],
        }]
    );
}

#[test]
fn named_reference_handler_is_never_tagged_inline_only_v1() {
    // The header literal lives in `createOrder`, defined elsewhere in the same file — v1 is
    // inline-only, never-guess: a named-identifier handler always returns false regardless of
    // what its (separately-defined) body does.
    let src = concat!(
        "const app = express();\n",
        "app.post('/orders', createOrder);\n",
        "function createOrder(req, res) {\n",
        "  req.get('Idempotency-Key');\n",
        "}\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/orders".into(),
            handler: Some("createOrder".into()),
            line: 2,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn unrelated_literal_is_never_tagged() {
    let src = concat!(
        "const app = express();\n",
        "app.post('/orders', (req, res) => {\n",
        "  const t = req.get('Authorization');\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/orders".into(),
            handler: None,
            line: 2,
            attr_keys: vec![],
        }]
    );
}

#[test]
fn only_the_reading_route_is_tagged_no_leak_to_its_sibling() {
    let src = concat!(
        "const router = express.Router();\n",
        "router.post('/orders', (req, res) => {\n",
        "  req.get('Idempotency-Key');\n",
        "});\n",
        "router.post('/preview', (req, res) => {\n",
        "  req.get('Authorization');\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("router.ts", src, &[]);
    assert_eq!(
        frag(&out, "router").entries,
        vec![
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/orders".into(),
                handler: None,
                line: 2,
                attr_keys: vec![IDEMPOTENCY_GUARDED_ATTR_KEY.to_string()],
            },
            RouterMountEntry::Verb {
                method: "POST".into(),
                path: "/preview".into(),
                handler: None,
                line: 5,
                attr_keys: vec![],
            },
        ]
    );
}

#[test]
fn hono_chain_header_read_tags_the_route() {
    let src = concat!(
        "const app = new Hono();\n",
        "app.post('/pay', (c) => {\n",
        "  c.req.header('Idempotency-Key');\n",
        "  return c.text('ok');\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/pay".into(),
            handler: None,
            line: 2,
            attr_keys: vec![IDEMPOTENCY_GUARDED_ATTR_KEY.to_string()],
        }]
    );
}

#[test]
fn no_substitution_template_literal_header_name_tags_the_route() {
    let src = concat!(
        "const app = express();\n",
        "app.post('/orders', (req, res) => {\n",
        "  req.get(`idempotency-key`);\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("app.ts", src, &[]);
    assert_eq!(
        frag(&out, "app").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/orders".into(),
            handler: None,
            line: 2,
            attr_keys: vec![IDEMPOTENCY_GUARDED_ATTR_KEY.to_string()],
        }]
    );
}

#[test]
fn route_level_auth_middleware_and_header_read_both_tag_the_entry() {
    let src = concat!(
        "const router = express.Router();\n",
        "router.post('/orders', requireAuth, (req, res) => {\n",
        "  req.get('Idempotency-Key');\n",
        "});\n"
    );
    let out = extract_router_mount_fragments("router.ts", src, &[]);
    assert_eq!(
        frag(&out, "router").entries,
        vec![RouterMountEntry::Verb {
            method: "POST".into(),
            path: "/orders".into(),
            handler: None,
            line: 2,
            attr_keys: vec![
                AUTH_GUARDED_ATTR_KEY.to_string(),
                IDEMPOTENCY_GUARDED_ATTR_KEY.to_string(),
            ],
        }]
    );
}
