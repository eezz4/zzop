//! Attribute-routed ASP.NET Core controllers — see the parent module doc (`mod.rs`) for scope. Walks
//! the same class-NESTING structure `lang::symbols` walks (namespace-transparent, type-nesting only),
//! but ONLY `class_declaration` is ever treated as a controller candidate (structs/interfaces/records
//! are never real ASP.NET controllers in practice — v1 narrowing, not attempted for other type kinds).
//!
//! ## Class gating (task-pinned)
//! A class is a controller when it carries `[ApiController]` or `[Controller]` (bare or with any
//! arguments — the attribute's own presence is the whole signal), OR its simple name ends in
//! `"Controller"` (ASP.NET's own naming-convention discovery rule). A NESTED class gates INDEPENDENTLY
//! of its enclosing class — its own attributes/name only, mirroring
//! `zzop_parser_java_21::provides::extract`'s identical "nested type gates independently" rule.
//!
//! ## Base path (`[Route("api/[controller]")]`)
//! A class-level `[Route(...)]` attribute's first quoted-string argument, with the literal token
//! `[controller]` replaced by the class's own simple name MINUS a trailing `"Controller"` suffix,
//! LOWERCASED (task brief: "match ASP.NET's lowercase convention" — this is ASP.NET's OWN default
//! token-replacement casing, not an invented approximation). No class-level `[Route]` at all -> empty
//! prefix (methods use their own path alone). A NON-LITERAL `[Route]` argument (a `const`-string
//! reference, `[Route(ApiRoutes.Base)]`) IS valid C# — attribute arguments must be compile-time constants,
//! and a named `const string` is one. This module only reads quoted literals, so a non-literal prefix
//! cannot be resolved here: the class's own routes are BLOCKED (`class_route_prefix` returns `None`)
//! rather than keyed at the empty base, which would fabricate phantoms under the wrong (missing) prefix —
//! the same honest-drop the Java side takes for a `PrefixState::Unresolved` class prefix
//! (`zzop_parser_java_21::project::resolve`).
//!
//! ## Method-level route composition
//! `method_route` (below) recognizes `[HttpGet]`/`[HttpPost]`/`[HttpPut]`/`[HttpDelete]`/`[HttpPatch]`/
//! `[HttpHead]` (verb implied by the attribute name) and `[Route("x")]` (no verb of its own). A method
//! with NO recognized verb attribute at all — including one carrying ONLY a bare `[Route]` with no
//! accompanying `HttpX` (ASP.NET's `[AcceptVerbs]` form is not implemented — roadmap) — is AMBIGUOUS
//! and skipped, mirroring `zzop_parser_java_21::provides::annotations::method_route`'s identical
//! `@RequestMapping`-with-no-`method`-attribute skip. When both an `HttpX` attribute AND a `[Route]`
//! attribute are present on the SAME method, the `HttpX` attribute's own path argument wins (its
//! absence, i.e. a bare `[HttpGet]`, falls back to the co-located `[Route]`'s path) — the full path is
//! `{class prefix}/{method path}`, joined with a single `/` (`http_interface_key`'s own slash-collapse
//! normalization makes the join exact regardless of leading/trailing slashes on either side, the same
//! `format!("{prefix}/{path}")` convention `zzop_parser_java_21::provides::extract::walk_member` uses).
//!
//! Path resolution is TRI-STATE (`attr_path_state`, the C# parallel of the Java
//! `provides::annotations::RoutePathState`): a quoted literal keys the route, a genuinely ABSENT path (a
//! bare `[HttpGet]`) is the base route `""`, but a NON-LITERAL path (`[HttpGet(Routes.List)]`) is UNKNOWN
//! — the route is DROPPED, never keyed at the empty base. Collapsing a non-literal path to `""` used to
//! fabricate a phantom base route and lose the real one.

use std::sync::OnceLock;

use regex::Regex;
use tree_sitter::Node;
use zzop_core::{http_interface_key, IoProvide};

use crate::util::{
    attribute_name, attribute_raw_args, attributes_of, line_of, node_text, valid_named_children,
};

/// Method-level mapping attribute name -> the HTTP verb it implies. `HttpHead` is pinned OUTSIDE
/// `zzop_core::HTTP_KEY_VERBS`'s five-verb vocabulary but still emitted verbatim — the same deliberate
/// divergence `zzop_parser_go::adapters::http_clients::VERB_METHODS`'s doc documents for `net/http`'s
/// own `Head`: an explicit attribute name is a witnessed fact, not a name-shaped guess.
const METHOD_ATTRIBUTES: &[(&str, &str)] = &[
    ("HttpGet", "GET"),
    ("HttpPost", "POST"),
    ("HttpPut", "PUT"),
    ("HttpDelete", "DELETE"),
    ("HttpPatch", "PATCH"),
    ("HttpHead", "HEAD"),
];

