use super::*;
use crate::adapters::extract_go_router_fragments;

fn frag<'a>(fragments: &'a [RouterMountFragment], name: &str) -> &'a RouterMountFragment {
    fragments
        .iter()
        .find(|f| f.name == name)
        .expect("fragment present")
}

#[test]
fn verb_chain_on_engine_receiver() {
    let src = "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.Default()\n\tr.GET(\"/users\", listUsers)\n\tr.POST(\"/users\", createUser)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    let f = frag(&frags, "r");
    assert_eq!(f.entries.len(), 2);
    match &f.entries[0] {
        RouterMountEntry::Verb {
            method,
            path,
            handler,
            ..
        } => {
            assert_eq!(method, "GET");
            assert_eq!(path, "/users");
            assert_eq!(handler.as_deref(), Some("listUsers"));
        }
        _ => panic!("expected Verb"),
    }
}

#[test]
fn gin_new_binding_is_recognized_too() {
    let src = "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.New()\n\tr.DELETE(\"/users/:id\", deleteUser)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    let f = frag(&frags, "r");
    assert_eq!(f.entries.len(), 1);
}

#[test]
fn group_mount_and_group_verbs_compose() {
    let src = "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.Default()\n\tapi := r.Group(\"/api\")\n\tapi.GET(\"/users\", listUsers)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    let r = frag(&frags, "r");
    assert_eq!(r.entries.len(), 1);
    match &r.entries[0] {
        RouterMountEntry::Mount { prefix, ident, .. } => {
            assert_eq!(prefix, "/api");
            assert_eq!(ident, "api");
        }
        _ => panic!("expected Mount"),
    }
    let api = frag(&frags, "api");
    assert_eq!(api.entries.len(), 1);
    match &api.entries[0] {
        RouterMountEntry::Verb { method, path, .. } => {
            assert_eq!(method, "GET");
            assert_eq!(path, "/users");
        }
        _ => panic!("expected Verb"),
    }
}

#[test]
fn nested_group_chains_resolve_in_source_order() {
    let src = "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.Default()\n\tapi := r.Group(\"/api\")\n\tv1 := api.Group(\"/v1\")\n\tv1.GET(\"/ping\", ping)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.iter().any(|f| f.name == "v1"));
    let v1 = frag(&frags, "v1");
    assert_eq!(v1.entries.len(), 1);
}

#[test]
fn non_literal_group_prefix_is_skipped() {
    let src = "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.Default()\n\tprefix := \"/api\"\n\tapi := r.Group(prefix)\n\tapi.GET(\"/users\", h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.iter().all(|f| f.name != "api"));
    assert!(frags.iter().all(|f| f.name != "r")); // no surviving entries on r either
}

#[test]
fn verb_vocabulary_pinned_to_http_key_verbs() {
    assert_eq!(GIN_VERB_METHODS, zzop_core::HTTP_KEY_VERBS);
}

#[test]
fn no_import_gate_negative() {
    let src = "package main\n\nfunc main() {\n\tr := gin.Default()\n\tr.GET(\"/users\", h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.is_empty());
}

#[test]
fn nested_call_site_inside_if_is_reachable() {
    let src = "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc setup(enabled bool) {\n\tr := gin.Default()\n\tif enabled {\n\t\tr.GET(\"/users\", h)\n\t}\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    let r = frag(&frags, "r");
    assert_eq!(r.entries.len(), 1);
}

// ---------------------------------------------------------------------------------------------
// Cross-file gin group-param gap (gin-group-param-v1) — ground-truth fixture mirrors the blind
// field test on gothinkster/golang-gin-realworld-example-app EXACTLY (two conceptual files; parser
// tests are per-file, so the call side and the definition side are each their own test below).
// ---------------------------------------------------------------------------------------------

