//! The canonical corpus shapes (A-D), the switch fallthrough-grouping shape, and verb-mention
//! edge cases.
use super::super::extract_pathname_dispatch_provides;
use super::keys;

// -- Shape A: nested method ifs, destructured pathname, method alias, `url: URL` param --

#[test]
fn shape_a_nested_method_ifs_with_destructured_pathname() {
    let src = concat!(
        "async function dispatch(request: Request, env: Env, url: URL): Promise<Response> {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  if (pathname === \"/me/achievements\") {\n",
        "    const playerId = extractPlayerId(request);\n",
        "    if (!playerId) return jsonErr(401, \"unauthorized\", \"x\");\n",
        "    if (method === \"GET\") return handleGet(playerId, env);\n",
        "    if (method === \"POST\") return handlePost(playerId, request, env);\n",
        "  }\n",
        "  return jsonErr(404, \"not_found\", \"Route not found\");\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(got, vec!["GET /me/achievements", "POST /me/achievements"]);
    assert!(out.iter().all(|p| p.symbol.as_deref() == Some("dispatch")));
    assert!(out.iter().all(|p| p.file == "worker.ts"));
    let path_test_line = 4; // `if (pathname === "/me/achievements") {`
    assert!(out.iter().all(|p| p.line == path_test_line));
}

// -- Shape B: method-first with an OR path group (health-check idiom) --

#[test]
fn shape_b_method_first_with_or_path_group() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (request.method === \"GET\" && (url.pathname === \"/\" || url.pathname === \"/health\")) {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(got, vec!["GET /", "GET /health"]);
}

// -- Shape C: compound guard in a plain function --

#[test]
fn shape_c_compound_guard_plain_function() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert_eq!(keys(&out), vec!["POST /apply"]);
}

// -- Shape D: object-literal Workers entry, untyped JS, no method mention -> verb-unknown sentinel --

#[test]
fn shape_d_object_literal_workers_entry_fallback_verbs() {
    let src = concat!(
        "export default {\n",
        "  async fetch(request, env) {\n",
        "    const url = new URL(request.url);\n",
        "    if (url.pathname === \"/webhook\") {\n",
        "      return handle(request, env);\n",
        "    }\n",
        "  },\n",
        "};\n"
    );
    let out = extract_pathname_dispatch_provides("worker.js", src);
    let mut got = keys(&out);
    got.sort();
    // No method comparison -> one UNKNOWN_VERB sentinel provide, not fabricated GET+POST.
    assert_eq!(got, vec!["? /webhook"]);
    assert!(out.iter().all(|p| p.symbol.as_deref() == Some("fetch")));
}

// -- switch (url.pathname): method-if case, no-method case, fallthrough-grouped DELETE case --

#[test]
fn switch_on_pathname_with_fallthrough_grouping() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  switch (url.pathname) {\n",
        "    case \"/a\":\n",
        "      if (request.method === \"GET\") return ok();\n",
        "      break;\n",
        "    case \"/b\":\n",
        "      doSomething();\n",
        "      break;\n",
        "    case \"/d\":\n",
        "    case \"/e\":\n",
        "      if (request.method === \"DELETE\") return ok();\n",
        "      break;\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    let mut got = keys(&out);
    got.sort();
    // `/a` explicit GET, `/d`/`/e` explicit DELETE; `/b` names no method -> UNKNOWN_VERB sentinel (`?`),
    // not fabricated GET+POST. `?` (0x3F) sorts before the letters.
    assert_eq!(got, vec!["? /b", "DELETE /d", "DELETE /e", "GET /a"]);
}

// -- Verb `!==` mention --

#[test]
fn verb_not_equal_mention_counts() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname === \"/x\") {\n",
        "    if (request.method !== \"POST\") return err();\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert_eq!(keys(&out), vec!["POST /x"]);
}

// -- Reversed operands, both path and verb --

#[test]
fn reversed_operands_both_path_and_verb() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (\"/x\" === url.pathname && \"POST\" === request.method) {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert_eq!(keys(&out), vec!["POST /x"]);
}

// -- Verb-only OR disjunction unions its verbs --

#[test]
fn verb_only_or_disjunction_unions_verbs() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname === \"/x\" && (request.method === \"PUT\" || request.method === \"DELETE\")) {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(got, vec!["DELETE /x", "PUT /x"]);
}

// -- Mixed OR (path || flag) is discarded entirely --

#[test]
fn mixed_or_path_and_flag_is_discarded() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname === \"/a\" || someFlag) {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert!(out.is_empty(), "{out:?}");
}
