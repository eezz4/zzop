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
//! Once isolated to ONE annotation's own raw argument text, though, this module reuses the OLD crate's
//! plain-regex value/verb extraction verbatim (`first_quoted_string`/`request_method_verb`) — porting
//! that logic byte-for-byte is what keeps `project::resolve`'s constant-concatenation resolver (which
//! also needs `first_quoted_string`) working unchanged, and keeps this module's OWN behavior identical
//! to the old crate's on every fixture that only exercises a single, well-formed annotation.

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

/// Reads `modifiers`' own directly-attached annotations for a `(VERB, path)` route — `None` when no
/// recognized mapping annotation is present, or the only `@RequestMapping` found carries no `method`
/// attribute (ambiguous — module doc).
pub(crate) fn method_route(modifiers: Option<Node>, src: &str) -> Option<(String, String)> {
    for ann in annotations_of(modifiers) {
        let Some(name) = annotation_name(ann, src) else {
            continue;
        };
        let args = annotation_raw_args(ann, src).unwrap_or_default();
        if let Some((_, verb)) = METHOD_ANNOTATIONS.iter().find(|(n, _)| *n == name) {
            return Some((
                verb.to_string(),
                first_quoted_string(&args).unwrap_or_default(),
            ));
        }
        if name == "RequestMapping" {
            if let Some(verb) = request_method_verb(&args) {
                return Some((verb, first_quoted_string(&args).unwrap_or_default()));
            }
            // No `method` attribute -> ambiguous, keep scanning (module doc).
        }
    }
    None
}

/// The first `"..."` literal found in `args` — verbatim port of
/// `zzop_parser_java::provides::annotations::first_quoted_string`.
pub(crate) fn first_quoted_string(args: &str) -> Option<String> {
    quoted_string_re().captures(args).map(|c| c[1].to_string())
}

/// The verb named by a `RequestMethod.X` token anywhere in `args`, or a statically-imported bare
/// constant (`method = POST`) accepted only when it is exactly one of `REQUEST_METHOD_NAMES` — verbatim
/// port of `zzop_parser_java::provides::annotations::request_method_verb`.
fn request_method_verb(args: &str) -> Option<String> {
    if let Some(c) = request_method_re().captures(args) {
        return Some(c[1].to_uppercase());
    }
    bare_method_attr_re()
        .captures(args)
        .map(|c| c[1].to_string())
        .filter(|v| REQUEST_METHOD_NAMES.contains(&v.as_str()))
}

fn quoted_string_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""([^"]*)""#).unwrap())
}

fn request_method_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bRequestMethod\s*\.\s*([A-Za-z]+)").unwrap())
}

fn bare_method_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bmethod\s*=\s*([A-Z]+)\b").unwrap())
}
