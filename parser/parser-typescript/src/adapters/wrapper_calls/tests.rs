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
