//! Coverage: one-hop-sink via a sibling helper, fixed-method via param-name suffix, a non-wrapper
//! fn with no reachable sink, call-site argument capture, import-specifier resolution, the volume
//! guard, determinism, and the empty-file case.
use super::extract_wrapper_fragments;
use zzop_core::WrapperDefFragment;

fn def<'a>(defs: &'a [WrapperDefFragment], name: &str) -> &'a WrapperDefFragment {
    defs.iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("no def named {name:?} in {defs:?}"))
}

#[test]
fn one_hop_sink_via_sibling_request_shape() {
    let src = concat!(
        "async function request(config) {\n",
        "  return axios.request(config);\n",
        "}\n",
        "\n",
        "export async function makeRestApiRequest<T>(\n",
        "  context: IRestApiContext,\n",
        "  method: Method,\n",
        "  endpoint: string,\n",
        "  data?: any,\n",
        "): Promise<T> {\n",
        "  const response = await request({ method, baseURL: context.baseUrl, endpoint, data });\n",
        "  return response.data;\n",
        "}\n",
        "\n",
        "export async function getFullApiResponse<T>(\n",
        "  context: IRestApiContext,\n",
        "  method: Method,\n",
        "  endpoint: string,\n",
        "): Promise<T> {\n",
        "  return request({ method, baseURL: context.baseUrl, endpoint });\n",
        "}\n"
    );
    let (defs, _) = extract_wrapper_fragments("apiRequest.ts", src);
    assert_eq!(defs.len(), 2, "{defs:?}");

    let make = def(&defs, "makeRestApiRequest");
    assert_eq!(make.method_param, Some(1));
    assert_eq!(make.path_param, 2);
    assert_eq!(make.fixed_method, None);

    let full = def(&defs, "getFullApiResponse");
    assert_eq!(full.method_param, Some(1));
    assert_eq!(full.path_param, 2);
    assert_eq!(full.fixed_method, None);
}

#[test]
fn fixed_method_wrapper_via_endpoint_suffix_param_name() {
    let src = concat!(
        "export async function streamRequest(ctx, apiEndpoint: string, payload) {\n",
        "  const url = apiEndpoint;\n",
        "  return fetch(url, { method: 'POST', body: JSON.stringify(payload) });\n",
        "}\n"
    );
    let (defs, _) = extract_wrapper_fragments("stream.ts", src);
    let d = def(&defs, "streamRequest");
    assert_eq!(d.method_param, None);
    assert_eq!(d.path_param, 1);
    assert_eq!(d.fixed_method.as_deref(), Some("POST"));
}

#[test]
fn file_private_request_wrapper_below_exported_callers_is_collected_and_keys() {
    // The ping-hub `features/group/api.ts` shape: exported callers pass a method + literal path
    // template to a file-PRIVATE `request(method, path, opts)` wrapper that funnels through `fetch`.
    // Pre-`intra-file-wrapper-v1` the def was export-gated and dropped, so these consumes never keyed.
    let src = concat!(
        "import { apiBase } from '@/lib/apiBase.js';\n",
        "\n",
        "export function getGroupInfo(id: string): Promise<{ name: string }> {\n",
        "  return request('GET', `/api/group/${encodeURIComponent(id)}/info`);\n",
        "}\n",
        "export function extendGroup(id: string, token: string): Promise<{ ok: boolean }> {\n",
        "  return request('POST', `/api/group/${encodeURIComponent(id)}/extend`, { token });\n",
        "}\n",
        "\n",
        "// --- private ---\n",
        "async function request<T>(method: 'GET' | 'POST', path: string, opts = {}): Promise<T> {\n",
        "  const res = await fetch(`${apiBase()}${path}`, { method });\n",
        "  return res.json();\n",
        "}\n"
    );
    let (defs, calls) = extract_wrapper_fragments("features/group/api.ts", src);

    let d = def(&defs, "request");
    assert_eq!(d.method_param, Some(0));
    assert_eq!(d.path_param, 1);
    assert_eq!(d.fixed_method, None);

    // Both callers emit a same-file (specifier: None) call fragment carrying the literal method + path.
    let get = calls
        .iter()
        .find(|c| c.args.first() == Some(&Some("GET".into())))
        .unwrap_or_else(|| panic!("no GET request(...) call in {calls:?}"));
    assert_eq!(get.callee, "request");
    assert_eq!(get.specifier, None);
    assert_eq!(get.args[1].as_deref(), Some("/api/group/{}/info"));

    let post = calls
        .iter()
        .find(|c| c.args.first() == Some(&Some("POST".into())))
        .unwrap_or_else(|| panic!("no POST request(...) call in {calls:?}"));
    assert_eq!(post.args[1].as_deref(), Some("/api/group/{}/extend"));
}

