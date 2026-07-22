//! `axios.defaults.baseURL` base-path marker (`axios-defaults-base-v1`) — a tree-level path prefix
//! the axios runtime joins onto every relative/root-relative axios call, which the per-file egress
//! extractor cannot see (the assignment usually lives in a bootstrap file, the call sites elsewhere).
//!
//! Mirrors `global_prefix.rs`'s sentinel pattern on the CONSUME side: rather than a new fragment channel, this rides the existing `IoFacts.consumes` channel with a sentinel
//! `IoConsume { kind: "client-base-prefix", key: Some(<path part>), client: Some("axios"), .. }`.
//! `zzop-engine`'s assemble pass collects every such sentinel, prepends the path to that tree's
//! axios-tagged (`IoConsume::client == Some("axios")`) http consume keys AFTER late cross-file
//! resolution, and strips the sentinel so it never reaches output or the cross-layer join.
//!
//! Only the base URL's PATH PART is carried — the host is deliberately ignored (deploy config, not
//! contract: the same effective-URL stance as the openapi example adapter's `servers[].url`
//! handling and `base-carrier head-drop`'s opaque-base rule). Only a string-literal (or
//! zero-interpolation template) value is recognized; `axios.defaults.baseURL = settings.baseApiUrl`
//! or any other non-literal expression emits nothing, per the repo's never-guess IO convention — a
//! wrong prefix would mis-key every axios consume in the tree. Non-literal bases stay on the
//! existing disclosure path (route-near-miss / prefix-drift) or an adapter overlay.
//!
//! Path-part extraction rule (see [`base_path_from_string`]):
//! - a value carrying `"://"` (an absolute URL) keys off the first `/` AFTER the scheme+host portion
//!   (`"https://api.example.io/api/"` -> `"/api"`); a value with no such `/` is host-only and yields
//!   `None` (prepending nothing is a no-op).
//! - a protocol-relative `"//host/path"` (leading `//`) strips the host like the `://` branch — the
//!   `//` head is a host carrier, never a path — so it keys off the first `/` after the host, checked
//!   BEFORE the `/`-leading rule below (which would otherwise treat `//cdn/api` as a verbatim path).
//! - a value already starting with `/` is itself a path (`"/api"` -> `"/api"`).
//! - any other (relative, non-slash) string (`"api/"`) is refused (`None`) — axios resolves that
//!   shape against the current page URL, not deterministically against this tree's routes.
//! - a trailing `/` is trimmed; an empty result (host-only base, or a bare `"/"`) yields `None`.
//! - a path containing `?` or `#` after trimming is refused (`None`) — a query/fragment in a base
//!   URL is a degenerate config, not a normalizable prefix.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    AssignExpr, AssignOp, AssignTarget, Expr, Lit, MemberProp, SimpleAssignTarget,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoConsume;

/// The sentinel `IoConsume::kind` — assemble consumes and strips it (never joined, never output).
pub const CLIENT_BASE_PREFIX_KIND: &str = "client-base-prefix";

/// Scans one TS file for `axios.defaults.baseURL = <string literal>` and, when the literal carries
/// a non-empty path part, returns a sentinel consume whose `key` is that path (leading `/`,
/// trailing `/` trimmed — `"https://api.example.io/api/"` and `"/api"` both yield `"/api"`) and
/// whose `client` names the client it scopes to (`"axios"`). Returns `None` for: no assignment,
/// non-literal value, a base with no path part (host-only — prepending nothing is a no-op), or a
/// file that fails to parse.
///
/// The assignment may live anywhere in the file (top level or inside any function — the whole tree
/// is walked). Only the FIRST `axios.defaults.baseURL = ...` assignment found (by AST visit order)
/// is considered — whether or not its value turns out to be a recognized literal shape; a second
/// assignment further down never gets a chance to override it (one marker per file, mirroring
/// `global_prefix.rs`'s "only the first matching call is reported" rule).
pub fn extract_client_base_prefix_marker(rel: &str, text: &str) -> Option<IoConsume> {
    let (cm, module) = crate::parse_with_cm(rel, text)?;
    let cm_ref: &SourceMap = &cm;
    let mut c = ClientBaseCollector {
        cm: cm_ref,
        file: rel,
        found: false,
        out: None,
    };
    module.visit_with(&mut c);
    c.out
}

