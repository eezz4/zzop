//! Annotation-shape classification for the Spring provides pass — see the parent module doc
//! (`provides/mod.rs`) for the recognized annotation vocabulary and never-guess rules. AST-grade
//! REIMPLEMENTATION of the old lexical `zzop_parser_java::provides::annotations` module: each
//! `annotation`/`marker_annotation` is read directly off `util::annotations_of` (already segmented and
//! balanced by the real grammar) instead of a regex-scanned raw text block, so the old crate's TWO
//! documented lexical limits no longer apply here — a PRECISION GAIN over the old crate, not a parity
//! requirement:
//! - An annotation argument containing a literal `)` inside a string (`@GetMapping("/x)y")`) no longer
//!   defeats extraction — the grammar already balanced the parens for us.
//! - A comment/blank line between an annotation and its declaration no longer ends the "annotation
//!   block" early — there is no lexical block scan to end; each annotation is its own AST node.
//!
//! Once isolated to ONE annotation's own raw argument text, this module still reuses the OLD crate's
//! plain-regex verb extraction (`request_method_verbs`, extended to the multi-verb array form), but the
//! path-literal extraction is NO LONGER a bare "first quoted string" scan: `route_path_arg` is
//! ATTRIBUTE-AWARE — a named `value=`/`path=` string wins over any other string-valued attribute
//! (`produces`, `headers`, ...) that happens to appear earlier in the argument list, falling back to
//! "first quoted string" only for the genuinely-positional single-arg form (`@GetMapping("/x")`). This is
//! a CORRECTNESS FIX over the old crate's first-quoted-string keying (which mis-keyed on
//! `@GetMapping(produces = "application/json", value = "/users")`), not a parity requirement — see
//! `route_path_arg`'s own doc. `project::resolve`'s constant-concatenation resolver reuses this same
//! `route_path_arg` for its literal-vs-constant-reference gate (module doc there).

use std::sync::OnceLock;

use regex::Regex;
use tree_sitter::Node;

use crate::util::{annotation_name, annotation_raw_args, annotations_of};

/// Method-level mapping annotation name -> the HTTP verb it implies — verbatim port of
/// `zzop_parser_java::provides::annotations::METHOD_ANNOTATIONS`. The verb column is pinned to
/// `zzop_core::HTTP_KEY_VERBS` by `provides::tests::method_annotation_verbs_are_pinned_to_the_core_verb_set`.
pub(crate) const METHOD_ANNOTATIONS: &[(&str, &str)] = &[
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("DeleteMapping", "DELETE"),
    ("PatchMapping", "PATCH"),
];

/// `RequestMethod` enum constant names — verbatim port of
/// `zzop_parser_java::provides::annotations::REQUEST_METHOD_NAMES`. Deliberately WIDER than
/// `zzop_core::HTTP_KEY_VERBS` (pinned divergence, see
/// `provides::tests::bare_request_method_names_are_a_deliberate_superset_of_the_core_verb_set`): this
/// mirrors Spring's own enum, and an explicit `method = RequestMethod.HEAD` attribute is a visible fact
/// worth providing even though no name-shaped consume vocabulary mints a HEAD key.
pub(crate) const REQUEST_METHOD_NAMES: &[&str] = &[
    "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE",
];

/// One class-shaped declaration's own annotation facts (never an inherited/meta-annotation — module doc
/// of `util::annotations_of`).
pub(crate) struct ClassAnnotationFacts {
    pub(crate) is_controller: bool,
    /// Raw argument text of this declaration's own `@RequestMapping`, if present at all. `Some("")` for
    /// a bare `@RequestMapping` (marker form, no parens) or `@RequestMapping()`; `None` when the
    /// annotation is absent entirely.
    pub(crate) request_mapping_arg: Option<String>,
}

/// Reads `modifiers`' own directly-attached annotations for the class-gating facts — shared by the
/// per-file fast path (`extract::walk_type`, which only trusts a literal quoted prefix) and
/// `project::collect` (which additionally resolves a non-literal prefix via corpus-wide constant lookup).
pub(crate) fn class_annotation_facts(modifiers: Option<Node>, src: &str) -> ClassAnnotationFacts {
    let mut is_controller = false;
    let mut request_mapping_arg = None;
    for ann in annotations_of(modifiers) {
        let Some(name) = annotation_name(ann, src) else {
            continue;
        };
        match name.as_str() {
            "RestController" | "Controller" => is_controller = true,
            "RequestMapping" => {
                request_mapping_arg = Some(annotation_raw_args(ann, src).unwrap_or_default());
            }
            _ => {}
        }
    }
    ClassAnnotationFacts {
        is_controller,
        request_mapping_arg,
    }
}