#[test]
fn bare_fetch_wrapper_non_pathlike_param_is_fixed_get() {
    // The S7 cop-out shape: `export function get(p) { return fetch(p) }`. `p` is not name-signalled,
    // so the path param comes from being fetch's first arg; a bare `fetch(url)` (no method key) → GET.
    let src = "export function get(p) {\n  return fetch(p);\n}\n";
    let (defs, _) = extract_wrapper_fragments("api.ts", src);
    let d = def(&defs, "get");
    assert_eq!(d.method_param, None);
    assert_eq!(d.path_param, 0);
    assert_eq!(d.fixed_method.as_deref(), Some("GET"));
}

#[test]
fn fetch_wrapper_non_pathlike_param_with_literal_method_is_fixed_post() {
    // `export function post(p, body) { return fetch(p, { method: 'POST', body }) }` — path via
    // fetch's first arg, verb from the string-literal method option.
    let src = concat!(
        "export function post(p, body) {\n",
        "  return fetch(p, { method: 'POST', body: JSON.stringify(body) });\n",
        "}\n"
    );
    let (defs, _) = extract_wrapper_fragments("api.ts", src);
    let d = def(&defs, "post");
    assert_eq!(d.method_param, None);
    assert_eq!(d.path_param, 0);
    assert_eq!(d.fixed_method.as_deref(), Some("POST"));
}

