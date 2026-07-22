//! The per-tree extraction entry point and call-site visitor: walks every file, matches recognized
//! HTTP call shapes, resolves URL variants, and emits `IoConsume` entries.

use std::collections::{HashMap, HashSet};

use swc_core::common::SourceMap;
use swc_core::ecma::ast::CallExpr;
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoConsume;

use super::angular::{angular_http_client_receivers, match_angular_http_call};
use super::body_shape::witnessed_body_shape;
use super::consts::build_const_map;
use super::correlation::method_url_pairs;
use super::generated_client::match_generated_client_call;
use super::keying::consume_key_for;
use super::matchers::match_http_call;
use super::react_query::{imports_react_query, match_react_query_call};
use super::retry::{file_wires_retry, is_retry_wrapper_call, is_write_verb};
use super::url_resolve::{expr_text, resolve_url_variants};

/// Extract HTTP egress IoConsume entries across all files (the const map is project-wide).
pub fn extract_http_egress(files: &[(String, String)]) -> Vec<IoConsume> {
    let consts = build_const_map(files);
    let mut out = Vec::new();
    for (rel, text) in files {
        let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
            continue;
        };
        let cm_ref: &SourceMap = &cm;
        let angular_receivers = angular_http_client_receivers(&module);
        let react_query_file = imports_react_query(&module);
        let retry_file = file_wires_retry(&module);
        let mut c = EgressCollector {
            cm: cm_ref,
            file: rel,
            consts: &consts,
            angular_receivers: &angular_receivers,
            react_query_file,
            retry_file,
            retry_depth: 0,
            out: Vec::new(),
        };
        module.visit_with(&mut c);
        out.extend(c.out);
    }
    out
}

struct EgressCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    consts: &'a HashMap<String, String>,
    angular_receivers: &'a HashSet<String>,
    react_query_file: bool,
    /// File imports `axios-retry` — its write egress calls run under transparent retry (`egress-retry-v1`).
    retry_file: bool,
    /// Depth of enclosing retry-wrapper calls (`pRetry(...)`, `backOff(...)`, …) at the current visit
    /// point; `> 0` means a write egress call here is retry-exposed. See [`super::retry`].
    retry_depth: u32,
    out: Vec<IoConsume>,
}