/// Reads `modifiers`' own directly-attached annotations for the `(VERB, path)` route(s) it implies —
/// empty when no recognized mapping annotation is present, or the only `@RequestMapping` found carries
/// no `method` attribute (ambiguous — module doc). A `@RequestMapping(method = {A, B})` listing several
/// verbs yields ONE `(verb, path)` pair per verb, all sharing the same path — the `METHOD_ANNOTATIONS`
/// shortcuts (`@GetMapping` etc.) always yield exactly one.
///
/// The per-annotation `(VERB, path-STATE)` route(s) — the raw tri-state the callers act on. The per-file
/// pass ([`method_route`]) drops a `NonLiteral` path (no corpus to resolve the constant); the whole-corpus
/// pass (`project::collect`/`walk`) instead carries the `NonLiteral` args forward and resolves the path
/// constant against the corpus, exactly as it already does for a class-level `@RequestMapping` prefix
/// (`project::resolve::resolve_method_path`). Empty when no recognized mapping annotation is present, or the
/// only `@RequestMapping` found carries no `method` attribute (ambiguous — module doc).
pub(crate) fn method_route_states(
    modifiers: Option<Node>,
    src: &str,
) -> Vec<(String, RoutePathState)> {
    for ann in annotations_of(modifiers) {
        let Some(name) = annotation_name(ann, src) else {
            continue;
        };
        let args = annotation_raw_args(ann, src).unwrap_or_default();
        if let Some((_, verb)) = METHOD_ANNOTATIONS.iter().find(|(n, _)| *n == name) {
            return vec![(verb.to_string(), route_path_state(&args))];
        }
        if name == "RequestMapping" {
            let verbs = request_method_verbs(&args);
            if !verbs.is_empty() {
                let state = route_path_state(&args);
                return verbs
                    .into_iter()
                    .map(|verb| (verb, state.clone()))
                    .collect();
            }
            // No `method` attribute -> ambiguous, keep scanning (module doc).
        }
    }
    Vec::new()
}

/// Reads `modifiers`' own directly-attached annotations for the `(VERB, path)` route(s) it implies — the
/// PER-FILE pass's view (`provides::extract`), which has no corpus in which to resolve a constant. A literal
/// keys the route, a genuinely ABSENT path (`@GetMapping`, `@PostMapping(produces = "json")`) keys the
/// controller-prefix-only base route `""`, but a NON-LITERAL path (`@GetMapping(ApiPaths.USERS)`) is DROPPED
/// rather than keyed at the empty base — collapsing it to `""` used to fabricate a phantom base route AND
/// lose the real one. The whole-corpus pass instead resolves the constant (see [`method_route_states`]).
pub(crate) fn method_route(modifiers: Option<Node>, src: &str) -> Vec<(String, String)> {
    method_route_states(modifiers, src)
        .into_iter()
        .filter_map(|(verb, state)| match state {
            RoutePathState::Literal(path) => Some((verb, path)),
            RoutePathState::Base => Some((verb, String::new())),
            RoutePathState::NonLiteral(_) => None,
        })
        .collect()
}

/// The tri-state a mapping annotation's raw argument text resolves to on the PATH axis — the method-level
/// parallel of the class-prefix `project::PrefixState`. See [`method_route_states`] for how the per-file vs
/// whole-corpus passes each act on `NonLiteral` (drop vs resolve-the-constant).
#[derive(Clone)]
pub(crate) enum RoutePathState {
    /// A quoted path literal (`@GetMapping("/x")`, `value = "/x"`).
    Literal(String),
    /// No path argument at all — a controller-prefix-only base route. Keyed as `""`.
    Base,
    /// A path argument is present but is a NON-LITERAL constant reference (`@GetMapping(ApiPaths.USERS)`,
    /// `value = SOME_CONST`). Carries the annotation's RAW ARGS so the whole-corpus pass can resolve the
    /// path constant (`project::resolve::resolve_method_path`); the per-file pass, having no corpus, drops it.
    NonLiteral(String),
}

/// Classifies a mapping annotation's raw args into [`RoutePathState`] — mirrors
/// `project::resolve::resolve_class_prefix`'s literal / empty / non-literal ladder: a
/// `route_path_arg` quoted literal wins; empty args are the base route; otherwise a path-DENOTING but
/// non-literal argument (`has_nonliteral_path_arg`, attribute-aware) is `NonLiteral`, while args that
/// carry ONLY non-path attributes (`produces=`, `headers=`, ...) are still the base route.
pub(crate) fn route_path_state(args: &str) -> RoutePathState {
    if let Some(lit) = route_path_arg(args) {
        return RoutePathState::Literal(lit);
    }
    if args.trim().is_empty() {
        return RoutePathState::Base;
    }
    if has_nonliteral_path_arg(args) {
        RoutePathState::NonLiteral(args.to_string())
    } else {
        RoutePathState::Base
    }
}