struct ClientBaseCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    /// True once the first `axios.defaults.baseURL = ...` assignment has been seen — gates further
    /// search regardless of whether that first assignment's value resolved to a marker (`out` may
    /// stay `None` even after `found` flips to `true`).
    found: bool,
    out: Option<IoConsume>,
}

impl Visit for ClientBaseCollector<'_> {
    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if !self.found && n.op == AssignOp::Assign && is_axios_defaults_base_url_target(&n.left) {
            self.found = true;
            if let Some(path) = base_url_value_to_path(&n.right) {
                self.out = Some(IoConsume {
                    kind: CLIENT_BASE_PREFIX_KIND.to_string(),
                    key: Some(path),
                    file: self.file.to_string(),
                    line: crate::line_of(self.cm, n.span.lo),
                    raw: None,
                    method: None,
                    retry_configured: None,
                    body: None,
                    client: Some("axios".to_string()),
                });
            }
            return; // first matching assignment wins — never look further, matched or not
        }
        n.visit_children_with(self);
    }
}

/// Whether `left` is the exact member chain `axios.defaults.baseURL` (receiver `axios` must be a
/// bare identifier of that exact name — never guessed via a differently-named import alias).
fn is_axios_defaults_base_url_target(left: &AssignTarget) -> bool {
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = left else {
        return false;
    };
    let MemberProp::Ident(prop) = &m.prop else {
        return false;
    };
    if prop.sym != "baseURL" {
        return false;
    }
    let Expr::Member(inner) = &*m.obj else {
        return false;
    };
    let MemberProp::Ident(inner_prop) = &inner.prop else {
        return false;
    };
    if inner_prop.sym != "defaults" {
        return false;
    }
    matches!(&*inner.obj, Expr::Ident(id) if id.sym == "axios")
}

/// Reads a plain string literal, or a template literal with ZERO interpolations (`` `api` ``, distinct
/// from `` `${x}` ``), as a plain string. Any other expression shape (identifier, member access,
/// concatenation, an interpolated template) is not a recognized value form — `None`, never guessed.
fn literal_or_zero_interp_template(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) if t.exprs.is_empty() => Some(
            t.quasis
                .first()
                .and_then(|q| q.cooked.as_ref())
                .and_then(|a| a.as_str())
                .unwrap_or_default()
                .to_string(),
        ),
        _ => None,
    }
}

/// The assignment's right-hand side, resolved to the sentinel's `key` (the base's path part), or
/// `None` when the value isn't a recognized literal shape or its path part doesn't survive
/// [`base_path_from_string`]'s rules. `pub(crate)` so [`super::client_base_generated`] normalizes a
/// generated-client base by the identical value→path rule (host-strip, path-only, never-guess).
pub(crate) fn base_url_value_to_path(e: &Expr) -> Option<String> {
    let base = literal_or_zero_interp_template(e)?;
    base_path_from_string(&base)
}

