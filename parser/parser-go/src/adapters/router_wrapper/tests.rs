use super::*;
use crate::adapters::extract_go_router_fragments;

fn frag<'a>(fragments: &'a [RouterMountFragment], name: &str) -> &'a RouterMountFragment {
    fragments
        .iter()
        .find(|f| f.name == name)
        .expect("fragment present")
}

/// Every verb entry across EVERY fragment, in order.
///
/// 🔴 Not `paths(frag(&frags, name))`. Two producers can each emit a fragment under the SAME name, and
/// `frag` returns the first — so a per-name read cannot see a second lane emitting the same route
/// again, which is the exact failure `a_gin_importing_file_is_left_to_the_gin_adapter` exists to catch.
/// An invalidation drill caught that: removing the gin decline outright left that test green.
fn all_paths(fragments: &[RouterMountFragment]) -> Vec<(String, String)> {
    fragments.iter().flat_map(paths).collect()
}

fn paths(f: &RouterMountFragment) -> Vec<(String, String)> {
    f.entries
        .iter()
        .filter_map(|e| match e {
            RouterMountEntry::Verb { method, path, .. } => Some((method.clone(), path.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn reads_a_project_router_with_no_framework_import() {
    // The whole point of the lane: no import names a router, and the routes are still read. This is
    // grafana's `pkg/api/api.go` shape reduced to its skeleton.
    let src = "package api\n\nfunc (hs *HTTPServer) registerRoutes() {\n\tr := hs.RouteRegister\n\tr.Get(\"/logout\", hs.Logout)\n\tr.Post(\"/login\", hs.LoginPost)\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    assert_eq!(
        paths(frag(&frags, "r")),
        vec![
            ("GET".to_string(), "/logout".to_string()),
            ("POST".to_string(), "/login".to_string()),
        ]
    );
}

#[test]
fn callback_group_prefixes_compose_by_descent() {
    let src = "package api\n\nfunc register() {\n\tr := reg()\n\tr.Group(\"/api\", func(apiRoute routing.RouteRegister) {\n\t\tapiRoute.Get(\"/user\", getUser)\n\t\tapiRoute.Group(\"/org\", func(orgRoute routing.RouteRegister) {\n\t\t\torgRoute.Put(\"/prefs\", putPrefs)\n\t\t\torgRoute.Get(\"/\", getOrg)\n\t\t})\n\t})\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    assert_eq!(
        paths(frag(&frags, "r")),
        vec![
            ("GET".to_string(), "/api/user".to_string()),
            ("PUT".to_string(), "/api/org/prefs".to_string()),
            // A group's own root route is the group's path — NOT "/api/org/", which the server would
            // answer as a different route.
            ("GET".to_string(), "/api/org".to_string()),
        ]
    );
}

#[test]
fn two_groups_sharing_a_callback_parameter_name_do_not_cross_mount() {
    // 🔴 The measured reason this lane resolves prefixes by DESCENT rather than by emitting a Mount
    // keyed on the parameter NAME. grafana's own `pkg/api/api.go` binds `dashboardRoute` at both
    // `/dashboards` and `/dashboard/snapshots`; a name-keyed fragment would serve each group's routes
    // under BOTH prefixes.
    let src = "package api\n\nfunc register() {\n\tr := reg()\n\tr.Group(\"/dashboards\", func(dashboardRoute routing.RouteRegister) {\n\t\tdashboardRoute.Post(\"/db\", postDb)\n\t})\n\tr.Group(\"/dashboard/snapshots\", func(dashboardRoute routing.RouteRegister) {\n\t\tdashboardRoute.Delete(\"/:key\", delSnap)\n\t})\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    assert_eq!(
        paths(frag(&frags, "r")),
        vec![
            ("POST".to_string(), "/dashboards/db".to_string()),
            (
                "DELETE".to_string(),
                "/dashboard/snapshots/:key".to_string()
            ),
        ]
    );
}

#[test]
fn a_receiver_whose_mount_point_is_elsewhere_emits_nothing() {
    // `func (api *TeamAPI) registerRoutes(r routing.RouteRegister)` — the prefix is decided by a caller
    // in another file. Emitting "/members" here would key the route at a path the server never answers,
    // which is worse than not emitting it: only the absence looks like the absence it is.
    let src = "package teamapi\n\nfunc (api *TeamAPI) registerRoutes(r routing.RouteRegister) {\n\tr.Get(\"/members\", api.getMembers)\n}\n";
    let frags = extract_go_router_fragments("teamapi.go", src);
    assert!(
        frags.is_empty(),
        "a parameter-rooted receiver must emit nothing, got {frags:?}"
    );
}

#[test]
fn a_group_with_a_non_literal_prefix_refuses_its_whole_body() {
    // Never-guess: the prefix cannot be read, so the callback parameter is an ordinary function
    // parameter and every route registered on it is refused with it.
    let src = "package api\n\nfunc register() {\n\tr := reg()\n\tr.Group(cfg.Prefix, func(sub routing.RouteRegister) {\n\t\tsub.Get(\"/x\", getX)\n\t})\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    assert!(frags.is_empty(), "expected no fragments, got {frags:?}");
}

#[test]
fn any_is_one_unknown_verb_entry_not_a_fan_out() {
    // Deliberately NOT gin's fan-out, and the module doc owns why — the repo ships BOTH answers for
    // this concept (net/http sentinel, gin expansion), the expansion side is a recorded v0.20.0
    // decision, and this lane sits on the sentinel side because it reads a router whose catch-all
    // semantics it has never been shown. This test pins the choice, not the argument.
    let src =
        "package api\n\nfunc register() {\n\tr := reg()\n\tr.Any(\"/proxy\", proxyHandler)\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    let entries = paths(frag(&frags, "r"));
    assert_eq!(entries.len(), 1, "expected one entry, got {entries:?}");
    assert_eq!(entries[0].0, zzop_core::UNKNOWN_VERB.to_uppercase());
    assert_eq!(entries[0].1, "/proxy");
}

#[test]
fn an_unrooted_string_literal_is_not_a_route() {
    // The gate that lets this lane run with no import to key on. `cache.Get("key")` and
    // `headers.Get("Accept")` are the shapes it must not claim, and the corpus says they are the only
    // reason the shape would ever be ambiguous.
    let src = "package svc\n\nfunc load() {\n\tv := cache.Get(\"session-key\")\n\ta := headers.Get(\"Accept\")\n\t_ = v\n\t_ = a\n}\n";
    let frags = extract_go_router_fragments("svc.go", src);
    assert!(frags.is_empty(), "expected no fragments, got {frags:?}");
}

#[test]
fn a_gin_importing_file_is_left_to_the_gin_adapter() {
    // The ownership boundary. `.Any` is the one spelling both lanes read, so a file gin owns must
    // produce gin's answer once — here, gin's fan-out — and never this lane's entry as well.
    let src = "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.Default()\n\tr.Any(\"/proxy\", proxyHandler)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    // Across ALL fragments: gin emits one named "r" and so would this lane, and a per-name read would
    // see only the first of the two.
    let entries = all_paths(&frags);
    assert_eq!(
        entries.len(),
        zzop_core::HTTP_KEY_VERBS.len(),
        "expected gin's fan-out and nothing else, got {entries:?}"
    );
    assert!(
        !entries
            .iter()
            .any(|(m, _)| m == &zzop_core::UNKNOWN_VERB.to_uppercase()),
        "the wrapper lane also emitted for a gin file: {entries:?}"
    );
}

#[test]
fn handle_and_handlefunc_stay_in_the_net_http_lane() {
    // Not this lane's vocabulary. With no `net/http` import nothing reads them at all, which is what
    // proves the exclusion is this adapter's and not net/http's gate doing the work.
    let src = "package api\n\nfunc register() {\n\tr := reg()\n\tr.Handle(\"/a\", h)\n\tr.HandleFunc(\"/b\", h)\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    assert!(frags.is_empty(), "expected no fragments, got {frags:?}");
}

#[test]
fn a_multi_name_parameter_declaration_still_refuses_every_name_it_declares() {
    // `func f(a, b T)` declares BOTH names in one `parameter_declaration`. Reading only the first would
    // leave `b` looking free, and a free identifier is emitted at its file-local path — the exact
    // wrong-key failure this lane refuses.
    let src = "package api\n\nfunc register(a, b routing.RouteRegister) {\n\ta.Get(\"/x\", getX)\n\tb.Get(\"/y\", getY)\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    assert!(frags.is_empty(), "expected no fragments, got {frags:?}");
}

#[test]
fn a_group_binding_does_not_outlive_its_callback_body() {
    // 🔴 An invalidation drill found this hole: deleting the restore in `try_group` left
    // `two_groups_sharing_a_callback_parameter_name_do_not_cross_mount` GREEN, because two SIBLING
    // groups each overwrite the binding on the way in, so the missing restore never shows there.
    //
    // It shows here. `sub` is a group parameter in `a` and an ordinary local router in `b`; a binding
    // that outlives its body would serve `b`'s route at `/api/y` on `a`'s router — a path this program
    // does not answer, keyed to the wrong fragment.
    let src = "package api\n\nfunc a() {\n\tr := reg()\n\tr.Group(\"/api\", func(sub routing.RouteRegister) {\n\t\tsub.Get(\"/x\", getX)\n\t})\n}\n\nfunc b() {\n\tsub := reg()\n\tsub.Get(\"/y\", getY)\n}\n";
    let frags = extract_go_router_fragments("api.go", src);
    assert_eq!(
        paths(frag(&frags, "r")),
        vec![("GET".to_string(), "/api/x".to_string())]
    );
    assert_eq!(
        paths(frag(&frags, "sub")),
        vec![("GET".to_string(), "/y".to_string())]
    );
}
