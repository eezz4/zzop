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
//! handling and `base-carrier head-drop`'s opaque-base rule). Recognized values are a string literal,
//! a zero-interpolation template, or — since 2026-08-08 — a **bare identifier this file binds exactly
//! once to such a literal** (`const API_BASE = '/api'; axios.defaults.baseURL = API_BASE`). That last
//! shape is the common one in the wild, and while it was unrecognized this whole channel stayed dark
//! on most real trees. It is a READ, not an inference: the hop reuses
//! [`super::egress::local_consts::LocalConsts`], so the same gates that let a URL argument resolve
//! against a same-file constant apply here — bound exactly once in this file, never reassigned, no
//! parameter shadow, plain literal initializer.
//!
//! Everything else still emits nothing, per the repo's never-guess IO convention — a wrong prefix
//! would mis-key every axios consume in the tree. `axios.defaults.baseURL = settings.baseApiUrl`, a
//! name bound twice, and `const API_BASE = process.env.API_BASE` all stay unresolved (an environment
//! base enters by injection, never by inference) and ride the existing disclosure path
//! (route-near-miss / prefix-drift) or an adapter overlay.
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

use super::egress::local_consts::LocalConsts;

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
    // The same-file one-hop map, sharing `egress`'s gates rather than re-collecting (see
    // `LocalConsts::literal_for_name`). The project-wide DOTTED map is deliberately empty here:
    // this extractor is per-file by signature, and that map admits no bare names anyway.
    let locals = LocalConsts::build(&module, &Default::default(), cm_ref);
    let mut c = ClientBaseCollector {
        cm: cm_ref,
        locals: &locals,
        file: rel,
        found: false,
        out: None,
    };
    module.visit_with(&mut c);
    c.out
}

struct ClientBaseCollector<'a> {
    cm: &'a SourceMap,
    locals: &'a LocalConsts,
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
            // The value inline, or ONE hop through a same-file binding. Almost nobody writes the base
            // at the assignment — they bind it once above and assign the binding — and that value is
            // written down in this file, so reading it is not inference. The hop is gated by
            // `LocalConsts`' own rules (bound exactly once here, never reassigned, no parameter
            // shadow, plain literal initializer), which is why this reuses that map instead of
            // collecting its own: a second set of gates is how "reads the value" and "proves the
            // value is readable" drift apart.
            let resolved = base_url_value_to_path(&n.right).or_else(|| {
                let Expr::Ident(id) = &*n.right else {
                    return None;
                };
                let literal = self.locals.literal_for_name(id.sym.as_ref())?;
                base_path_from_string(literal)
            });
            if let Some(path) = resolved {
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
