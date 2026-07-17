//! Coverage for `compose_router_mount_provides`: same-file mount join, a 3-hop mount chain across
//! files, `/`-prefix passthrough, root exclusion (mounted child never emitted unprefixed;
//! unresolvable-but-named child skipped wholesale), sole-fragment fallback for default-import
//! aliases, cycle guard, determinism, and producer-judged attribute composition (`attr_keys` /
//! `ScopedAttr` -> `zzop_core::Attribute`).
use super::*;
use zzop_core::{RouterMountEntry, RouterMountFragment};

fn verb(method: &str, path: &str, handler: &str, line: u32) -> RouterMountEntry {
    RouterMountEntry::Verb {
        method: method.to_string(),
        path: path.to_string(),
        handler: Some(handler.to_string()),
        line,
        attr_keys: vec![],
    }
}

fn verb_with_attrs(
    method: &str,
    path: &str,
    handler: &str,
    line: u32,
    attr_keys: Vec<&str>,
) -> RouterMountEntry {
    RouterMountEntry::Verb {
        method: method.to_string(),
        path: path.to_string(),
        handler: Some(handler.to_string()),
        line,
        attr_keys: attr_keys.into_iter().map(str::to_string).collect(),
    }
}

fn mount(prefix: &str, ident: &str, specifier: Option<&str>) -> RouterMountEntry {
    RouterMountEntry::Mount {
        prefix: prefix.to_string(),
        ident: ident.to_string(),
        specifier: specifier.map(str::to_string),
        attr_keys: vec![],
    }
}

fn mount_with_attrs(
    prefix: &str,
    ident: &str,
    specifier: Option<&str>,
    attr_keys: Vec<&str>,
) -> RouterMountEntry {
    RouterMountEntry::Mount {
        prefix: prefix.to_string(),
        ident: ident.to_string(),
        specifier: specifier.map(str::to_string),
        attr_keys: attr_keys.into_iter().map(str::to_string).collect(),
    }
}

fn scoped_attr(prefix: &str, key: &str, line: u32) -> RouterMountEntry {
    RouterMountEntry::ScopedAttr {
        prefix: prefix.to_string(),
        key: key.to_string(),
        line,
    }
}

fn frag(name: &str, entries: Vec<RouterMountEntry>) -> RouterMountFragment {
    RouterMountFragment {
        name: name.to_string(),
        entries,
    }
}

fn no_resolver() -> impl Fn(&str, &str, &str) -> Option<String> {
    |_: &str, _: &str, _: &str| None
}

/// Maps (specifier, from_file) pairs to target rel paths. `ident` (the resolver's 3rd parameter,
/// carrying the mount's own ident — see `compose_router_mount_provides`'s doc for why it exists) is
/// unused by every one of these fixtures: they model TS/Python/Rust-shaped one-file resolution, where
/// disambiguation happens entirely in `candidates_in` AFTER `resolve` returns. The Go-shaped tests
/// below use a bespoke inline resolver instead, since Go's resolution genuinely depends on `ident`.
fn resolver<'a>(
    map: &'a [(&'a str, &'a str, &'a str)],
) -> impl Fn(&str, &str, &str) -> Option<String> + 'a {
    move |spec: &str, from: &str, _ident: &str| {
        map.iter()
            .find(|(s, f, _)| *s == spec && *f == from)
            .map(|(_, _, t)| t.to_string())
    }
}

#[test]
fn same_file_mount_joins_prefix() {
    let (out, _attrs) = compose_router_mount_provides(
        vec![(
            "src/app.ts".to_string(),
            vec![
                frag(
                    "app",
                    vec![
                        verb("GET", "/health", "h", 2),
                        mount("/admin", "adminRouter", None),
                    ],
                ),
                frag("adminRouter", vec![verb("POST", "/users", "createUser", 9)]),
            ],
        )],
        no_resolver(),
    );
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["GET /health", "POST /admin/users"]);
    assert_eq!(out[1].file, "src/app.ts");
    assert_eq!(out[1].line, 9);
    assert_eq!(out[1].symbol.as_deref(), Some("createUser"));
}

