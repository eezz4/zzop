//! `pathname.match(/re/)` regex-dispatch shapes — the raw-Cloudflare-Worker parameterized-route
//! idiom (bound match binding referenced in a later `if`, inline match, `!== null` guard), full
//! mono-hub-style parity, and the never-guess bail cases.
use super::super::extract_pathname_dispatch_provides;
use super::keys;

// -- Bound match: `const m = pathname.match(/re/); if (m && method === "M")` --

#[test]
fn bound_match_single_param() {
    let src = concat!(
        "async function dispatch(request: Request, env: Env, url: URL): Promise<Response> {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  const verifyMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)\\/verify$/);\n",
        "  if (verifyMatch && method === \"POST\") {\n",
        "    return verifyCode(request, env, verifyMatch[1]);\n",
        "  }\n",
        "  return jsonError(404, \"not_found\");\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("handleRequest.ts", src);
    assert_eq!(keys(&out), vec!["POST /api/ledger/{}/verify"]);
    assert!(out.iter().all(|p| p.symbol.as_deref() == Some("dispatch")));
}

// -- Full mono-hub settle-hub-be dispatch: 2 literal routes + 6 regex routes, all native now --

#[test]
fn full_worker_mixed_literal_and_regex() {
    let src = concat!(
        "async function dispatch(request: Request, env: Env, url: URL): Promise<Response> {\n",
        "  const { pathname } = url;\n",
        "  const method = request.method;\n",
        "  if (pathname === \"/api/rates\" && method === \"GET\") return getRates(request, env);\n",
        "  if (pathname === \"/api/ledger\" && method === \"POST\") return createLedger(request, env);\n",
        "  const verifyMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)\\/verify$/);\n",
        "  if (verifyMatch && method === \"POST\") return verifyCode(request, env, verifyMatch[1]);\n",
        "  const ledgerMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)$/);\n",
        "  if (ledgerMatch && method === \"GET\") return getLedger(request, env, ledgerMatch[1]);\n",
        "  const revisionPostMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)\\/revision$/);\n",
        "  if (revisionPostMatch && method === \"POST\") return postRevision(request, env, revisionPostMatch[1]);\n",
        "  const revisionGetMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)\\/revision\\/(\\d+)$/);\n",
        "  if (revisionGetMatch && method === \"GET\") return getRevision(request, env, revisionGetMatch[1]);\n",
        "  const roCodeMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)\\/ro-code$/);\n",
        "  if (roCodeMatch && method === \"POST\") return updateRoCode(request, env, roCodeMatch[1]);\n",
        "  const touchMatch = pathname.match(/^\\/api\\/ledger\\/([^/]+)\\/touch$/);\n",
        "  if (touchMatch && method === \"POST\") return touchLedger(request, env, touchMatch[1]);\n",
        "  return jsonError(404, \"not_found\");\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("handleRequest.ts", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(
        got,
        vec![
            "GET /api/ledger/{}",
            "GET /api/ledger/{}/revision/{}",
            "GET /api/rates",
            "POST /api/ledger",
            "POST /api/ledger/{}/revision",
            "POST /api/ledger/{}/ro-code",
            "POST /api/ledger/{}/touch",
            "POST /api/ledger/{}/verify",
        ]
    );
}

// -- Inline match in the `if` test (no intermediate binding) --

#[test]
fn inline_match() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  if (pathname.match(/^\\/api\\/group\\/([^/]+)\\/join$/) && request.method === \"POST\") {\n",
        "    return join();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert_eq!(keys(&out), vec!["POST /api/group/{}/join"]);
}

// -- `m !== null` truthiness guard unwraps to the binding --

#[test]
fn null_compare_guard() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const m = pathname.match(/^\\/api\\/x\\/([^/]+)$/);\n",
        "  if (m !== null && request.method === \"GET\") return ok();\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert_eq!(keys(&out), vec!["GET /api/x/{}"]);
}

// -- Never-guess: an unconvertible regex (alternation) emits NOTHING, but a literal sibling route in
// the same function still emits — the bail is scoped to the one bad match, not the whole file. --

#[test]
fn bails_on_alternation_but_keeps_sibling_literal() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const altMatch = pathname.match(/^\\/api\\/(foo|bar)$/);\n",
        "  if (altMatch && request.method === \"GET\") return handleAlt();\n",
        "  if (pathname === \"/api/health\" && request.method === \"GET\") return ok();\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    // The alternation route is dropped (never-guessed); only the literal route survives.
    assert_eq!(keys(&out), vec!["GET /api/health"]);
}

// -- Never-guess on block-scope reuse: sibling braced blocks each `const m = pathname.match(...)`
// with a DIFFERENT regex. The flat name-keyed binding map cannot tell the two `m`s apart, so it
// poisons the name and emits NOTHING for either (safe under-extraction) rather than mis-attribute
// the second block's route to the first block's guard. --

#[test]
fn poisons_block_scope_reused_match_binding() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  if (request.method === \"GET\") {\n",
        "    const m = pathname.match(/^\\/api\\/a\\/([^/]+)$/);\n",
        "    if (m) return handleA(m[1]);\n",
        "  }\n",
        "  if (request.method === \"POST\") {\n",
        "    const m = pathname.match(/^\\/api\\/b\\/([^/]+)$/);\n",
        "    if (m) return handleB(m[1]);\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    // Ambiguous binding name across the two blocks -> nothing emitted (never a wrong key).
    assert!(out.is_empty(), "got {:?}", keys(&out));
}

// -- A match on a NON-pathname receiver is ignored (provenance gate still applies) --

#[test]
fn ignores_match_on_non_pathname_receiver() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  const { pathname } = url;\n",
        "  const bodyMatch = someOtherString.match(/^\\/api\\/x\\/([^/]+)$/);\n",
        "  if (bodyMatch && request.method === \"GET\") return ok();\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert!(out.is_empty(), "got {:?}", keys(&out));
}
