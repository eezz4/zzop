use super::*;
use crate::adapters::extract_go_router_fragments;

fn verb_paths(fragments: &[RouterMountFragment], name: &str) -> Vec<(String, String)> {
    fragments
        .iter()
        .find(|f| f.name == name)
        .expect("fragment present")
        .entries
        .iter()
        .filter_map(|e| match e {
            RouterMountEntry::Verb { method, path, .. } => Some((method.clone(), path.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn handlefunc_with_go122_verb_pattern() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc main() {\n\thttp.HandleFunc(\"GET /users\", h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert_eq!(
        verb_paths(&frags, "http"),
        vec![("GET".to_string(), "/users".to_string())]
    );
}

#[test]
fn handlefunc_without_leading_verb_emits_unknown_verb_sentinel() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc main() {\n\thttp.HandleFunc(\"/users\", h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    // No leading method token -> serves every method, statically unknown: ONE UNKNOWN_VERB sentinel
    // entry (`?`), not fabricated GET+POST.
    assert_eq!(
        verb_paths(&frags, "http"),
        vec![("?".to_string(), "/users".to_string())]
    );
}

#[test]
fn handle_call_is_recognized_like_handlefunc() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc main() {\n\thttp.Handle(\"/static/\", handler)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    // No leading method token -> one UNKNOWN_VERB sentinel entry (`?`), not fabricated GET+POST.
    assert_eq!(
        verb_paths(&frags, "http"),
        vec![("?".to_string(), "/static/".to_string())]
    );
}

#[test]
fn new_servemux_binding_uses_receiver_name_not_http() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc main() {\n\tmux := http.NewServeMux()\n\tmux.HandleFunc(\"GET /users\", h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.iter().all(|f| f.name != "http"));
    assert_eq!(
        verb_paths(&frags, "mux"),
        vec![("GET".to_string(), "/users".to_string())]
    );
}

#[test]
fn non_literal_pattern_skips_whole_call() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc main() {\n\tp := \"/users\"\n\thttp.HandleFunc(p, h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.iter().all(|f| f.name != "http"));
}

#[test]
fn host_headed_pattern_is_skipped_never_guessed() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc main() {\n\thttp.HandleFunc(\"GET example.com/path\", h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.iter().all(|f| f.name != "http"));
}

#[test]
fn handler_bare_identifier_is_captured() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc main() {\n\thttp.HandleFunc(\"GET /users\", listUsers)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    let f = frags.iter().find(|f| f.name == "http").unwrap();
    match &f.entries[0] {
        RouterMountEntry::Verb { handler, .. } => assert_eq!(handler.as_deref(), Some("listUsers")),
        _ => panic!("expected a Verb entry"),
    }
}

#[test]
fn nested_call_site_inside_if_is_reachable() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc setup(enabled bool) {\n\tif enabled {\n\t\thttp.HandleFunc(\"GET /users\", h)\n\t}\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert_eq!(
        verb_paths(&frags, "http"),
        vec![("GET".to_string(), "/users".to_string())]
    );
}

#[test]
fn no_import_gate_negative() {
    let src = "package main\n\nfunc main() {\n\thttp.HandleFunc(\"GET /users\", h)\n}\n";
    let frags = extract_go_router_fragments("a.go", src);
    assert!(frags.is_empty());
}
