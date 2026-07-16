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
