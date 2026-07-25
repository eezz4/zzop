//! Per-branch `symbol` attribution (module doc "`symbol` — per-BRANCH, two cases"): case 1 gives a
//! single-call branch body its CALLEE, case 2 keeps the enclosing function for everything else.
//! The defect these pin: a Cloudflare-Worker-style dispatcher serves N routes from one function, so
//! attributing all N provides to `dispatch` made the consuming rules' call-graph BFS see the UNION
//! of every sibling route's reachability (measured FN in `mutating-route-no-auth`, 6/6 FP in
//! `unsafe-read-endpoint`).
use super::super::extract_pathname_dispatch_provides;
use super::{keyed_symbols, keys};

/// A minimal dispatcher whose single `POST /api/groups` branch has `body` as its whole consequent.
fn dispatcher_with_branch(body: &str) -> String {
    concat!(
        "async function dispatch(request: Request, env: Env, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  if (pathname === \"/api/groups\" && method === \"POST\") "
    )
    .to_string()
        + body
        + "\n}\n"
}

fn symbol_of(src: &str, key: &str) -> Option<String> {
    let out = extract_pathname_dispatch_provides("handleRequest.ts", src);
    let hit: Vec<_> = out.iter().filter(|p| p.key == key).collect();
    assert_eq!(
        hit.len(),
        1,
        "expected exactly one `{key}` provide: {out:?}"
    );
    hit[0].symbol.clone()
}

// -- Case 1: a single-call branch body attributes to the branch target --

#[test]
fn single_return_call_branch_attributes_to_callee() {
    let src = concat!(
        "async function dispatch(request: Request, env: Env, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  if (pathname === \"/api/groups\" && method === \"POST\") return createGroup(request, env);\n",
        "}\n"
    );
    assert_eq!(
        symbol_of(src, "POST /api/groups").as_deref(),
        Some("createGroup")
    );
}

#[test]
fn one_statement_block_await_and_paren_wrappings_still_attribute_to_callee() {
    for body in [
        "{ return createGroup(request, env); }",
        "{ return await createGroup(request, env); }",
        "return (createGroup(request, env));",
        "{ createGroup(request, env); }",
    ] {
        let src = dispatcher_with_branch(body);
        assert_eq!(
            symbol_of(&src, "POST /api/groups").as_deref(),
            Some("createGroup"),
            "body: {body}"
        );
    }
}

#[test]
fn switch_case_with_a_single_call_body_attributes_to_callee() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  switch (url.pathname) {\n",
        "    case \"/a\":\n",
        "      return handleA(request);\n",
        "    case \"/b\":\n",
        "      return handleB(request);\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("handleRequest.ts", src);
    assert_eq!(
        keyed_symbols(&out),
        vec![
            ("? /a".to_string(), Some("handleA".to_string())),
            ("? /b".to_string(), Some("handleB".to_string())),
        ]
    );
}

// -- Case 2: anything else keeps the enclosing function --

#[test]
fn multi_statement_branch_keeps_the_enclosing_function() {
    let src = concat!(
        "async function dispatch(request: Request, env: Env, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  if (pathname === \"/api/groups\" && method === \"POST\") {\n",
        "    const body = await request.json();\n",
        "    return createGroup(body, env);\n",
        "  }\n",
        "}\n"
    );
    assert_eq!(
        symbol_of(src, "POST /api/groups").as_deref(),
        Some("dispatch")
    );
}

#[test]
fn unnameable_branch_expressions_keep_the_enclosing_function() {
    // `new Response(...)` is not a call; a member callee is deliberately not nameable (picking one
    // property off the chain would be a guess); an inline arrow names nothing at all.
    for body in [
        "return new Response(\"ok\");",
        "return handlers.createGroup(request, env);",
        "return env.DB.prepare(\"insert into x values (1)\").run();",
        "return [1].map((n) => createGroup(n));",
        "return 1;",
    ] {
        let src = dispatcher_with_branch(body);
        assert_eq!(
            symbol_of(&src, "POST /api/groups").as_deref(),
            Some("dispatch"),
            "body: {body}"
        );
    }
}

