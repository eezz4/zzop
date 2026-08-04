use super::*;

const RT_IMPORT: &str = "import org.springframework.web.client.RestTemplate;\n";
const WC_IMPORT: &str = "import org.springframework.web.reactive.function.client.WebClient;\n";

// --- RestTemplate ---------------------------------------------------------------------------------

#[test]
fn resttemplate_get_for_object_literal_path_is_keyed() {
    let src = format!(
        "{RT_IMPORT}class C {{ String m(RestTemplate rt) {{ return rt.getForObject(\"/api/users\", String.class); }} }}"
    );
    let out = extract_java_http_consumes("src/main/java/C.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users"));
    assert_eq!(out[0].client.as_deref(), Some("resttemplate"));
    assert_eq!(out[0].raw, None);
}

#[test]
fn resttemplate_post_for_entity_and_absolute_url() {
    let src = format!(
        "{RT_IMPORT}class C {{ void m(RestTemplate rt) {{ rt.postForEntity(\"https://vendor.example/x\", null, Void.class); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out[0].key.as_deref(), Some("POST https://vendor.example/x"));
}

#[test]
fn resttemplate_query_suffix_is_dropped_from_the_key() {
    let src = format!(
        "{RT_IMPORT}class C {{ void m(RestTemplate rt) {{ rt.getForObject(\"/api/users?limit=10\", String.class); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users"));
}