impl Visit for EgressCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        // A retry wrapper (`pRetry(() => …)`, `backOff(…)`) marks its subtree retry-exposed; bump/restore.
        let is_retry_wrapper = is_retry_wrapper_call(call);
        if is_retry_wrapper {
            self.retry_depth += 1;
        }
        if let Some(hc) = match_http_call(call)
            .or_else(|| match_angular_http_call(call, self.angular_receivers))
            .or_else(|| match_generated_client_call(call))
            .or_else(|| match_react_query_call(call, self.react_query_file))
        {
            // Body-shape evidence is a property of THIS call site (its `args[1]`), independent of which
            // method/URL variant a given emitted IoConsume ends up carrying — computed once and cloned
            // into every emit point below (resolved/unresolved/vetoed alike), per `body-shape-v1`.
            let body = witnessed_body_shape(call, &hc);
            let url_variants = resolve_url_variants(hc.arg, self.consts, self.cm);
            let line = crate::line_of(self.cm, call.span.lo);
            // Retry-exposed (write verbs only, tagged below): inside a retry wrapper (any client), or an
            // `axios-retry`-wired file — but the file gate only patches AXIOS, so a `fetch()` write in the
            // same file is not covered by it (a wrapper still would be).
            let retry_active = self.retry_depth > 0 || (self.retry_file && hc.client == "axios");
            if url_variants.is_empty() {
                // Unresolved: the URL couldn't be resolved against THIS call's own `consts` map at all
                // (no variants produced). One consume PER METHOD so a caller with a wider constant map
                // can re-resolve each method branch independently via [`resolve_raw_path`]; with a single
                // method this is exactly today's one-consume behavior.
                let raw = expr_text(hc.arg, self.cm);
                for method in &hc.methods {
                    self.out.push(IoConsume {
                        client: Some(hc.client.to_string()),
                        body: body.clone(),
                        kind: "http".into(),
                        key: None,
                        file: self.file.into(),
                        line,
                        raw: Some(raw.clone()),
                        method: Some(method.to_uppercase()),
                        retry_configured: (retry_active && is_write_verb(method)).then_some(true),
                    });
                }
            } else {
                // Resolved (>=1 variant): one consume per (method, url) pair that classifies to a key,
                // deduped by key; a pair whose URL variant classifies to nothing (the veto list in
                // `base_relative_path`, etc.) falls back to the unresolved shape above, deduped per
                // method since `raw` is the whole call-arg source text, not per-variant.
                let raw = expr_text(hc.arg, self.cm);
                let mut seen_keys: HashSet<String> = HashSet::new();
                let mut seen_unresolved_methods: HashSet<String> = HashSet::new();
                for (method, url) in method_url_pairs(call, &hc, &url_variants, self.cm) {
                    match consume_key_for(method, url) {
                        Some(key) => {
                            if seen_keys.insert(key.clone()) {
                                self.out.push(IoConsume {
                                    client: Some(hc.client.to_string()),
                                    body: body.clone(),
                                    kind: "http".into(),
                                    key: Some(key),
                                    file: self.file.into(),
                                    line,
                                    raw: None,
                                    method: None,
                                    retry_configured: (retry_active && is_write_verb(method))
                                        .then_some(true),
                                });
                            }
                        }
                        None => {
                            if seen_unresolved_methods.insert(method.clone()) {
                                self.out.push(IoConsume {
                                    client: Some(hc.client.to_string()),
                                    body: body.clone(),
                                    kind: "http".into(),
                                    key: None,
                                    file: self.file.into(),
                                    line,
                                    raw: Some(raw.clone()),
                                    method: Some(method.to_uppercase()),
                                    retry_configured: (retry_active && is_write_verb(method))
                                        .then_some(true),
                                });
                            }
                        }
                    }
                }
            }
        }
        call.visit_children_with(self); // recurse into nested calls
        if is_retry_wrapper {
            self.retry_depth = self.retry_depth.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_http_egress`: HTTP call-site detection, URL resolution
    //! (literal/template/const-indirection), and internal-vs-external classification.
    use super::extract_http_egress;
    use crate::adapters::egress::{files, keys};

    #[test]
    fn captures_internal_axios_string_literal() {
        let out = extract_http_egress(&files(&[("a.tsx", r#"axios.get("/authen/getUserInfo")"#)]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "http");
        assert_eq!(out[0].key.as_deref(), Some("GET /authen/getUserInfo"));
        assert_eq!(out[0].file, "a.tsx");
        assert_eq!(out[0].line, 1);
        assert!(out[0].raw.is_none());
        // No retry context and a GET — never tagged.
        assert_eq!(out[0].retry_configured, None);
    }

    // retry_configured (`egress-retry-v1`) is covered end-to-end — parser tag through cross-layer join —
    // in `crates/engine/tests/analyze_cross_layer_retry_write.rs` (axios-retry file gate, `pRetry(...)`
    // wrapper, read-verb and non-retry negatives). The inline assertion above pins the common untagged
    // case (a plain GET with no retry context).

    #[test]
    fn resolves_cross_file_controlkey_indirection() {
        let out = extract_http_egress(&files(&[
            ("protocol/ControlKey.ts", r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo", getSignout: "/authen/getSignout" } };"#),
            ("Ctx.tsx", "axios.get(ControlKey.AUTHEN.getUserInfo); axios.get(ControlKey.AUTHEN.getSignout);"),
        ]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /authen/getUserInfo".to_string()),
                Some("GET /authen/getSignout".to_string())
            ]
        );
    }

    #[test]
    fn resolves_as_const() {
        let out = extract_http_egress(&files(&[
            (
                "protocol/ControlKey.ts",
                r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo" } } as const;"#,
            ),
            ("Ctx.tsx", "axios.get(ControlKey.AUTHEN.getUserInfo)"),
        ]));
        assert_eq!(out[0].key.as_deref(), Some("GET /authen/getUserInfo"));
    }

    #[test]
    fn derives_method_from_post_and_fetch_options() {
        let out = extract_http_egress(&files(&[
            ("k.ts", r#"const K = { create: "/items/create" };"#),
            (
                "p.tsx",
                r#"axios.post(K.create); fetch("/items/create", { method: "delete" });"#,
            ),
        ]));
        assert_eq!(
            keys(&out),
            vec![
                Some("POST /items/create".to_string()),
                Some("DELETE /items/create".to_string())
            ]
        );
    }

    #[test]
    fn normalizes_template_literal_params() {
        let out = extract_http_egress(&files(&[("t.tsx", "axios.get(`/api/users/${id}/posts`)")]));
        assert_eq!(out[0].key.as_deref(), Some("GET /api/users/{}/posts"));
    }

    #[test]
    fn absolute_url_becomes_a_host_carrying_key_for_the_external_bucket() {
        let out = extract_http_egress(&files(&[(
            "e.tsx",
            r#"axios.get("https://api.stripe.com/v1/charges")"#,
        )]));
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].key.as_deref(),
            Some("GET https://api.stripe.com/v1/charges")
        );
        assert!(out[0].raw.is_none());
    }

    #[test]
    fn marks_dynamic_url_as_null_with_raw() {
        let out = extract_http_egress(&files(&[("d.tsx", "axios.get(buildUrl(x))")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("buildUrl(x)"));
        // Carried for late re-resolution even though `buildUrl(x)` is not itself a dotted chain.
        assert_eq!(out[0].method.as_deref(), Some("GET"));
    }

    #[test]
    fn cross_file_constant_indirection_unresolved_consume_carries_its_method() {
        // Only THIS file is visible, so `ControlKey` never resolves here — but `method` must still be set
        // so a caller with a wider constant map can key the consume once it does resolve.
        let out = extract_http_egress(&files(&[(
            "Ctx.tsx",
            "axios.post(ControlKey.AUTHEN.getUserInfo);",
        )]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("ControlKey.AUTHEN.getUserInfo"));
        assert_eq!(out[0].method.as_deref(), Some("POST"));
    }

    #[test]
    fn ignores_non_http_calls() {
        let out = extract_http_egress(&files(&[("x.ts", r#"foo.get("/a"); console.log("/b");"#)]));
        assert!(out.is_empty());
    }
}