#[test]
fn wrapper_call_keeps_the_enclosing_function() {
    // The outermost callee is a response wrapper; the REAL handler is the inner call. Naming the
    // wrapper would aim all three consuming BFSes (`mutating-route-no-auth`,
    // `unsafe-read-endpoint`, `non-idempotent-write`) at a subtree that reaches neither the auth
    // guard nor the write inside `createGroup`/`handleThing` — `mutating-route-no-auth` would newly
    // ACCUSE a guarded write route. The enclosing symbol only ever over-reaches, so it wins.
    for body in [
        "return ok(handleThing(req));",
        "return json(await createGroup(req, env));",
        "return ok(await handleThing(req), { status: 201 });",
        "handleThing(wrap(req));",
        "return createGroup(request, env, () => audit(request));",
    ] {
        let src = dispatcher_with_branch(body);
        assert_eq!(
            symbol_of(&src, "POST /api/groups").as_deref(),
            Some("dispatch"),
            "body: {body}"
        );
    }
}

#[test]
fn argument_coercion_call_also_keeps_the_enclosing_function() {
    // Module doc "Case-2 residual", pinned as INTENDED behavior, not an accident: here the outer
    // callee IS the handler and `Number.parseInt` only coerces a path segment, but "arguments
    // themselves invoke something" is lexical and cannot tell this apart from the wrapper shape
    // above. Measured 2026-07-25 on mono-hub: this is the one `unsafe-read-endpoint` finding of 6
    // that per-branch attribution did not clear (`settle-hub-be` `GET /api/ledger/{}/revision/{}`).
    // Whoever narrows this must first show the wrapper shape above still keeps `dispatch`.
    for body in [
        "return getRevision(request, env, m[1], Number.parseInt(m[2], 10));",
        "return getRevision(request, env, decodeURIComponent(m[1]));",
    ] {
        let src = dispatcher_with_branch(body);
        assert_eq!(
            symbol_of(&src, "POST /api/groups").as_deref(),
            Some("dispatch"),
            "body: {body}"
        );
    }
}

#[test]
fn multi_statement_switch_case_keeps_the_enclosing_function() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  switch (url.pathname) {\n",
        "    case \"/a\":\n",
        "      handleA(request);\n",
        "      break;\n",
        "  }\n",
        "}\n"
    );
    assert_eq!(symbol_of(src, "? /a").as_deref(), Some("dispatch"));
}

// -- The defect itself: sibling routes of ONE dispatcher must not share a symbol --

#[test]
fn sibling_routes_in_one_dispatcher_get_different_symbols() {
    // The measured shape: a GET sibling and three mutating siblings, one of which reaches an auth
    // guard. Under the old enclosing-only rule every key below carried `dispatch`, so the consuming
    // rules' BFS unioned all four routes' reachability.
    let src = concat!(
        "async function dispatch(request: Request, env: Env, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  if (pathname === \"/api/rates\" && method === \"GET\") return getRates(request, env);\n",
        "  if (pathname === \"/api/groups\" && method === \"POST\") return createGroup(request, env);\n",
        "  if (pathname === \"/api/join\" && method === \"POST\") return joinGroup(request, env);\n",
        "  const verifyMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)\\/verify$/);\n",
        "  if (verifyMatch && method === \"POST\") return verifyCode(request, env, verifyMatch[1]);\n",
        "  return jsonError(404, \"not_found\");\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("handleRequest.ts", src);
    assert_eq!(
        keyed_symbols(&out),
        vec![
            ("GET /api/rates".to_string(), Some("getRates".to_string())),
            (
                "POST /api/groups".to_string(),
                Some("createGroup".to_string())
            ),
            ("POST /api/join".to_string(), Some("joinGroup".to_string())),
            (
                "POST /api/ledger/{}/verify".to_string(),
                Some("verifyCode".to_string())
            ),
        ],
        "each sibling route must carry its OWN handler; occurrence order must stay deterministic"
    );
}

// -- Determinism: dedup keys on (key, line, symbol), and order stays occurrence order --

#[test]
fn same_route_through_two_branches_with_different_symbols_stays_two_provides() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  if (pathname === \"/x\" && method === \"POST\") return alpha(request);\n",
        "  if (pathname === \"/x\" && method === \"POST\") return beta(request);\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("handleRequest.ts", src);
    // Distinct lines AND distinct symbols -> the `(key, line, symbol)` dedup keeps both, in
    // occurrence order.
    assert_eq!(keys(&out), vec!["POST /x", "POST /x"]);
    assert_eq!(
        out.iter().map(|p| p.symbol.clone()).collect::<Vec<_>>(),
        vec![Some("alpha".to_string()), Some("beta".to_string())]
    );
}