/// Extracts the path part of a base-URL string per this module's doc: absolute URL -> path after the
/// scheme+host; protocol-relative (`//host/path`) -> path after the host (the `//` head IS a host
/// carrier, never a path — taking it verbatim would bake the host into every prefixed key, exactly
/// the "host is deploy config, not contract" breach this module exists to avoid); already-a-path
/// (single-`/`-headed) -> itself; anything else (relative, non-slash) -> `None` (never guessed —
/// axios would resolve that against the page URL, not this tree's routes). Trailing `/` trimmed; an
/// empty result (host-only, or bare `/`) or a result still carrying `?`/`#` -> `None`.
fn base_path_from_string(base: &str) -> Option<String> {
    let path = if let Some(scheme_idx) = base.find("://") {
        let after_scheme = &base[scheme_idx + 3..];
        let slash_idx = after_scheme.find('/')?; // host-only (no path segment at all) -> None
        &after_scheme[slash_idx..]
    } else if let Some(after_slashes) = base.strip_prefix("//") {
        // Protocol-relative base (`//cdn.acme.com/api`) — same host-strip as the `://` branch.
        let slash_idx = after_slashes.find('/')?; // host-only -> None
        &after_slashes[slash_idx..]
    } else if base.starts_with('/') {
        base
    } else {
        // Relative, non-slash string (`"api/"`) — axios resolves this against the current page URL,
        // not deterministically against this tree's routes. Never guessed.
        return None;
    };

    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None; // host-only base, or a bare "/" — prepending nothing is a no-op
    }
    if trimmed.contains('?') || trimmed.contains('#') {
        return None; // degenerate config: query/fragment baked into a base URL
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_client_base_prefix_marker`: the absolute-URL and bare-path literal happy
    //! paths (with trailing-slash trimming), the host-only/never-guess refusals (non-literal value,
    //! concat, interpolated template, wrong receiver, wrong property, query/fragment-in-base), the
    //! zero-interpolation template acceptance, and the inside-a-function-body case.
    use super::*;

    #[test]
    fn literal_with_host_and_path_yields_the_path_part() {
        let src = r#"axios.defaults.baseURL = "https://api.example.io/api/";"#;
        let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(m.kind, CLIENT_BASE_PREFIX_KIND);
        assert_eq!(m.key.as_deref(), Some("/api"));
        assert_eq!(m.client.as_deref(), Some("axios"));
        assert_eq!(m.file, "main.ts");
        assert_eq!(m.line, 1);
        assert!(m.raw.is_none() && m.method.is_none() && m.body.is_none());
    }

    #[test]
    fn bare_path_literal_is_kept_as_is() {
        let src = r#"axios.defaults.baseURL = "/api";"#;
        let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(m.key.as_deref(), Some("/api"));
    }

    #[test]
    fn trailing_slash_is_trimmed() {
        let src = r#"axios.defaults.baseURL = "/api/";"#;
        let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(m.key.as_deref(), Some("/api"));
    }

    #[test]
    fn host_only_base_yields_none() {
        let src = r#"axios.defaults.baseURL = "https://api.example.io";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
        let src2 = r#"axios.defaults.baseURL = "https://api.example.io/";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src2).is_none());
    }

    #[test]
    fn zero_interpolation_template_literal_works() {
        let src = "axios.defaults.baseURL = `https://api.example.io/api`;";
        let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(m.key.as_deref(), Some("/api"));
    }

    #[test]
    fn member_expression_value_is_never_guessed() {
        let src = "axios.defaults.baseURL = settings.baseApiUrl;";
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn string_concatenation_value_is_never_guessed() {
        let src = r#"axios.defaults.baseURL = HOST + "/api";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn interpolated_template_value_is_never_guessed() {
        let src = "axios.defaults.baseURL = `${HOST}/api`;";
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn assignment_inside_a_function_body_is_found() {
        let src = r#"
            function setup() {
                axios.defaults.baseURL = "https://api.example.io/api";
            }
        "#;
        let m = extract_client_base_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(m.key.as_deref(), Some("/api"));
    }

    #[test]
    fn unrelated_receiver_is_never_matched() {
        let src = r#"x.defaults.baseURL = "/api";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn unrelated_property_is_never_matched() {
        let src = r#"axios.defaults.headers = { "X-Foo": "1" };"#;
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn query_or_fragment_in_base_is_never_guessed() {
        let src = r#"axios.defaults.baseURL = "https://api.example.io/api?v=1";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
        let src2 = r#"axios.defaults.baseURL = "/api#frag";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src2).is_none());
    }

    #[test]
    fn protocol_relative_base_strips_the_host_like_an_absolute_url() {
        // `//host/api` is a HOST carrier, not a path — taking it verbatim would bake the host into
        // every prefixed key (the exact "host is deploy config, not contract" breach).
        let src = r#"axios.defaults.baseURL = "//cdn.acme.com/api";"#;
        let m = extract_client_base_prefix_marker("main.ts", src).expect("marker");
        assert_eq!(m.key.as_deref(), Some("/api"));
    }

    #[test]
    fn protocol_relative_host_only_base_is_a_no_op() {
        let src = r#"axios.defaults.baseURL = "//cdn.acme.com";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src).is_none());
        let src2 = r#"axios.defaults.baseURL = "//cdn.acme.com/";"#;
        assert!(extract_client_base_prefix_marker("main.ts", src2).is_none());
    }
}
