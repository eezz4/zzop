use crate::{hits, scan, TempDir};

// --- insecure-cookie ---

#[test]
fn cookie_set_without_httponly_anywhere_in_the_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  res.cookie(\"session\", token);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "insecure-cookie");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn cookie_set_with_httponly_option_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  res.cookie(\"session\", token, { httpOnly: true, secure: true });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "insecure-cookie").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn cookie_ok_marker_above_the_cookie_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  // cookie-ok: non-sensitive UI preference cookie, not session/auth\n  res.cookie(\"theme\", token);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "insecure-cookie").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- api-key-in-url ---

#[test]
fn api_key_query_param_in_url_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "export const url = \"https://api.example.com/data?api_key=abc123\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "api-key-in-url");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn access_token_query_param_in_url_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "export const url = \"https://api.example.com/oauth/callback?access_token=xyz789\";\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "api-key-in-url").len(), 1, "{:?}", out.findings);
}

#[test]
fn url_with_no_secret_param_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "export const url = \"https://api.example.com/data?id=42\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "api-key-in-url").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn url_key_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "// url-key-ok: short-lived one-time token for a third-party webhook callback\nexport const url = \"https://api.example.com/data?api_key=abc123\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "api-key-in-url").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- error-leak-to-client ---

#[test]
fn raw_error_sent_via_res_status_5xx_json_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/errors.ts",
        "declare const res: any;\nexport function handleError(err: any) {\n  res.status(500).json(err);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "error-leak-to-client");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn raw_error_sent_via_hono_c_json_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/errors.ts",
        "declare const c: any;\nexport function handleError(err: any) {\n  return c.json(err);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "error-leak-to-client").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn generic_error_message_sent_to_the_client_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/errors.ts",
        "declare const res: any;\nexport function handleError(err: any) {\n  console.error(err);\n  res.status(500).json({ error: \"Internal server error\" });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "error-leak-to-client").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- stacktrace-to-response (Java) ---

#[test]
fn print_stack_trace_in_a_method_that_also_returns_a_response_entity_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/ApiController.java",
        "public class ApiController {\n    public ResponseEntity<String> handle(Exception e) {\n        e.printStackTrace();\n        return ResponseEntity.status(500).body(e.getMessage());\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "stacktrace-to-response");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn print_stack_trace_with_no_response_object_in_the_method_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Worker.java",
        "public class Worker {\n    public void process(Exception e) {\n        e.printStackTrace();\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "stacktrace-to-response").is_empty(),
        "{:?}",
        out.findings
    );
}