#[test]
fn call_side_ground_truth_users_register_mount() {
    // main.go: r := gin.Default(); v1 := r.Group("/api"); users.UsersRegister(v1.Group("/users"))
    let src = concat!(
        "package main\n\n",
        "import (\n",
        "\t\"github.com/gin-gonic/gin\"\n",
        "\t\"example.com/app/users\"\n",
        ")\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tv1 := r.Group(\"/api\")\n",
        "\tusers.UsersRegister(v1.Group(\"/users\"))\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("main.go", src);

    let r = frag(&frags, "r");
    assert_eq!(r.entries.len(), 1);
    match &r.entries[0] {
        RouterMountEntry::Mount {
            prefix,
            ident,
            specifier,
            ..
        } => {
            assert_eq!(prefix, "/api");
            assert_eq!(ident, "v1");
            assert_eq!(specifier, &None);
        }
        other => panic!("expected Mount, got {other:?}"),
    }

    let v1 = frag(&frags, "v1");
    assert_eq!(v1.entries.len(), 1);
    match &v1.entries[0] {
        RouterMountEntry::Mount {
            prefix,
            ident,
            specifier,
            attr_keys,
        } => {
            assert_eq!(prefix, "/users");
            assert_eq!(ident, "UsersRegister");
            assert_eq!(specifier.as_deref(), Some("example.com/app/users"));
            assert!(attr_keys.is_empty());
        }
        other => panic!("expected Mount, got {other:?}"),
    }
}

#[test]
fn def_side_ground_truth_users_register_fragment() {
    // users/routers.go: func UsersRegister(router *gin.RouterGroup) { router.POST(...); ... }
    let src = concat!(
        "package users\n\n",
        "import \"github.com/gin-gonic/gin\"\n\n",
        "func UsersRegister(router *gin.RouterGroup) {\n",
        "\trouter.POST(\"\", UsersRegistration)\n",
        "\trouter.POST(\"/login\", Login)\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("users/routers.go", src);

    // The receiver's own param name ("router") is NOT a fragment — only the enclosing function's
    // name is (module doc's function-parameter receivers section).
    assert!(frags.iter().all(|f| f.name != "router"));

    let f = frag(&frags, "UsersRegister");
    assert_eq!(f.entries.len(), 2);
    match &f.entries[0] {
        RouterMountEntry::Verb {
            method,
            path,
            handler,
            ..
        } => {
            assert_eq!(method, "POST");
            assert_eq!(path, "");
            assert_eq!(handler.as_deref(), Some("UsersRegistration"));
        }
        other => panic!("expected Verb, got {other:?}"),
    }
    match &f.entries[1] {
        RouterMountEntry::Verb {
            method,
            path,
            handler,
            ..
        } => {
            assert_eq!(method, "POST");
            assert_eq!(path, "/login");
            assert_eq!(handler.as_deref(), Some("Login"));
        }
        other => panic!("expected Verb, got {other:?}"),
    }
}

#[test]
fn call_site_bare_receiver_pass_has_empty_prefix() {
    let src = concat!(
        "package main\n\n",
        "import (\n",
        "\t\"github.com/gin-gonic/gin\"\n",
        "\t\"example.com/app/users\"\n",
        ")\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tusers.Routes(r)\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("main.go", src);
    let r = frag(&frags, "r");
    assert_eq!(r.entries.len(), 1);
    match &r.entries[0] {
        RouterMountEntry::Mount {
            prefix,
            ident,
            specifier,
            ..
        } => {
            assert_eq!(prefix, "");
            assert_eq!(ident, "Routes");
            assert_eq!(specifier.as_deref(), Some("example.com/app/users"));
        }
        other => panic!("expected Mount, got {other:?}"),
    }
}

#[test]
fn call_site_non_literal_group_prefix_is_skipped() {
    let src = concat!(
        "package main\n\n",
        "import (\n",
        "\t\"github.com/gin-gonic/gin\"\n",
        "\t\"example.com/app/users\"\n",
        ")\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tv1 := r.Group(\"/api\")\n",
        "\tprefix := \"/users\"\n",
        "\tusers.UsersRegister(v1.Group(prefix))\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("main.go", src);
    // v1 has no verb/mount entries of its own (the whole call was skipped) — no fragment at all.
    assert!(frags.iter().all(|f| f.name != "v1"));
    // r's own Group-binding Mount to v1 is untouched by the skip.
    let r = frag(&frags, "r");
    assert_eq!(r.entries.len(), 1);
}

#[test]
fn call_site_local_variable_callee_operand_is_skipped() {
    // `db` is a local variable (bound from a bare, non-gin call), not an imported package — the
    // callee operand does not resolve in the ImportMap, so `db.Routes(v1)` is never a candidate.
    let src = concat!(
        "package main\n\n",
        "import \"github.com/gin-gonic/gin\"\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tv1 := r.Group(\"/api\")\n",
        "\tdb := someHelper()\n",
        "\tdb.Routes(v1)\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("main.go", src);
    // v1 never receives a "Routes" mount entry — the call was skipped outright.
    assert!(frags.iter().all(|f| f.name != "v1"));
    let r = frag(&frags, "r");
    assert_eq!(r.entries.len(), 1); // only the r -> v1 Group mount, nothing from db.Routes(v1)
}

#[test]
fn engine_param_variant_registers_function_named_fragment() {
    let src = concat!(
        "package main\n\n",
        "import \"github.com/gin-gonic/gin\"\n\n",
        "func SetupRoutes(engine *gin.Engine) {\n",
        "\tengine.GET(\"/health\", health)\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.iter().all(|f| f.name != "engine"));
    let f = frag(&frags, "SetupRoutes");
    assert_eq!(f.entries.len(), 1);
    match &f.entries[0] {
        RouterMountEntry::Verb {
            method,
            path,
            handler,
            ..
        } => {
            assert_eq!(method, "GET");
            assert_eq!(path, "/health");
            assert_eq!(handler.as_deref(), Some("health"));
        }
        other => panic!("expected Verb, got {other:?}"),
    }
}

#[test]
fn same_param_name_in_two_functions_is_scoped_independently() {
    // Regression guard for the `known` restore mechanics in `Collector::run`'s
    // "function_declaration" arm: two sibling functions reusing the SAME parameter name must not
    // bleed into each other's fragment, and neither registration may survive past its own function.
    let src = concat!(
        "package main\n\n",
        "import \"github.com/gin-gonic/gin\"\n\n",
        "func A(r *gin.RouterGroup) {\n",
        "\tr.GET(\"/a\", handlerA)\n",
        "}\n\n",
        "func B(r *gin.RouterGroup) {\n",
        "\tr.GET(\"/b\", handlerB)\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.iter().all(|f| f.name != "r"));

    let a = frag(&frags, "A");
    assert_eq!(a.entries.len(), 1);
    match &a.entries[0] {
        RouterMountEntry::Verb { path, .. } => assert_eq!(path, "/a"),
        other => panic!("expected Verb, got {other:?}"),
    }

    let b = frag(&frags, "B");
    assert_eq!(b.entries.len(), 1);
    match &b.entries[0] {
        RouterMountEntry::Verb { path, .. } => assert_eq!(path, "/b"),
        other => panic!("expected Verb, got {other:?}"),
    }
}

#[test]
fn group_var_colliding_with_an_import_name_still_mounts_with_no_specifier() {
    // Opus review F1 regression: `db := r.Group("/db")` in a file that ALSO imports a package whose
    // local binding is `db`. The Mount's ident is the fresh local group variable, so its specifier
    // must be None — attaching the colliding import's path would send compose down the
    // resolve-by-specifier branch (unresolvable for a Go path) and silently drop the group's routes.
    let src = concat!(
        "package main\n\n",
        "import (\n\t\"github.com/gin-gonic/gin\"\n\t\"example.com/app/db\"\n)\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tdb := r.Group(\"/db\")\n",
        "\tdb.GET(\"/ping\", pingDb)\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("a.go", src);
    let parent = frag(&frags, "r");
    match &parent.entries[0] {
        RouterMountEntry::Mount {
            prefix,
            ident,
            specifier,
            ..
        } => {
            assert_eq!(prefix, "/db");
            assert_eq!(ident, "db");
            assert_eq!(
                specifier, &None,
                "local group var must never carry a specifier"
            );
        }
        other => panic!("expected Mount, got {other:?}"),
    }
    // The group fragment itself still carries the verb, joinable by ident.
    let group = frag(&frags, "db");
    assert!(matches!(
        &group.entries[0],
        RouterMountEntry::Verb { method, path, .. } if method == "GET" && path == "/ping"
    ));
}

// ---------------------------------------------------------------------------------------------
// Arity-asymmetry fix (gin-group-param-v2): the def-side function-parameter registration already
// handled MULTI-parameter registration functions (`func Register(db *DB, r *gin.RouterGroup)`), but
// the call-side `try_call_site` used to reject any call with more than one argument, so a multi-arg
// call site never mounted the fragment the def-side registered — its routes surfaced unprefixed as
// an unmounted-fragment DFS root. `try_call_site` is now arity-agnostic: exactly one
// mountable-receiver argument is required, every other argument is ignored, and two-or-more
// mountable-receiver arguments is rejected as ambiguous.
// ---------------------------------------------------------------------------------------------

#[test]
fn call_site_multi_arg_with_one_receiver_mounts_prefixed() {
    // pkg.Register(db, api.Group("/admin")) — `db` is a non-receiver argument (ignored), the single
    // `api.Group("/admin")` argument is the mountable receiver. Pins the Mount emitted on `api`'s own
    // fragment: prefix "/admin", ident "Register".
    let src = concat!(
        "package main\n\n",
        "import (\n",
        "\t\"github.com/gin-gonic/gin\"\n",
        "\t\"example.com/app/pkg\"\n",
        ")\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tapi := r.Group(\"/api\")\n",
        "\tdb := connect()\n",
        "\tpkg.Register(db, api.Group(\"/admin\"))\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("main.go", src);
    let api = frag(&frags, "api");
    assert_eq!(api.entries.len(), 1);
    match &api.entries[0] {
        RouterMountEntry::Mount {
            prefix,
            ident,
            specifier,
            ..
        } => {
            assert_eq!(prefix, "/admin");
            assert_eq!(ident, "Register");
            assert_eq!(specifier.as_deref(), Some("example.com/app/pkg"));
        }
        other => panic!("expected Mount, got {other:?}"),
    }
}

#[test]
fn def_side_multi_param_function_registers_fragment_named_after_function() {
    // pkg/routes.go: func Register(db *DB, r *gin.RouterGroup) { r.POST(...) } — pins that the
    // def-side half of the repro (already correct before this fix) registers the gin-typed parameter
    // as a tracked receiver whose fragment is the enclosing function's own name, `db` being a plain
    // non-gin parameter that is never registered.
    let src = concat!(
        "package pkg\n\n",
        "import \"github.com/gin-gonic/gin\"\n\n",
        "func Register(db *DB, r *gin.RouterGroup) {\n",
        "\tr.POST(\"/create\", Create)\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("pkg/routes.go", src);
    assert!(frags.iter().all(|f| f.name != "r"));
    assert!(frags.iter().all(|f| f.name != "db"));
    let f = frag(&frags, "Register");
    assert_eq!(f.entries.len(), 1);
    match &f.entries[0] {
        RouterMountEntry::Verb {
            method,
            path,
            handler,
            ..
        } => {
            assert_eq!(method, "POST");
            assert_eq!(path, "/create");
            assert_eq!(handler.as_deref(), Some("Create"));
        }
        other => panic!("expected Verb, got {other:?}"),
    }
}

#[test]
fn call_site_two_mountable_receivers_is_ambiguous_and_rejected() {
    // pkg.Wire(api.Group("/a"), admin.Group("/b")) — TWO mountable-receiver arguments in the same
    // call is genuinely ambiguous (which one does Wire actually mount onto?), so the whole call is
    // rejected: neither api nor admin gets a "Wire" Mount entry, and r's own two Group-binding Mounts
    // (to api and admin) are untouched by the rejection.
    let src = concat!(
        "package main\n\n",
        "import (\n",
        "\t\"github.com/gin-gonic/gin\"\n",
        "\t\"example.com/app/pkg\"\n",
        ")\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tapi := r.Group(\"/a\")\n",
        "\tadmin := r.Group(\"/b\")\n",
        "\tpkg.Wire(api.Group(\"/a\"), admin.Group(\"/b\"))\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("main.go", src);
    assert!(frags.iter().all(|f| f.name != "api"));
    assert!(frags.iter().all(|f| f.name != "admin"));
    let r = frag(&frags, "r");
    assert_eq!(r.entries.len(), 2); // only the two Group-binding mounts, nothing from Wire
    for entry in &r.entries {
        match entry {
            RouterMountEntry::Mount { ident, .. } => {
                assert_ne!(ident, "Wire", "ambiguous call must never emit a Wire mount");
            }
            other => panic!("expected Mount, got {other:?}"),
        }
    }
}

#[test]
fn call_site_single_arg_shape_regression_still_mounts() {
    // Regression pin for the arity-agnostic rewrite of try_call_site: the pre-existing plain
    // single-argument shape must still mount exactly as before the multi-arg fix.
    let src = concat!(
        "package main\n\n",
        "import (\n",
        "\t\"github.com/gin-gonic/gin\"\n",
        "\t\"example.com/app/pkg\"\n",
        ")\n\n",
        "func main() {\n",
        "\tr := gin.Default()\n",
        "\tapi := r.Group(\"/api\")\n",
        "\tpkg.Register(api.Group(\"/admin\"))\n",
        "}\n",
    );
    let frags = extract_go_router_fragments("main.go", src);
    let api = frag(&frags, "api");
    assert_eq!(api.entries.len(), 1);
    match &api.entries[0] {
        RouterMountEntry::Mount {
            prefix,
            ident,
            specifier,
            ..
        } => {
            assert_eq!(prefix, "/admin");
            assert_eq!(ident, "Register");
            assert_eq!(specifier.as_deref(), Some("example.com/app/pkg"));
        }
        other => panic!("expected Mount, got {other:?}"),
    }
}