pub(super) fn extract(rel: &str, root: Node, src: &str, out: &mut Vec<IoProvide>) {
    walk_scope(rel, root, src, out);
}

/// Recurses through a block `namespace_declaration`'s own body transparently (`lang::symbols`'
/// identical namespace-transparent scope), dispatching each `class_declaration` found.
fn walk_scope(rel: &str, node: Node, src: &str, out: &mut Vec<IoProvide>) {
    for child in valid_named_children(node) {
        match child.kind() {
            "namespace_declaration" => {
                if let Some(body) = child.child_by_field_name("body") {
                    walk_scope(rel, body, src, out);
                }
            }
            "class_declaration" => walk_class(rel, child, src, out),
            _ => {}
        }
    }
}

/// One class's own gating facts (never an ancestor's — module doc), then its direct members. A nested
/// `class_declaration` inside this class's body is walked independently via the same fn (module doc's
/// "nested class gates independently").
fn walk_class(rel: &str, node: Node, src: &str, out: &mut Vec<IoProvide>) {
    let attrs = attributes_of(node);
    let simple_name = node
        .child_by_field_name("name")
        .map(|n| node_text(n, src))
        .unwrap_or("");
    let is_controller = attrs.iter().any(|a| {
        matches!(
            attribute_name(*a, src).as_deref(),
            Some("ApiController") | Some("Controller")
        )
    }) || simple_name.ends_with("Controller");
    let prefix = class_route_prefix(&attrs, src, simple_name);

    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    for member in valid_named_children(body) {
        if member.kind() == "class_declaration" {
            walk_class(rel, member, src, out);
            continue;
        }
        if !is_controller
            || !matches!(
                member.kind(),
                "method_declaration" | "constructor_declaration"
            )
        {
            continue;
        }
        let Some(prefix) = prefix.as_deref() else {
            // Class prefix is a non-literal `[Route(CONST)]` this pass cannot resolve -> its own routes'
            // full paths are unknown. Skip rather than key them under the empty base (a phantom). Nested
            // classes were already recursed above, gating independently.
            continue;
        };
        let Some((verb, path)) = method_route(&attributes_of(member), src) else {
            continue;
        };
        let full_path = format!("{prefix}/{path}");
        out.push(IoProvide {
            kind: "http".to_string(),
            key: http_interface_key(&verb, &full_path),
            file: rel.to_string(),
            line: line_of(member),
            symbol: member
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string()),
            body: None,
        });
    }
}

/// The class-level `[Route]` base prefix: `Some(prefix)` (possibly `""` when no `[Route]` attribute is
/// present at all — methods then use their own path alone), or `None` when a `[Route]` IS present but its
/// argument is a NON-LITERAL constant reference this pass cannot resolve — a signal to the caller to BLOCK
/// the class's own routes rather than key them under a wrong (missing) prefix (module doc's "Base path").
fn class_route_prefix(attrs: &[Node], src: &str, simple_name: &str) -> Option<String> {
    let Some(route_attr) = attrs
        .iter()
        .find(|a| attribute_name(**a, src).as_deref() == Some("Route"))
    else {
        return Some(String::new());
    };
    let args = attribute_raw_args(*route_attr, src).unwrap_or_default();
    match attr_path_state(&args) {
        PathState::Literal(raw_path) => Some(substitute_controller_token(&raw_path, simple_name)),
        // `[Route()]` with no argument is a degenerate empty prefix, not a block.
        PathState::Absent => Some(String::new()),
        PathState::NonLiteral(_) => None,
    }
}

/// Replaces the ASP.NET `[controller]` route token with the class's own simple name MINUS a trailing
/// `"Controller"` suffix, LOWERCASED (ASP.NET's own default token-replacement casing — module doc's
/// "Base path"). Shared by the per-file `class_route_prefix` above and the whole-corpus pass
/// (`crate::project`), which applies the SAME substitution AFTER resolving a non-literal prefix constant.
pub(crate) fn substitute_controller_token(raw_path: &str, simple_name: &str) -> String {
    let base = simple_name
        .strip_suffix("Controller")
        .unwrap_or(simple_name);
    raw_path.replace("[controller]", &base.to_lowercase())
}

/// The tri-state a route attribute's raw argument text resolves to on the PATH axis — the C# parallel of
/// `zzop_parser_java_21::provides::annotations::RoutePathState`.
pub(crate) enum PathState {
    /// A quoted path literal (`[HttpGet("x")]`, `[Route("api/[controller]")]`).
    Literal(String),
    /// No argument at all — a bare attribute (`[HttpGet]`). The base route / empty prefix.
    Absent,
    /// An argument is present but is NOT a quoted literal — a `const`-string reference
    /// (`[HttpGet(Routes.List)]`). Carries the raw argument text so the WHOLE-CORPUS pass (`crate::project`)
    /// can resolve the constant against the corpus, exactly as Java's `RoutePathState::NonLiteral(String)`
    /// does; the per-file pass, having no corpus, still drops the route / blocks the class prefix.
    NonLiteral(String),
}

