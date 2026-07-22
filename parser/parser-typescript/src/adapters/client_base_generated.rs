//! Generated-client base-path marker (`generated-client-base-v1`) — the CONSUME-side base prefix for
//! the swagger codegen client whose calls [`super::egress::generated_client`] recognizes (`client ==
//! "generated"`). swagger-typescript-api emits an `HttpClient` class carrying its server base as a field
//! (`public baseUrl: string = "https://api.example.com/api"`) and builds every request URL as
//! `` `${baseUrl || this.baseUrl}${path}` ``, so the descriptor `path` the egress pass extracts
//! (`/articles`) is only the SUFFIX — the effective route is `<baseUrl-path>/articles`.
//!
//! Rides the SAME `"client-base-prefix"` sentinel channel as [`super::client_base`]'s axios recognizer,
//! tagged `client: "generated"` instead of `"axios"`; `zzop_engine`'s `apply_client_base_prefixes` is
//! already client-generic, so it prepends this path onto exactly the `client == "generated"` http
//! consumes and strips the sentinel before output/join. The base's PATH PART only (host stripped —
//! deploy config, not contract), via the shared [`super::client_base::base_url_value_to_path`], so an
//! absolute `"https://api.example.com/api"` and a bare `"/api"` normalize to the same `"/api"`. Only a
//! string-literal value is recognized; a runtime/env base (`this.baseUrl = process.env.X`) or an empty
//! default (`baseUrl = ""`) yields nothing — never guessed, per the repo's IO convention.
//!
//! ## Recognition gate (deliberately narrow — mirrors the axios recognizer's exact-chain tightness)
//! A `baseUrl`/`baseURL` string-literal CLASS FIELD counts ONLY when its class ALSO declares a
//! `request` member — the swagger `HttpClient` hallmark (`request = <T>(…) => …`) that the egress
//! recognizer keys its `this.request({…})` calls off. This ties the base to the ACTUAL generated client
//! rather than any class that happens to name a field `baseUrl`: without the gate, an unrelated
//! config/settings class with a literal `baseUrl` could become the sole `"generated"` sentinel and
//! silently mis-prefix every generated consume in a tree whose real client base is non-literal. A bare
//! `const baseUrl = "…"` and a `baseUrl` field on a request-less class are both left alone. First
//! qualifying class by AST order wins (one marker per file, mirroring the axios recognizer).

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{Class, ClassMember, PropName};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoConsume;

use super::client_base::{base_url_value_to_path, CLIENT_BASE_PREFIX_KIND};

/// Scans one TS file for the swagger `HttpClient` base field (a `baseUrl`/`baseURL` string-literal class
/// field on a class that also declares a `request` member) and, when it carries a non-empty path part,
/// returns a `"client-base-prefix"` sentinel `IoConsume` tagged `client: "generated"`. `None` for no
/// such class, a non-literal/host-only/empty value, or a parse failure.
pub fn extract_generated_client_base_prefix_marker(rel: &str, text: &str) -> Option<IoConsume> {
    let (cm, module) = crate::parse_with_cm(rel, text)?;
    let cm_ref: &SourceMap = &cm;
    let mut c = GeneratedBaseCollector {
        cm: cm_ref,
        file: rel,
        found: false,
        out: None,
    };
    module.visit_with(&mut c);
    c.out
}

fn is_base_url_name(sym: &str) -> bool {
    sym == "baseUrl" || sym == "baseURL"
}

/// Whether a class member is a `request` method or `request = …` field — the generated `HttpClient`
/// hallmark that gates base recognition (see module doc).
fn is_request_member(m: &ClassMember) -> bool {
    match m {
        ClassMember::Method(mm) => matches!(&mm.key, PropName::Ident(k) if k.sym == "request"),
        ClassMember::ClassProp(p) => matches!(&p.key, PropName::Ident(k) if k.sym == "request"),
        _ => false,
    }
}