#[test]
fn fetch_wrapper_with_dynamic_method_emits_no_fixed_fragment() {
    // A present-but-dynamic `method` (a local, not a `method` param) must never be guessed to a verb.
    let src = concat!(
        "export function send(p, verb) {\n",
        "  return fetch(p, { method: verb });\n",
        "}\n"
    );
    let (defs, _) = extract_wrapper_fragments("api.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn refetch_and_prefetch_are_not_fetch_sinks() {
    // BLOCKING (opus): `fetch(` must match at a word boundary, else `refetch(`/`prefetch(` (React Query)
    // are read as fetch sinks and, via the positional-path + GET-default paths, fabricate a `GET`
    // consume for a fire-and-forget query refresh.
    let src = concat!(
        "export function reload(key) {\n  return refetch(key);\n}\n",
        "export function warm(key) {\n  return prefetch(key);\n}\n",
    );
    let (defs, _) = extract_wrapper_fragments("q.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn opaque_fetch_options_do_not_default_to_get() {
    // BLOCKING (opus): `fetch(url, opts)` with an opaque options identifier (which may carry
    // `{ method: 'POST' }`) must NOT be stamped GET — the verb is caller-controlled, so it is guessed.
    let src = "export function send(url, opts) {\n  return fetch(url, opts);\n}\n";
    let (defs, _) = extract_wrapper_fragments("api.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn inline_fetch_options_object_still_defaults_to_get() {
    // The opaque-options guard must NOT over-correct: a transparent inline `{ ... }` with no method key
    // is a legitimate GET.
    let src = "export function get(p) {\n  return fetch(p, { headers: h });\n}\n";
    let (defs, _) = extract_wrapper_fragments("api.ts", src);
    let d = def(&defs, "get");
    assert_eq!(d.fixed_method.as_deref(), Some("GET"));
}

#[test]
fn non_wrapper_fn_calling_fetch_with_literal_url_is_not_a_wrapper() {
    // No path param to identify (fetch's first arg is a literal, not a param) — a direct call site,
    // `egress.rs`'s channel, not a wrapper def.
    let src = "export function refresh() {\n  return fetch('/api/refresh');\n}\n";
    let (defs, _) = extract_wrapper_fragments("api.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn fetch_wrapper_composite_first_arg_falls_to_name_signal_not_guessed() {
    // A composite fetch first arg (`base + id`) is NOT a verbatim param, and `id` is not name-signalled,
    // so no path param resolves — honest miss over a guessed positional.
    let src = "export function load(id) {\n  return fetch(baseUrl + id);\n}\n";
    let (defs, _) = extract_wrapper_fragments("api.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn file_private_non_wrapper_helper_with_path_param_but_no_sink_stays_uncollected() {
    // Broadening def collection to private decls must NOT sweep in a private path-shaped helper that
    // never reaches a fetch/axios/ky sink — the sink gate is the guard against false wrapper defs.
    let src = concat!(
        "function normalizePath(path: string): string {\n",
        "  return path.replace(/\\/+$/, '');\n",
        "}\n"
    );
    let (defs, _) = extract_wrapper_fragments("util.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn exported_fn_with_url_param_but_no_sink_is_not_a_wrapper() {
    let src = concat!(
        "export function buildUrl(base: string, url: string): string {\n",
        "  return base + url;\n",
        "}\n"
    );
    let (defs, _) = extract_wrapper_fragments("build-url.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn external_host_sink_disqualifies_the_wrapper() {
    // A per-service fetcher with an absolute-URL sink: call sites pass internal-LOOKING paths
    // that actually leave the system — see the external-host veto in `classify_def`.
    let src = concat!(
        "export const fetcher = async (endpoint: string, init?: RequestInit) => {\n",
        "  return fetch(`https://api.example.com/v1${endpoint}`, { method: \"GET\", ...init });\n",
        "};\n"
    );
    let (defs, _) = extract_wrapper_fragments("exampleApiFetcher.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn ambiguous_fixed_method_two_distinct_verbs_is_not_a_wrapper() {
    let src = concat!(
        "export function poll(url: string, mode: string) {\n",
        "  if (mode === 'a') { return fetch(url, { method: 'GET' }); }\n",
        "  return fetch(url, { method: 'POST' });\n",
        "}\n"
    );
    let (defs, _) = extract_wrapper_fragments("poll.ts", src);
    assert!(defs.is_empty(), "{defs:?}");
}

#[test]
fn call_site_positional_capture_literal_and_template_and_specifier() {
    let src = concat!(
        "import { makeRestApiRequest } from './helper';\n",
        "function useThing() {\n",
        "  makeRestApiRequest(context, 'GET', '/workflows/new');\n",
        "  makeRestApiRequest(ctx, 'POST', `/workflows/${id}/activate`, data);\n",
        "}\n"
    );
    let (_, calls) = extract_wrapper_fragments("use-thing.ts", src);
    assert_eq!(calls.len(), 2, "{calls:?}");

    assert_eq!(calls[0].callee, "makeRestApiRequest");
    assert_eq!(calls[0].specifier.as_deref(), Some("./helper"));
    assert_eq!(
        calls[0].args,
        vec![None, Some("GET".into()), Some("/workflows/new".into())]
    );
    assert_eq!(calls[0].line, 3);

    assert_eq!(calls[1].specifier.as_deref(), Some("./helper"));
    assert_eq!(
        calls[1].args,
        vec![
            None,
            Some("POST".into()),
            Some("/workflows/{}/activate".into()),
            None,
        ]
    );
}

#[test]
fn local_callee_has_no_specifier() {
    let src = "function f() {\n  localHelper('GET', '/x');\n}\n";
    let (_, calls) = extract_wrapper_fragments("local.ts", src);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].callee, "localHelper");
    assert_eq!(calls[0].specifier, None);
}

#[test]
fn call_with_no_verb_and_no_slash_arg_is_not_captured() {
    let src = concat!(
        "function f(x, y) {\n",
        "  noop(x, y);\n",
        "  helper('abc', 'def');\n",
        "}\n"
    );
    let (_, calls) = extract_wrapper_fragments("skip.ts", src);
    assert!(calls.is_empty(), "{calls:?}");
}

#[test]
fn deterministic_across_repeated_extractions() {
    let src = concat!(
        "async function request(config) {\n",
        "  return axios.request(config);\n",
        "}\n",
        "export async function makeRestApiRequest(context, method: Method, endpoint: string) {\n",
        "  return request({ method, endpoint });\n",
        "}\n",
        "makeRestApiRequest(ctx, 'GET', '/a');\n"
    );
    let a = extract_wrapper_fragments("d.ts", src);
    let b = extract_wrapper_fragments("d.ts", src);
    assert_eq!(a, b);
}

#[test]
fn empty_file_yields_no_fragments() {
    let (defs, calls) = extract_wrapper_fragments("e.ts", "");
    assert!(defs.is_empty());
    assert!(calls.is_empty());
}