#[test]
fn three_hop_mount_chain_composes_full_url() {
    // server/router.ts mounts auth at /api/auth; auth/index.ts mounts twoFactorRoute at
    // /two-factor (plus an inline verb and a "/"-passthrough mount); the leaf file registers
    // POST /setup. Expected: /api/auth/two-factor/setup with the LEAF file:line anchor.
    let fragments = vec![
        (
            "auth/index.ts".to_string(),
            vec![frag(
                "auth",
                vec![
                    verb("GET", "/csrf", "csrfHandler", 21),
                    mount("/", "sessionRoute", Some("./routes/session")),
                    mount("/two-factor", "twoFactorRoute", Some("./routes/two-factor")),
                ],
            )],
        ),
        (
            "auth/routes/session.ts".to_string(),
            vec![frag("sessionRoute", vec![verb("GET", "/session", "s", 5)])],
        ),
        (
            "auth/routes/two-factor.ts".to_string(),
            vec![frag(
                "twoFactorRoute",
                vec![verb("POST", "/setup", "setup", 20)],
            )],
        ),
        (
            "server/router.ts".to_string(),
            vec![frag(
                "app",
                vec![mount("/api/auth", "auth", Some("@example/auth-server"))],
            )],
        ),
    ];
    let (out, _attrs) = compose_router_mount_provides(
        fragments,
        resolver(&[
            (
                "./routes/session",
                "auth/index.ts",
                "auth/routes/session.ts",
            ),
            (
                "./routes/two-factor",
                "auth/index.ts",
                "auth/routes/two-factor.ts",
            ),
            ("@example/auth-server", "server/router.ts", "auth/index.ts"),
        ]),
    );
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "GET /api/auth/csrf",
            "GET /api/auth/session",
            "POST /api/auth/two-factor/setup",
        ]
    );
    assert_eq!(out[2].file, "auth/routes/two-factor.ts");
    assert_eq!(out[2].line, 20);
}