/// Classifies a route attribute's raw args into [`PathState`]: a quoted literal wins, empty args are the
/// bare-attribute base, and any other non-empty text is a non-literal constant reference (carried forward
/// verbatim for whole-corpus resolution).
pub(crate) fn attr_path_state(args: &str) -> PathState {
    if let Some(s) = first_quoted_string(args) {
        return PathState::Literal(s);
    }
    if args.trim().is_empty() {
        PathState::Absent
    } else {
        PathState::NonLiteral(args.to_string())
    }
}

/// Reads `attrs` for a `(VERB, path-STATE)` route — the raw tri-state both callers act on, the C# parallel
/// of `zzop_parser_java_21::provides::annotations::method_route_states`. `None` when the method is
/// AMBIGUOUS (no recognized verb attribute). A LITERAL template on either attribute is a known route: the
/// `HttpX` template wins when it is literal, but a literal `[Route]` still surfaces the endpoint when the
/// `HttpX` template is non-literal (both attributes register routes in ASP.NET, and we can only key the one
/// we can read). When no literal is available anywhere but a NON-LITERAL path IS present, the resulting
/// `NonLiteral` carries its raw args forward — the per-file [`method_route`] drops it (no corpus), the
/// whole-corpus pass (`crate::project`) resolves the constant. Both absent -> a bare verb, the base route.
pub(crate) fn method_route_state(attrs: &[Node], src: &str) -> Option<(String, PathState)> {
    let mut verb: Option<&str> = None;
    let mut http_state = PathState::Absent;
    let mut route_state = PathState::Absent;
    for attr in attrs {
        let Some(name) = attribute_name(*attr, src) else {
            continue;
        };
        let args = attribute_raw_args(*attr, src).unwrap_or_default();
        if let Some((_, v)) = METHOD_ATTRIBUTES.iter().find(|(n, _)| *n == name) {
            verb = Some(v);
            http_state = attr_path_state(&args);
        } else if name == "Route" {
            route_state = attr_path_state(&args);
        }
    }
    let verb = verb?;
    let path = match (http_state, route_state) {
        (PathState::Literal(p), _) => PathState::Literal(p),
        (_, PathState::Literal(p)) => PathState::Literal(p),
        (PathState::NonLiteral(a), _) => PathState::NonLiteral(a),
        (_, PathState::NonLiteral(a)) => PathState::NonLiteral(a),
        (PathState::Absent, PathState::Absent) => PathState::Absent,
    };
    Some((verb.to_string(), path))
}

/// The PER-FILE pass's view: reads `attrs` for a `(VERB, path)` route — `None` when the method is
/// AMBIGUOUS (no recognized verb attribute) OR when its resolved path is a NON-LITERAL constant reference
/// (unknown — dropped rather than keyed at the empty base; the whole-corpus pass resolves it instead).
fn method_route(attrs: &[Node], src: &str) -> Option<(String, String)> {
    let (verb, state) = method_route_state(attrs, src)?;
    match state {
        PathState::Literal(p) => Some((verb, p)),
        PathState::Absent => Some((verb, String::new())),
        PathState::NonLiteral(_) => None,
    }
}

/// The first `"..."` literal found in `args` — verbatim port of
/// `zzop_parser_java_21::provides::annotations::first_quoted_string`.
pub(crate) fn first_quoted_string(args: &str) -> Option<String> {
    quoted_string_re().captures(args).map(|c| c[1].to_string())
}

fn quoted_string_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""([^"]*)""#).unwrap())
}

#[cfg(test)]
mod tests {
    /// Parity pin with `zzop_parser_go::adapters::http_clients::VERB_METHODS`'s HEAD carve-out (see
    /// `METHOD_ATTRIBUTES`'s doc): every emitted verb must be a `zzop_core::HTTP_KEY_VERBS` join member
    /// except the deliberate `HttpHead`->`HEAD` divergence (a witnessed attribute, not a name-shaped
    /// guess). The pin fails if the carve-out disappears (update doc + pin together) or a stray verb creeps in.
    #[test]
    fn method_attributes_are_core_key_verbs_plus_deliberate_head() {
        for (_, verb) in super::METHOD_ATTRIBUTES {
            assert!(
                zzop_core::HTTP_KEY_VERBS.contains(verb) || *verb == "HEAD",
                "METHOD_ATTRIBUTES emits {verb}, neither a core HTTP_KEY_VERBS member nor the pinned HEAD carve-out"
            );
        }
        assert!(
            super::METHOD_ATTRIBUTES.iter().any(|(_, v)| *v == "HEAD"),
            "the HEAD carve-out disappeared — update the doc + this pin together"
        );
    }
}
