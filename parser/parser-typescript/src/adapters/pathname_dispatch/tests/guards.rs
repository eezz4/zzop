//! Every never-guess FP guard, the Durable Object veto, the deliberate exclusions, and the
//! pre-gate.
use super::super::extract_pathname_dispatch_provides;
use super::keys;

// -- FP guard: `location.pathname` with no request evidence --

#[test]
fn fp_guard_location_pathname_no_request_evidence() {
    let src = concat!(
        "function onClick() {\n",
        "  if (location.pathname === \"/about\") {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("client.ts", src);
    assert!(out.is_empty(), "{out:?}");
}

// -- FP guard: `new URL(window.location.href)` in a no-request function --

#[test]
fn fp_guard_new_url_from_window_location_no_request_evidence() {
    let src = concat!(
        "function route() {\n",
        "  const u = new URL(window.location.href);\n",
        "  if (u.pathname === \"/x\") {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("client.ts", src);
    assert!(out.is_empty(), "{out:?}");
}

// -- FP guard: `request.nextUrl.pathname` (member-of-member receiver, not provenanced) --

#[test]
fn fp_guard_next_middleware_nexturl_pathname() {
    let src = concat!(
        "function middleware(request: Request) {\n",
        "  if (request.nextUrl.pathname === \"/x\") {\n",
        "    return ok();\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("middleware.ts", src);
    assert!(out.is_empty(), "{out:?}");
}

// -- DO veto via constructor param typed DurableObjectState --

#[test]
fn do_veto_via_constructor_state_param() {
    let src = concat!(
        "class Room {\n",
        "  constructor(state: DurableObjectState, env: unknown) {}\n",
        "  async fetch(request: Request, url: URL) {\n",
        "    if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
        "      return ok();\n",
        "    }\n",
        "  }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("room.ts", src);
    assert!(out.is_empty(), "{out:?}");
}

// -- DO veto via `implements DurableObject` / `extends DurableObject` --

#[test]
fn do_veto_via_implements_and_extends_durable_object() {
    let implements_src = concat!(
        "class Room implements DurableObject {\n",
        "  async fetch(request: Request, url: URL) {\n",
        "    if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
        "      return ok();\n",
        "    }\n",
        "  }\n",
        "}\n"
    );
    assert!(extract_pathname_dispatch_provides("room.ts", implements_src).is_empty());

    let extends_src = concat!(
        "class Room extends DurableObject {\n",
        "  async fetch(request: Request, url: URL) {\n",
        "    if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
        "      return ok();\n",
        "    }\n",
        "  }\n",
        "}\n"
    );
    assert!(extract_pathname_dispatch_provides("room.ts", extends_src).is_empty());
}

// -- Exclusions: startsWith, interpolated template, no leading slash, `!==` path guard --

#[test]
fn exclusions_never_emit() {
    let starts_with = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname.startsWith(\"/api\")) { return ok(); }\n",
        "}\n"
    );
    assert!(extract_pathname_dispatch_provides("w.ts", starts_with).is_empty());

    let interpolated = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  const id = \"1\";\n",
        "  if (url.pathname === `/x/${id}`) { return ok(); }\n",
        "}\n"
    );
    assert!(extract_pathname_dispatch_provides("w.ts", interpolated).is_empty());

    let no_leading_slash = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname === \"x\") { return ok(); }\n",
        "}\n"
    );
    assert!(extract_pathname_dispatch_provides("w.ts", no_leading_slash).is_empty());

    let not_equal_guard = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname !== \"/x\") { return err(); }\n",
        "  return ok();\n",
        "}\n"
    );
    assert!(extract_pathname_dispatch_provides("w.ts", not_equal_guard).is_empty());
}

// -- Zero-interpolation template literal path counts --

#[test]
fn zero_interpolation_template_literal_path_is_emitted() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname === `/t`) { return ok(); }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("w.ts", src);
    // No method comparison in the block -> one UNKNOWN_VERB sentinel provide (`?`), not fabricated GET+POST.
    assert_eq!(keys(&out), vec!["? /t"]);
}

// -- Key normalization sanity: double slashes / trailing slash collapse via http_interface_key --

#[test]
fn key_normalization_sanity_via_http_interface_key() {
    let src = concat!(
        "function dispatch(request: Request, url: URL) {\n",
        "  if (url.pathname === \"/api//x/\" && request.method === \"GET\") { return ok(); }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("w.ts", src);
    assert_eq!(keys(&out), vec!["GET /api/x"]);
}

// -- Pre-gate: no `.pathname` anywhere in the file --

#[test]
fn pre_gate_no_pathname_in_file_yields_empty() {
    assert!(extract_pathname_dispatch_provides("w.ts", "export const x = 1;\n").is_empty());
    let plain_server = concat!(
        "function dispatch(request: Request) {\n",
        "  return new Response(\"ok\");\n",
        "}\n"
    );
    assert!(extract_pathname_dispatch_provides("w.ts", plain_server).is_empty());
}

// -- Browser service-worker veto, and the Cloudflare Worker it must NOT catch ------------------

/// The FP this veto exists for: a service-worker fetch helper passes both per-function evidence
/// gates (a `request` param, a locally constructed `URL`) and used to emit its offline/cache
/// routes as public endpoints.
#[test]
fn a_browser_service_worker_emits_nothing() {
    let src = concat!(
        "self.addEventListener(\"install\", (e) => self.skipWaiting());\n",
        "self.addEventListener(\"fetch\", (e) => e.respondWith(onFetch(e.request)));\n",
        "function onFetch(request) {\n",
        "  const url = new URL(request.url);\n",
        "  if (url.pathname === \"/offline\") { return caches.match(\"/offline.html\"); }\n",
        "  if (url.pathname === \"/api/data\") { return fetch(request); }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("sw.ts", src);
    assert!(
        out.is_empty(),
        "a service worker is not a server; its cache routes must not enter the join: {:?}",
        keys(&out)
    );
}

/// The direction that would have cost more than the FP: a LEGACY-SYNTAX Cloudflare Worker registers
/// its entry point with the same `addEventListener("fetch", ...)` call a service worker uses, so the
/// veto this module's doc once proposed (keying on `self.addEventListener`) would have silenced one
/// of this adapter's primary targets. The lifecycle markers are what discriminate — a Worker has no
/// registration to skip and no clients to claim.
#[test]
fn a_legacy_syntax_cloudflare_worker_is_not_vetoed() {
    let src = concat!(
        "addEventListener(\"fetch\", (event) => event.respondWith(handle(event.request)));\n",
        "async function handle(request: Request) {\n",
        "  const url = new URL(request.url);\n",
        "  if (url.pathname === \"/api/users\" && request.method === \"GET\") { return ok(); }\n",
        "}\n"
    );
    let out = extract_pathname_dispatch_provides("worker.ts", src);
    assert_eq!(
        keys(&out),
        vec!["GET /api/users"],
        "a legacy Cloudflare Worker shares the service worker's registration call and must still be seen"
    );
}

/// `install` must be vetoed only as a LISTENER NAME. A server file that happens to mention an
/// installer identifier is still a server, and a substring veto on the bare word would take it out.
#[test]
fn a_bare_install_identifier_does_not_veto_a_server() {
    let src = concat!(
        "function installDeps() {}\n",
        "function dispatch(request: Request) {\n",
        "  const url = new URL(request.url);\n",
        "  if (url.pathname === \"/api/x\" && request.method === \"GET\") { return ok(); }\n",
        "}\n"
    );
    assert_eq!(
        keys(&extract_pathname_dispatch_provides("w.ts", src)),
        vec!["GET /api/x"]
    );
}
