use super::{assert_landing_precedes_imperative, hits, scan, TempDir};

#[test]
fn hono_req_json_into_eval_is_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "handler.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  const body = await c.req.json();\n  eval(body);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn hono_req_query_into_exec_is_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "exec.ts",
        "import type { Context } from \"hono\";\nimport { exec } from \"node:child_process\";\nexport const h = async (c: Context) => {\n  const cmd = c.req.query(\"cmd\");\n  exec(cmd);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn schema_parse_sanitizer_clears_the_finding() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "safe.ts",
        "import type { Context } from \"hono\";\ndeclare const schema: any;\nexport const h = async (c: Context) => {\n  const raw = await c.req.json();\n  const safe = schema.parse(raw);\n  eval(safe);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

#[test]
fn json_parse_of_a_tainted_value_does_not_veto_the_finding() {
    // `JSON.parse(req.body)` is the commonest way to OBTAIN the tainted value, not a sanitizer —
    // parsing JSON validates syntax, not safety for an eval/exec/SQL sink. Regression pin for the
    // formerly-over-broad `\.parse\(` veto that also swallowed `JSON.parse(`/`Date.parse(`.
    let dir = TempDir::new("zzop-security");
    dir.write(
        "json.ts",
        "export function handler(req: any, res: any) {\n  const data = JSON.parse(req.body);\n  eval(data.code);\n  res.end();\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn dangerously_set_inner_html_with_tainted_value_is_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "Comp.tsx",
        "import type { Context } from \"hono\";\nexport const renderFromReq = async (c: Context) => {\n  const data = await c.req.json();\n  return <div dangerouslySetInnerHTML={{ __html: data.html }}>x</div>;\n};\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn innerhtml_equality_comparison_is_not_a_write_sink() {
    // `el.innerHTML === x` / `== x` is a READ (comparison), not an assignment. The sink pattern must not
    // treat the `=` of `===`/`==` as an innerHTML write, or any handler that also touches request input
    // false-fires taint-flow.
    let dir = TempDir::new("zzop-security");
    dir.write(
        "handler.ts",
        "export function check(req: any) {\n  const want = req.query.h;\n  const el: any = document.body;\n  if (el.innerHTML === want) return true;\n  return el.outerHTML == want;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "taint-flow").is_empty(),
        "an innerHTML/outerHTML equality read must not fire the write sink: {:?}",
        out.findings
    );
}

#[test]
fn next_request_json_into_execute_raw_unsafe_is_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "route.ts",
        "declare const prisma: any;\nexport async function POST(request: Request) {\n  const body = await request.json();\n  await prisma.$executeRawUnsafe(body.sql);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn express_req_query_into_eval_is_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "express.ts",
        "export function handler(req: any, res: any) {\n  const expr = req.query.expr;\n  eval(expr);\n  res.end();\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn search_params_get_into_dangerously_set_inner_html_is_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "Page.tsx",
        "export function Page({ searchParams }: { searchParams: URLSearchParams }) {\n  const html = searchParams.get(\"html\");\n  return <div dangerouslySetInnerHTML={{ __html: html }}>x</div>;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn taint_ok_marker_directly_above_the_sink_suppresses_the_finding() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "marked.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  const cmd = c.req.query(\"cmd\");\n  // zzop-taint-flow-ok: admin only, internal tooling\n  eval(cmd);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

#[test]
fn plain_function_with_no_source_or_sink_is_not_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "plain.ts",
        "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

// --- skip_comment_lines + test-path exclusion ---

#[test]
fn source_and_sink_mentioned_only_in_a_comment_are_not_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "handler.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  // const body = await c.req.json(); eval(body); -- old handler, removed\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

#[test]
fn source_into_sink_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "src/__tests__/handler.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  const body = await c.req.json();\n  eval(body);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

/// §33/§37 LANDING for `eval-dynamic-code`, spliced ahead of the "avoid dynamic code" imperative.
///
/// WHY. The remedy names three substitutes — a plain function, a lookup table, `JSON.parse` — and each
/// accepts strictly LESS than the call it replaces. JSON is a proper subset of the object syntax
/// `eval` took, so unquoted keys, single quotes, a trailing comma, `undefined`, `NaN` and
/// `new Date(...)` all parse today and throw tomorrow; the legacy shape this rule most often flags —
/// an `eval` around a parenthesised object literal — is precisely a producer of that dialect, so the
/// swap fails on the very inputs that motivated the call. A lookup table replaces NAMES, not an
/// expression language, so where the built string is arithmetic or a user-authored predicate there is
/// no table to write and the landing says the honest thing instead of a smaller lie.
///
/// NOT A DISQUALIFIER — this rule already carries one, and it answers the other question ("if any part
/// of that string can be influenced by user input"). The finding stays right for a reader whose
/// payload is a non-JSON dialect: they are still building code from a runtime string. What changes is
/// that the cheapest of the three substitutes is not available to them.
///
/// POSITION, not presence. The invalidation probe is to move this constant to the tail of the message.
const DYNAMIC_CODE_SUBSTITUTE_LANDING: &str = "EACH SUBSTITUTE IS NARROWER THAN WHAT IT REPLACES, AND THE INPUTS THAT NO LONGER FIT FAIL AT RUNTIME ON REAL DATA RATHER THAN IN REVIEW: JSON is a strict subset of the object syntax `eval` accepted, so a payload carrying unquoted keys, single-quoted strings, a trailing comma, a comment, `undefined`, `NaN` or a `new Date(...)` parsed yesterday and throws a `SyntaxError` out of `JSON.parse` today — and the legacy shape this rule most often flags, an `eval` wrapping a parenthesised object literal, is exactly a producer of that dialect. A lookup table replaces a fixed set of NAMES, not an expression language: where the string being built is arithmetic, a filter predicate or a template someone authored, there is no table to write and the honest answer is a real parser for the small language you actually accept. Read a sample of the strings this call has received in production before you pick one — the substitute is decided by that dialect, not by the call site.";

#[test]
fn eval_dynamic_code_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "run.ts",
        "declare const userInput: string;\nexport function run() {\n  eval(userInput);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "eval-dynamic-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "eval-dynamic-code",
        &h[0].message,
        DYNAMIC_CODE_SUBSTITUTE_LANDING,
        "Avoid dynamic code construction",
    );
    for needle in [
        "JSON is a strict subset of the object syntax `eval` accepted",
        "an `eval` wrapping a parenthesised object literal",
        "A lookup table replaces a fixed set of NAMES, not an expression language",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/eval-dynamic-code: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
}

// --- eval-dynamic-code ---

#[test]
fn eval_with_a_variable_argument_is_flagged_eval_nonliteral() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "run.ts",
        "declare const userInput: string;\nexport function run() {\n  eval(userInput);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "eval-dynamic-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    assert_eq!(
        h[0].data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("eval-nonliteral")
    );
}