/// True when `args` carries a path-DENOTING argument that is not a quoted literal (so `route_path_arg`
/// already returned `None`) — a non-literal `value=`/`path=` RHS, or a positional first argument that is
/// not a `name = ...` attribute. Attribute-boundary-anchored exactly like
/// `project::resolve::const_ref_qualified`: a non-path attribute (`produces = "json"`, `headers = X`) that
/// happens to lead the argument list denotes no path and must read as the base route, not `NonLiteral`.
fn has_nonliteral_path_arg(args: &str) -> bool {
    // A named `value=`/`path=` attribute is path-denoting. If its RHS were a quoted literal,
    // `route_path_arg` would have caught it before we got here, so reaching this means a non-literal RHS.
    if named_path_attr_re().is_match(args) {
        return true;
    }
    // Otherwise: a POSITIONAL first argument (`@GetMapping(ApiPaths.USERS)`, or the array form
    // `@GetMapping({ApiPaths.USERS})`) is the path unless it is itself a `name = ...` attribute (which
    // would denote some other, non-path attribute). A leading `"`/`{` cannot reach here as a LITERAL —
    // `route_path_arg`'s positional branch already returned the first quoted string for those shapes, so a
    // quote-less `{...}` (or a lone quote-less token) that reaches this point is a non-literal path.
    let first = args.split(',').next().unwrap_or("").trim();
    !first.is_empty() && !leading_named_attr_re().is_match(first)
}

/// The route path literal implied by one annotation's raw argument text — Spring treats a mapping
/// annotation's `value` and `path` attributes as aliases for the same thing, so a NAMED `value=`/`path=`
/// attribute always wins over any other string-valued attribute that happens to appear earlier
/// (`produces`, `headers`, ...); only the bare POSITIONAL string-first form (`@GetMapping("/x")`, where the
/// path is genuinely the first quoted thing) falls back to "first quoted string in the args". Both the
/// named and the positional shape also accept the array/brace form (`value = {"/a", "/b"}`,
/// `@GetMapping({"/a","/b"})`) — only the FIRST path of a multi-path array is captured (full multi-path
/// expansion is deliberately out of scope, future work). `None` when neither shape is present (no path
/// literal at all — callers preserve today's `unwrap_or_default()` empty-string behavior).
pub(crate) fn route_path_arg(args: &str) -> Option<String> {
    if let Some(c) = value_attr_re().captures(args) {
        return Some(c[1].to_string());
    }
    if let Some(c) = path_attr_re().captures(args) {
        return Some(c[1].to_string());
    }
    let trimmed = args.trim_start();
    if trimmed.starts_with('"') || trimmed.starts_with('{') {
        return first_quoted_string(args);
    }
    None
}

/// The first `"..."` literal found in `args` — verbatim port of
/// `zzop_parser_java::provides::annotations::first_quoted_string`. Only reached today via
/// `route_path_arg`'s positional-form fallback.
fn first_quoted_string(args: &str) -> Option<String> {
    quoted_string_re().captures(args).map(|c| c[1].to_string())
}

/// Every verb named by a `RequestMethod.X` token in `args` (dedup'd, first-appearance order — the
/// `method = {RequestMethod.GET, RequestMethod.POST}` array-literal shape), or — only when no
/// `RequestMethod.X` token appears at all — a single statically-imported bare constant (`method = POST`)
/// accepted only when it is exactly one of `REQUEST_METHOD_NAMES`. Extends the old crate's single-verb
/// `request_method_verb` to the multi-verb array form.
fn request_method_verbs(args: &str) -> Vec<String> {
    let mut verbs = Vec::new();
    for c in request_method_re().captures_iter(args) {
        let v = c[1].to_uppercase();
        if !verbs.contains(&v) {
            verbs.push(v);
        }
    }
    if !verbs.is_empty() {
        return verbs;
    }
    bare_method_attr_re()
        .captures(args)
        .map(|c| c[1].to_string())
        .filter(|v| REQUEST_METHOD_NAMES.contains(&v.as_str()))
        .into_iter()
        .collect()
}

fn quoted_string_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""([^"]*)""#).unwrap())
}

fn value_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\bvalue\s*=\s*\{?\s*"([^"]*)""#).unwrap())
}

fn path_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\bpath\s*=\s*\{?\s*"([^"]*)""#).unwrap())
}

/// A `value=`/`path=` named attribute at a REAL attribute boundary (start, or just after a `(`/`,`) —
/// the boolean, quote-agnostic counterpart of `value_attr_re`/`path_attr_re` used by
/// `has_nonliteral_path_arg` to detect a path attribute whose RHS is NOT a quoted literal. Mirrors
/// `project::resolve::value_attr_const_re`'s boundary anchoring so a `value=` buried in another
/// attribute's string (`params = "value=1"`) is never mistaken for the named attribute.
fn named_path_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:^|[(,])\s*(?:value|path)\s*=").unwrap())
}

/// A leading `name = ...` named attribute — used by `has_nonliteral_path_arg` to tell a positional path
/// (`ApiPaths.USERS`) from a non-path named attribute (`produces = "json"`) in first position.
fn leading_named_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*[A-Za-z_$][\w$]*\s*=").unwrap())
}

fn request_method_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bRequestMethod\s*\.\s*([A-Za-z]+)").unwrap())
}

fn bare_method_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bmethod\s*=\s*([A-Z]+)\b").unwrap())
}