#[test]
fn mounted_child_is_never_emitted_unprefixed_even_when_unresolvable() {
    // `admin` is mounted by name from a file the resolver cannot link — the child fragment
    // must NOT surface `/users` with the missing `/admin` prefix (conservative root
    // exclusion, mirroring compose_trpc_provides).
    let fragments = vec![
        (
            "src/app.ts".to_string(),
            vec![frag(
                "app",
                vec![mount("/admin", "admin", Some("./nowhere"))],
            )],
        ),
        (
            "src/admin.ts".to_string(),
            vec![frag("admin", vec![verb("GET", "/users", "h", 3)])],
        ),
    ];
    let (out, _attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert!(out.is_empty());
}

#[test]
fn sole_fragment_fallback_covers_default_import_alias() {
    // `export default route` re-imported as `pdfRoute` — no name match in the target file,
    // but it holds exactly one fragment, so the mount resolves to it.
    let fragments = vec![
        (
            "server/files.ts".to_string(),
            vec![frag(
                "filesRoute",
                vec![mount("/", "pdfRoute", Some("./routes/pdf"))],
            )],
        ),
        (
            "server/routes/pdf.ts".to_string(),
            vec![frag(
                "route",
                vec![verb("GET", "/envelope/:id/item.pdf", "h", 4)],
            )],
        ),
    ];
    let (out, _attrs) = compose_router_mount_provides(
        fragments,
        resolver(&[("./routes/pdf", "server/files.ts", "server/routes/pdf.ts")]),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "GET /envelope/{}/item.pdf");
}

#[test]
fn mount_cycle_is_guarded() {
    let fragments = vec![(
        "src/a.ts".to_string(),
        vec![
            frag("a", vec![verb("GET", "/x", "h", 1), mount("/b", "b", None)]),
            frag("b", vec![mount("/a", "a", None)]),
        ],
    )];
    let (out, _attrs) = compose_router_mount_provides(fragments, no_resolver());
    // `a` and `b` mount each other, so neither is a root — conservative empty output rather
    // than an infinite walk or a truncated-prefix guess.
    assert!(out.is_empty());
}

#[test]
fn output_is_deterministic_across_input_order() {
    let build = |rev: bool| {
        let mut v = vec![
            (
                "src/app.ts".to_string(),
                vec![frag(
                    "app",
                    vec![mount("/api", "sub", None), verb("GET", "/", "root", 1)],
                )],
            ),
            (
                "src/app.ts".to_string(),
                vec![frag("sub", vec![verb("POST", "/items", "create", 8)])],
            ),
        ];
        if rev {
            v.reverse();
        }
        v
    };
    let (a, _) = compose_router_mount_provides(build(false), no_resolver());
    let (b, _) = compose_router_mount_provides(build(true), no_resolver());
    let view = |v: &[IoProvide]| -> Vec<(String, String, u32)> {
        v.iter()
            .map(|p| (p.key.clone(), p.file.clone(), p.line))
            .collect()
    };
    assert_eq!(view(&a), view(&b));
}

// --- attribute composition ---

#[test]
fn verb_attr_key_composes_to_an_iokey_attribute_matching_the_provide_key() {
    let fragments = vec![(
        "src/app.ts".to_string(),
        vec![
            frag("app", vec![mount("/admin", "adminRouter", None)]),
            frag(
                "adminRouter",
                vec![verb_with_attrs(
                    "POST",
                    "/widgets",
                    "createWidget",
                    4,
                    vec!["auth-guarded"],
                )],
            ),
        ],
    )];
    let (out, attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "POST /admin/widgets");
    assert_eq!(attrs.len(), 1);
    assert_eq!(
        attrs[0].target,
        zzop_core::EntityRef::IoKey {
            kind: "http".to_string(),
            key: out[0].key.clone(),
        }
    );
    assert_eq!(attrs[0].key, "auth-guarded");
    assert_eq!(attrs[0].value, serde_json::json!(true));
}

#[test]
fn resolved_mount_with_attr_keys_emits_no_attribute() {
    // The mount resolves to a real sub-router fragment — attr_keys on a RESOLVED mount are
    // ignored (the ident was a router, not a middleware guard).
    let fragments = vec![(
        "src/app.ts".to_string(),
        vec![
            frag(
                "app",
                vec![mount_with_attrs(
                    "/admin",
                    "adminRouter",
                    None,
                    vec!["auth-guarded"],
                )],
            ),
            frag("adminRouter", vec![verb("GET", "/x", "h", 1)]),
        ],
    )];
    let (out, attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert_eq!(out.len(), 1);
    assert!(attrs.is_empty(), "{attrs:?}");
}

#[test]
fn unresolved_mount_with_attr_keys_emits_a_pathscope_at_the_composed_prefix() {
    // `requireAuth` never resolves to any fragment: its specifier resolution fails (`no_resolver`),
    // and — unlike a `None`-specifier mount, where a single-fragment file's sole-fragment fallback
    // would otherwise resolve it — this mount's own attr_keys resolve as a PathScope at the
    // composed prefix instead of being silently dropped.
    let fragments = vec![(
        "src/app.ts".to_string(),
        vec![frag(
            "app",
            vec![mount_with_attrs(
                "/admin",
                "requireAuth",
                Some("./middleware/require-auth"),
                vec!["auth-guarded"],
            )],
        )],
    )];
    let (out, attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert!(out.is_empty());
    assert_eq!(attrs.len(), 1);
    assert_eq!(
        attrs[0].target,
        zzop_core::EntityRef::PathScope {
            prefix: "/admin".to_string(),
        }
    );
    assert_eq!(attrs[0].key, "auth-guarded");
}

#[test]
fn scoped_attr_on_a_sub_router_mounted_at_prefix_composes_the_full_pathscope() {
    let fragments = vec![(
        "src/app.ts".to_string(),
        vec![
            frag("app", vec![mount("/api", "apiRouter", None)]),
            frag(
                "apiRouter",
                vec![
                    scoped_attr("/", "auth-guarded", 1),
                    verb("POST", "/widgets", "h", 2),
                ],
            ),
        ],
    )];
    let (out, attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert_eq!(out.len(), 1);
    assert_eq!(attrs.len(), 1);
    assert_eq!(
        attrs[0].target,
        zzop_core::EntityRef::PathScope {
            prefix: "/api".to_string(),
        }
    );
    assert_eq!(attrs[0].key, "auth-guarded");
}

#[test]
fn scoped_attr_under_a_param_prefix_normalizes_to_curly_braces_and_covers_the_route() {
    // `router.use('/users/:id/admin', requireAuth())` composes a ScopedAttr whose raw prefix
    // carries `:id` — fix 2 normalizes it the same way `http_interface_key` normalizes route
    // paths, so it actually covers `http_interface_key`-keyed routes under it (which carry `{}`,
    // never the raw `:id` spelling).
    let fragments = vec![(
        "src/app.ts".to_string(),
        vec![frag(
            "app",
            vec![
                scoped_attr("/users/:id/admin", "auth-guarded", 1),
                verb("POST", "/users/:id/admin/ban", "banUser", 2),
            ],
        )],
    )];
    let (out, attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert_eq!(out[0].key, "POST /users/{}/admin/ban");
    assert_eq!(attrs.len(), 1);
    assert_eq!(
        attrs[0].target,
        zzop_core::EntityRef::PathScope {
            prefix: "/users/{}/admin".to_string(),
        },
        "the emitted PathScope prefix must be normalized, not raw :id: {attrs:?}"
    );

    // End-to-end: the normalized PathScope must actually exempt the normalized route key via
    // AttributeStore::route_attr — the consumer-side lookup `mutating_route_no_auth` uses.
    let store = zzop_core::AttributeStore::from_attrs(attrs);
    assert_eq!(
        store.route_attr("http", &out[0].key, "auth-guarded"),
        Some(&serde_json::json!(true)),
        "a normalized PathScope prefix must exempt the normalized route under it"
    );
}

#[test]
fn unresolved_mount_with_param_prefix_normalizes_to_curly_braces() {
    let fragments = vec![(
        "src/app.ts".to_string(),
        vec![frag(
            "app",
            vec![mount_with_attrs(
                "/users/:id/admin",
                "requireAuth",
                Some("./middleware/require-auth"),
                vec!["auth-guarded"],
            )],
        )],
    )];
    let (out, attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert!(out.is_empty());
    assert_eq!(attrs.len(), 1);
    assert_eq!(
        attrs[0].target,
        zzop_core::EntityRef::PathScope {
            prefix: "/users/{}/admin".to_string(),
        },
        "the unresolved-Mount PathScope prefix must be normalized, not raw :id: {attrs:?}"
    );
}

#[test]
fn root_excluded_fragment_emits_no_attribute() {
    // `admin` is mounted-by-name from an unresolvable specifier — the same root-exclusion
    // conservatism that keeps its provides unemitted also keeps its ScopedAttr silent.
    let fragments = vec![
        (
            "src/app.ts".to_string(),
            vec![frag(
                "app",
                vec![mount("/admin", "admin", Some("./nowhere"))],
            )],
        ),
        (
            "src/admin.ts".to_string(),
            vec![frag(
                "admin",
                vec![
                    scoped_attr("/", "auth-guarded", 1),
                    verb("GET", "/x", "h", 2),
                ],
            )],
        ),
    ];
    let (out, attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert!(out.is_empty());
    assert!(attrs.is_empty(), "{attrs:?}");
}

#[test]
fn attribute_composition_is_deterministic_across_repeated_calls() {
    let fragments = vec![(
        "src/app.ts".to_string(),
        vec![
            frag("app", vec![mount("/admin", "adminRouter", None)]),
            frag(
                "adminRouter",
                vec![
                    scoped_attr("/", "auth-guarded", 1),
                    verb_with_attrs("POST", "/x", "h", 2, vec!["auth-guarded"]),
                ],
            ),
        ],
    )];
    let (_out_a, attrs_a) = compose_router_mount_provides(fragments.clone(), no_resolver());
    let (_out_b, attrs_b) = compose_router_mount_provides(fragments, no_resolver());
    assert_eq!(attrs_a, attrs_b);
    assert_eq!(attrs_a.len(), 2, "{attrs_a:?}");
}

// --- Go (gin) mounts: a specifier is a Go IMPORT PATH resolving to a PACKAGE DIRECTORY (many
// candidate files), so disambiguation genuinely needs `ident` — unlike the TS/Python/Rust fixtures
// above, which resolve `specifier` to a single file and never touch the resolver's 3rd parameter.

#[test]
fn go_mount_resolves_via_specifier_and_ident_together() {
    // Mirrors the real Go resolver built in `assemble::provides`: `specifier` ("app/users") alone
    // does not name one file — it names a package directory that could hold several router-mount
    // fragments, so the mock resolver here only succeeds when BOTH the specifier/from_file pair AND
    // the mount's own `ident` ("UsersRegister") line up with the def-side fragment, exactly like the
    // real engine's `resolve_go_import_package_dir` + fragment-name search.
    let fragments = vec![
        (
            "cmd/main.go".to_string(),
            vec![frag(
                "main",
                vec![mount("/users", "UsersRegister", Some("app/users"))],
            )],
        ),
        (
            "users/routers.go".to_string(),
            vec![frag(
                "UsersRegister",
                vec![verb("GET", "/list", "listUsers", 12)],
            )],
        ),
    ];
    let go_resolver = |spec: &str, from: &str, ident: &str| -> Option<String> {
        if spec == "app/users" && from == "cmd/main.go" && ident == "UsersRegister" {
            Some("users/routers.go".to_string())
        } else {
            None
        }
    };
    let (out, _attrs) = compose_router_mount_provides(fragments, go_resolver);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "GET /users/list");
    assert_eq!(out[0].file, "users/routers.go");
    assert_eq!(out[0].line, 12);
    assert_eq!(out[0].symbol.as_deref(), Some("listUsers"));
}

#[test]
fn go_mount_with_unresolvable_specifier_stays_conservative() {
    // The import path names no package the resolver recognizes (e.g. an external module, or a
    // `go.mod`-less tree) — same conservative "skip the subtree" behavior every other language's
    // unresolvable-mount case gets, never a truncated-prefix guess.
    let fragments = vec![(
        "cmd/main.go".to_string(),
        vec![frag(
            "main",
            vec![mount("/users", "UsersRegister", Some("app/unknown"))],
        )],
    )];
    let (out, _attrs) = compose_router_mount_provides(fragments, no_resolver());
    assert!(out.is_empty());
}