#[test]
fn resttemplate_exchange_with_literal_http_method_is_keyed() {
    let src = format!(
        "{RT_IMPORT}import org.springframework.http.HttpMethod;\nclass C {{ void m(RestTemplate rt) {{ rt.exchange(\"/api/users/1\", HttpMethod.PUT, null, Void.class); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("PUT /api/users/1"));
}

#[test]
fn resttemplate_execute_with_literal_http_method_is_keyed() {
    let src = format!(
        "{RT_IMPORT}import org.springframework.http.HttpMethod;\nclass C {{ void m(RestTemplate rt) {{ rt.execute(\"/api/orders\", HttpMethod.POST, null, null); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("POST /api/orders"));
    assert_eq!(out[0].client.as_deref(), Some("resttemplate"));
}

#[test]
fn resttemplate_execute_with_non_literal_method_is_skipped() {
    let src = format!(
        "{RT_IMPORT}class C {{ void m(RestTemplate rt, Object verb) {{ rt.execute(\"/api/orders\", verb, null, null); }} }}"
    );
    assert!(extract_java_http_consumes("C.java", &src).is_empty());
}

#[test]
fn executor_execute_with_a_single_runnable_is_not_recognized() {
    // `Executor.execute(runnable)` has one argument — no second `HttpMethod` literal to read — so a
    // RestTemplate-importing file scheduling work must never mint an egress consume.
    let src = format!(
        "{RT_IMPORT}import java.util.concurrent.Executor;\nclass C {{ void m(Executor ex, Runnable r) {{ ex.execute(r); }} }}"
    );
    assert!(extract_java_http_consumes("C.java", &src).is_empty());
}

#[test]
fn resttemplate_exchange_with_non_literal_method_is_skipped() {
    let src = format!(
        "{RT_IMPORT}class C {{ void m(RestTemplate rt, Object verb) {{ rt.exchange(\"/api/users\", verb, null, Void.class); }} }}"
    );
    assert!(extract_java_http_consumes("C.java", &src).is_empty());
}

#[test]
fn variable_url_is_unresolved_never_guessed() {
    let src = format!(
        "{RT_IMPORT}class C {{ String m(RestTemplate rt, String url) {{ return rt.getForObject(url, String.class); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("url"));
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn concatenated_url_is_unresolved() {
    let src = format!(
        "{RT_IMPORT}class C {{ void m(RestTemplate rt, long id) {{ rt.getForObject(\"/api/users/\" + id, String.class); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("\"/api/users/\" + id"));
}

#[test]
fn base_relative_literal_is_unresolved() {
    let src = format!(
        "{RT_IMPORT}class C {{ void m(RestTemplate rt) {{ rt.getForObject(\"users\", String.class); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("\"users\""));
}

#[test]
fn generic_put_on_a_map_is_not_recognized() {
    // `put`/`delete` are deliberately absent from the vocabulary (module doc): a HashMap `put` in a
    // RestTemplate-importing file must never mint an egress consume.
    let src = format!(
        "{RT_IMPORT}import java.util.HashMap;\nclass C {{ void m() {{ new HashMap<String, String>().put(\"/api/users\", \"x\"); }} }}"
    );
    assert!(extract_java_http_consumes("C.java", &src).is_empty());
}

#[test]
fn import_gate_blocks_extraction_without_the_import() {
    let src =
        "class C { String m(RestTemplate rt) { return rt.getForObject(\"/api/users\", String.class); } }";
    assert!(extract_java_http_consumes("C.java", src).is_empty());
}

#[test]
fn package_glob_import_also_gates() {
    let src = "import org.springframework.web.client.*;\nclass C { void m(RestTemplate rt) { rt.getForObject(\"/api/users\", String.class); } }";
    let out = extract_java_http_consumes("C.java", src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users"));
}

// --- WebClient ------------------------------------------------------------------------------------

#[test]
fn webclient_verb_chain_uri_literal_is_keyed() {
    let src = format!(
        "{WC_IMPORT}class C {{ void m(WebClient client) {{ client.get().uri(\"/api/orders\").retrieve(); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/orders"));
    assert_eq!(out[0].client.as_deref(), Some("webclient"));
}

#[test]
fn webclient_method_http_method_chain_is_keyed() {
    let src = format!(
        "{WC_IMPORT}import org.springframework.http.HttpMethod;\nclass C {{ void m(WebClient client) {{ client.method(HttpMethod.DELETE).uri(\"/api/orders/{{id}}\").retrieve(); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out[0].key.as_deref(), Some("DELETE /api/orders/{}"));
}

#[test]
fn webclient_uri_lambda_is_unresolved() {
    let src = format!(
        "{WC_IMPORT}class C {{ void m(WebClient client) {{ client.get().uri(b -> b.path(\"/api/orders\").build()).retrieve(); }} }}"
    );
    let out = extract_java_http_consumes("C.java", &src);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn uri_without_a_verb_in_the_chain_is_skipped() {
    // A `.uri(...)` on something that is not a WebClient request builder (no verb call upstream) —
    // e.g. a UriComponentsBuilder — must not be treated as egress.
    let src = format!("{WC_IMPORT}class C {{ void m(Object b) {{ b.uri(\"/api/orders\"); }} }}");
    assert!(extract_java_http_consumes("C.java", &src).is_empty());
}

#[test]
fn webclient_import_does_not_enable_the_resttemplate_vocabulary() {
    let src = format!(
        "{WC_IMPORT}class C {{ void m(Object rt) {{ rt.getForObject(\"/api/users\", String.class); }} }}"
    );
    assert!(extract_java_http_consumes("C.java", &src).is_empty());
}

// --- shared gates ---------------------------------------------------------------------------------

#[test]
fn test_classified_paths_are_silent() {
    let src = format!(
        "{RT_IMPORT}class GatewayTest {{ void m(RestTemplate rt) {{ rt.getForObject(\"/api/users\", String.class); }} }}"
    );
    // The Maven/Gradle test source root (`/test/` segment) and the `*Test.java` suffix both gate.
    assert!(extract_java_http_consumes("src/test/java/com/acme/Gateway.java", &src).is_empty());
    assert!(extract_java_http_consumes("src/main/java/com/acme/GatewayTest.java", &src).is_empty());
}

#[test]
fn empty_on_parse_failure() {
    assert!(extract_java_http_consumes("C.java", "\u{0}\u{1}not java{{{{").is_empty());
}