#[test]
fn eval_with_a_literal_argument_is_not_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "run2.ts",
        "export function run() {\n  eval(\"2 + 2\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "eval-dynamic-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn new_function_with_a_variable_argument_is_flagged_new_function() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "build.ts",
        "declare const code: string;\nexport function build() {\n  const fn = new Function(code);\n  return fn;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "eval-dynamic-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("new-function")
    );
}

#[test]
fn new_function_with_only_literal_arguments_is_still_flagged() {
    // Boundary pin: `new Function(...)` is flagged regardless of literal args — constructing a function
    // from a string at runtime defeats CSP and every static analyzer even when the body is a fixed literal.
    let dir = TempDir::new("zzop-security");
    dir.write(
        "build2.ts",
        "export function build() {\n  const fn = new Function(\"a\", \"return a * 2\");\n  return fn;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "eval-dynamic-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn plain_js_file_eval_with_a_variable_is_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "run.js",
        "function run(userInput) {\n  eval(userInput);\n}\nmodule.exports = { run };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "eval-dynamic-code");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn eval_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "run3.ts",
        "declare const userInput: string;\nexport function run() {\n  // eval(userInput); -- old implementation, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "eval-dynamic-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn eval_dynamic_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "sandboxed.ts",
        "declare const pluginCode: string;\nexport function run() {\n  // zzop-eval-dynamic-code-ok: sandboxed plugin worker, no user-controlled input\n  eval(pluginCode);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "eval-dynamic-code").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn eval_dynamic_code_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-security");
    dir.write(
        "src/__tests__/run.ts",
        "declare const userInput: string;\nexport function run() {\n  eval(userInput);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "eval-dynamic-code").is_empty(),
        "{:?}",
        out.findings
    );
}
