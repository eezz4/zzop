use super::{hits, scan, TempDir};

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
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  const cmd = c.req.query(\"cmd\");\n  // taint-ok: admin only, internal tooling\n  eval(cmd);\n  return c.json({});\n};\n",
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
        "declare const pluginCode: string;\nexport function run() {\n  // eval-dynamic-ok: sandboxed plugin worker, no user-controlled input\n  eval(pluginCode);\n}\n",
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
