use super::*;

#[test]
fn literal_relative_path_keys_via_http_consume_interface_key() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc f() {\n\thttp.Get(\"/users\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
    assert_eq!(out[0].raw, None);
}

#[test]
fn http_client_instance_get_is_a_consume() {
    // `c := &http.Client{}; c.Get(url)` — a bound client value's convenience method keys like `http.Get`.
    let src = "package main\n\nimport \"net/http\"\n\nfunc f() {\n\tc := &http.Client{}\n\tc.Get(\"/users\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn http_client_value_and_new_instances_are_consumes() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc f() {\n\tvar a = http.Client{}\n\ta.Post(\"https://svc/x\")\n\tb := new(http.Client)\n\tb.Head(\"/ping\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out.len(), 2, "{out:?}");
    assert_eq!(out[0].key.as_deref(), Some("POST https://svc/x"));
    assert_eq!(out[1].key.as_deref(), Some("HEAD /ping"));
}

#[test]
fn zero_value_var_declaration_client_is_a_consume() {
    // `var c http.Client` — no initializer; the zero value is a usable client, so `c.Get(url)` is egress.
    let src = "package main\n\nimport \"net/http\"\n\nfunc f() {\n\tvar c http.Client\n\tc.Get(\"/users\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn plain_reassignment_to_a_client_is_a_consume() {
    // `var c *http.Client` then `c = &http.Client{}` (assignment_statement, not `:=`) must still bind.
    let src = "package main\n\nimport \"net/http\"\n\nfunc f() {\n\tvar c *http.Client\n\tc = &http.Client{}\n\tc.Get(\"/users\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn a_non_client_receiver_get_is_not_a_consume() {
    // `.Get` on an unrelated value (a sync.Map) in a net/http-importing file must not be keyed.
    let src = "package main\n\nimport \"net/http\"\n\nfunc f(m mymap) {\n\t_ = http.Get\n\tm.Get(\"/users\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn absolute_url_keys_as_external() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc f() {\n\thttp.Get(\"https://api.example.com/v1/ping\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://api.example.com/v1/ping")
    );
}

#[test]
fn query_suffix_is_dropped() {
    let src =
        "package main\n\nimport \"net/http\"\n\nfunc f() {\n\thttp.Get(\"/users?limit=10\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn sprintf_reassembly_collapses_verbs() {
    let src = "package main\n\nimport (\n\t\"fmt\"\n\t\"net/http\"\n)\n\nfunc f(id int) {\n\thttp.Get(fmt.Sprintf(\"/users/%d\", id))\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
}

#[test]
fn sprintf_headed_by_interpolation_is_unresolved() {
    let src = "package main\n\nimport (\n\t\"fmt\"\n\t\"net/http\"\n)\n\nfunc f(base string) {\n\thttp.Get(fmt.Sprintf(\"%s/users\", base))\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out[0].key, None);
    assert!(out[0].raw.is_some());
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn post_and_postform_and_head_verbs() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc f() {\n\thttp.Post(\"/users\", \"application/json\", nil)\n\thttp.PostForm(\"/submit\", nil)\n\thttp.Head(\"/status\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    let keys: Vec<_> = out.iter().map(|c| c.key.clone().unwrap()).collect();
    assert!(keys.contains(&"POST /users".to_string()));
    assert!(keys.contains(&"POST /submit".to_string()));
    assert!(keys.contains(&"HEAD /status".to_string()));
}

#[test]
fn nested_call_site_inside_helper_function_is_reachable() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc fetch() {\n\tif true {\n\t\thttp.Get(\"/nested\")\n\t}\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /nested"));
}

#[test]
fn unresolved_bare_name_argument_is_witnessed_not_dropped() {
    let src = "package main\n\nimport \"net/http\"\n\nfunc f(u string) {\n\thttp.Get(u)\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("u"));
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn no_import_gate_negative() {
    let src = "package main\n\nfunc f() {\n\thttp.Get(\"/users\")\n}\n";
    let out = extract_go_http_consumes("a.go", src);
    assert!(out.is_empty());
}

#[test]
fn verb_methods_verbs_are_core_key_verbs_plus_deliberate_head() {
    // T2 subset pin with one T3 carve-out (see VERB_METHODS's doc): every emitted verb must be a
    // core join verb, except the deliberate HEAD divergence — honest-but-unjoinable client fact.
    for (_, verb) in super::VERB_METHODS {
        assert!(
            zzop_core::HTTP_KEY_VERBS.contains(verb) || *verb == "HEAD",
            "VERB_METHODS emits {verb}, which is neither a core HTTP_KEY_VERBS member nor the pinned HEAD carve-out"
        );
    }
    assert!(
        super::VERB_METHODS.iter().any(|(_, v)| *v == "HEAD"),
        "the HEAD carve-out disappeared — update the doc + this pin together"
    );
}