struct GeneratedBaseCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    /// True once the first qualifying class (one with a `request` member) has been inspected — gates
    /// further search whether or not its `baseUrl` field resolved to a marker (mirrors the axios one).
    found: bool,
    out: Option<IoConsume>,
}

impl Visit for GeneratedBaseCollector<'_> {
    fn visit_class(&mut self, class: &Class) {
        if !self.found && class.body.iter().any(is_request_member) {
            self.found = true;
            for m in &class.body {
                let ClassMember::ClassProp(p) = m else {
                    continue;
                };
                let (PropName::Ident(key), Some(value)) = (&p.key, &p.value) else {
                    continue;
                };
                if !is_base_url_name(&key.sym) {
                    continue;
                }
                if let Some(path) = base_url_value_to_path(value) {
                    self.out = Some(IoConsume {
                        kind: CLIENT_BASE_PREFIX_KIND.to_string(),
                        key: Some(path),
                        file: self.file.to_string(),
                        line: crate::line_of(self.cm, p.span.lo),
                        raw: None,
                        method: None,
                        retry_configured: None,
                        body: None,
                        client: Some("generated".to_string()),
                    });
                }
                break; // first baseUrl field of the generated client wins
            }
        }
        class.visit_children_with(self);
    }
}

#[cfg(test)]
mod tests {
    use super::extract_generated_client_base_prefix_marker;

    // A swagger `HttpClient` shell: a `request` member is the recognition gate.
    fn http_client(body: &str) -> String {
        format!("export class HttpClient {{\n  request = () => {{}};\n{body}\n}}\n")
    }

    #[test]
    fn swagger_httpclient_base_url_class_field_absolute_url_yields_path_part() {
        let src = http_client("  public baseUrl: string = \"https://api.realworld.show/api\";");
        let m = extract_generated_client_base_prefix_marker("api.ts", &src).expect("marker");
        assert_eq!(m.kind, "client-base-prefix");
        assert_eq!(m.key.as_deref(), Some("/api"));
        assert_eq!(m.client.as_deref(), Some("generated"));
    }

    #[test]
    fn bare_relative_base_path_is_recognized() {
        let src = http_client("  baseURL = \"/api/v2\";");
        let m = extract_generated_client_base_prefix_marker("api.ts", &src).expect("marker");
        assert_eq!(m.key.as_deref(), Some("/api/v2"));
    }

    #[test]
    fn host_only_base_yields_no_marker() {
        let src = http_client("  baseUrl = \"https://api.example.com\";");
        assert!(extract_generated_client_base_prefix_marker("api.ts", &src).is_none());
    }

    #[test]
    fn empty_default_base_yields_no_marker() {
        let src = http_client("  baseUrl = \"\";");
        assert!(extract_generated_client_base_prefix_marker("api.ts", &src).is_none());
    }

    #[test]
    fn non_literal_base_is_not_guessed() {
        let src = http_client("  baseUrl = defaultBase;");
        assert!(extract_generated_client_base_prefix_marker("api.ts", &src).is_none());
    }

    #[test]
    fn a_base_url_field_on_a_class_without_a_request_member_is_not_recognized() {
        // The FP fix: an unrelated config/settings class with a `baseUrl` field is NOT the generated
        // client, so its base must never become the `"generated"` sentinel and mis-prefix real consumes.
        let src =
            "class Settings {\n  public baseUrl: string = \"https://cfg.example.com/api\";\n}\n";
        assert!(extract_generated_client_base_prefix_marker("settings.ts", src).is_none());
    }

    #[test]
    fn a_bare_const_base_url_is_not_a_generated_client_base() {
        let src = "const baseUrl = \"https://api.example.com/api\";\n";
        assert!(extract_generated_client_base_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn first_qualifying_class_wins() {
        let src = format!(
            "{}{}",
            http_client("  baseUrl = \"/api\";"),
            http_client("  baseUrl = \"/other\";")
        );
        let m = extract_generated_client_base_prefix_marker("api.ts", &src).expect("marker");
        assert_eq!(m.key.as_deref(), Some("/api"));
    }
}
