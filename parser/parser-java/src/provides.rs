//! Java/Spring HTTP route PROVIDES extraction — lexical, annotation-based (same "no real grammar, just a
//! comment/string-aware scan" contract as `crate`'s own `parse_method_spans`). Reuses
//! `parse_method_spans`'s class/method span data — `SourceSymbol::line` already lands on a symbol's own
//! header including any directly-preceding annotation block — to locate each class's and method's
//! annotation text, then classifies Spring's five method-level mapping annotations plus `@RequestMapping`
//! into `zzop_core::IoProvide`s.
//!
//! ## Scope (v1)
//! - Method-level: `@GetMapping`/`@PostMapping`/`@PutMapping`/`@DeleteMapping`/`@PatchMapping`, verb implied
//!   by the annotation name. Path comes from a bare annotation (empty path), a positional string arg, or a
//!   named `value = "/x"` / `path = "/x"` attribute — this extractor takes the FIRST quoted string in the
//!   argument list regardless of which attribute name it followed.
//! - `@RequestMapping` at method level additionally requires an explicit `method = RequestMethod.X` (or any
//!   bare `RequestMethod.X` token), or the statically-imported form `method = POST` (bare token accepted
//!   only when it is exactly a `RequestMethod` enum constant name — dogfood round 14's UsersApi idiom),
//!   naming exactly one verb. With no `method` attribute the annotation is AMBIGUOUS — Spring itself would
//!   map every verb — and is silently skipped rather than guess-emitting all five (report nothing
//!   resolvable rather than force-match).
//! - `@RequestMapping` at CLASS level prefixes every method-level path via plain string concatenation;
//!   `http_interface_key`'s slash-collapse normalization makes the join exact regardless of leading/trailing
//!   slashes on either side.
//! - Class gating: only methods inside a class whose own annotation block carries `@RestController` or
//!   `@Controller` produce a provide.
//!
//! ## Known lexical-approximation limits (documented, not fixed — v1 scope)
//! - An annotation argument containing a literal `)` inside a string (e.g. `@GetMapping("/x)y")`) defeats
//!   the balanced-non-nested-paren argument scanner; accepted for v1, same spirit as `parse_method_spans`'s
//!   own "nested parens inside an annotation" limit.
//! - `value = {"/a", "/b"}` (an array of multiple paths on one annotation) takes only the FIRST quoted
//!   string — a real Spring route registers under every path in the array; this extractor reports one.
//! - The annotation-block scan (`annotation_block`) stops at the first line that is not itself annotation
//!   text (trimmed-starts-with `@`) and is not a continuation of an unbalanced parenthesized argument list —
//!   a comment line or blank line INSERTED between an annotation and its declaration (unidiomatic) would
//!   end the block early and drop that annotation.
//! - Class gating only looks at annotations directly attached to THIS class's own declaration — an
//!   inherited annotation from a superclass, or a meta-annotation that itself carries `@RestController`
//!   (Spring supports composed annotations), is invisible to a lexical scan and is not detected.

use std::sync::OnceLock;

use regex::Regex;
use zzop_core::{http_interface_key, IoProvide, SourceSymbol, SourceSymbolKind};

use crate::parse_method_spans;

/// Method-level mapping annotation name -> the HTTP verb it implies. The verb column is pinned to
/// `zzop_core::HTTP_KEY_VERBS` by a test (the annotation spelling `Get` can't literally share the
/// core const, so the pin is what keeps this vocabulary from drifting off the core verb set).
const METHOD_ANNOTATIONS: &[(&str, &str)] = &[
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("DeleteMapping", "DELETE"),
    ("PatchMapping", "PATCH"),
];

/// Extracts Spring MVC HTTP route `IoProvide`s from one Java file's raw source — see module doc for the
/// exact annotation shapes recognized and the class/method gating rule. Never panics on malformed input:
/// built entirely on `parse_method_spans` (itself panic-free) plus plain string/regex scans over the same
/// text.
pub fn extract_http_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let symbols = parse_method_spans(rel, text);
    let lines: Vec<&str> = text.lines().collect();

    let classes: Vec<&SourceSymbol> = symbols
        .iter()
        .filter(|s| s.kind == SourceSymbolKind::Class)
        .collect();

    let mut out = Vec::new();
    for method in symbols
        .iter()
        .filter(|s| s.kind == SourceSymbolKind::Function)
    {
        let Some(class) = enclosing_class(method, &classes) else {
            continue; // no enclosing class at all — never a controller.
        };
        let Some(ctx) = class_context(class, &lines) else {
            continue; // enclosing class is not @RestController/@Controller-annotated.
        };
        let Some((verb, path)) = method_route(&lines, method.line) else {
            continue; // no recognized mapping annotation, or an ambiguous bare @RequestMapping.
        };
        let full_path = format!("{}/{}", ctx.prefix, path);
        out.push(IoProvide {
            kind: "http".to_string(),
            key: http_interface_key(&verb, &full_path),
            file: rel.to_string(),
            line: method.line,
            symbol: Some(method.name.clone()),
        });
    }
    out
}

/// The innermost class among `classes` whose body span fully contains `method`'s body span — "smallest
/// containing span" picks the direct enclosing class even under nesting. `None` if `method` sits at top
/// level or its own span is incomplete.
pub(crate) fn enclosing_class<'a>(
    method: &SourceSymbol,
    classes: &[&'a SourceSymbol],
) -> Option<&'a SourceSymbol> {
    let (ms, me) = (method.body_start?, method.body_end?);
    classes
        .iter()
        .filter(|c| {
            let Some(cs) = c.body_start else {
                return false;
            };
            let Some(ce) = c.body_end else {
                return false;
            };
            cs <= ms && me <= ce
        })
        .min_by_key(|c| c.body_end.unwrap_or(u32::MAX) - c.body_start.unwrap_or(0))
        .copied()
}

/// A controller class's own routing context: the class-level `@RequestMapping` prefix (`""` when absent).
struct ClassContext {
    prefix: String,
}

/// Raw class-level annotation facts, before any constant resolution — shared by the per-file fast path
/// (`class_context` below, which only trusts a literal quoted prefix and falls back to `""` otherwise) and
/// `crate::project`'s whole-corpus pass (which additionally resolves a non-literal `request_mapping_arg`
/// via a corpus-wide constant lookup instead of defaulting).
pub(crate) struct ClassAnnotationFacts {
    pub(crate) is_controller: bool,
    /// Raw argument text of this class's own `@RequestMapping`, if that annotation is present at all.
    /// `Some("")` for a bare `@RequestMapping` with empty/no parens, `None` when no `@RequestMapping`
    /// annotation is present on this class at all.
    pub(crate) request_mapping_arg: Option<String>,
}

pub(crate) fn class_annotation_facts(class: &SourceSymbol, lines: &[&str]) -> ClassAnnotationFacts {
    let block = annotation_block(lines, class.line);
    let mut is_controller = false;
    let mut request_mapping_arg = None;
    for (name, args) in annotation_matches(&block) {
        match name.as_str() {
            "RestController" | "Controller" => is_controller = true,
            "RequestMapping" => request_mapping_arg = Some(args.unwrap_or_default()),
            _ => {}
        }
    }
    ClassAnnotationFacts {
        is_controller,
        request_mapping_arg,
    }
}

/// Reads `class`'s own annotation block and returns its `ClassContext` iff it carries `@RestController` or
/// `@Controller` — `None` gates the whole class out (see module doc's class-gating rule). Trusts only a
/// literal quoted `@RequestMapping` prefix; a non-literal one (a constant reference — this per-file pass
/// has no cross-file visibility to resolve it) silently defaults to `""`, same as an absent
/// `@RequestMapping` — see `crate::project` for the whole-corpus pass that resolves it instead of guessing.
fn class_context(class: &SourceSymbol, lines: &[&str]) -> Option<ClassContext> {
    let facts = class_annotation_facts(class, lines);
    let prefix = facts
        .request_mapping_arg
        .as_deref()
        .and_then(first_quoted_string)
        .unwrap_or_default();
    facts.is_controller.then_some(ClassContext { prefix })
}

/// Reads the annotation block directly preceding `method_line` (1-indexed — `SourceSymbol::line`'s own
/// convention) and returns `(VERB, path)` for the first recognized mapping annotation found, or `None` when
/// no such annotation is present (plain, non-routed method) or the only `@RequestMapping` found carries no
/// `method` attribute (ambiguous — see module doc).
pub(crate) fn method_route(lines: &[&str], method_line: u32) -> Option<(String, String)> {
    let block = annotation_block(lines, method_line);
    for (name, args) in annotation_matches(&block) {
        let args_str = args.unwrap_or_default();
        if let Some((_, verb)) = METHOD_ANNOTATIONS.iter().find(|(n, _)| *n == name) {
            return Some((
                verb.to_string(),
                first_quoted_string(&args_str).unwrap_or_default(),
            ));
        }
        if name == "RequestMapping" {
            if let Some(verb) = request_method_verb(&args_str) {
                return Some((verb, first_quoted_string(&args_str).unwrap_or_default()));
            }
            // No `method` attribute -> ambiguous, keep scanning (in case another, unambiguous mapping
            // annotation is also present — not idiomatic Spring, but not this extractor's job to reject).
        }
    }
    None
}

/// Collects the contiguous block of annotation source text starting at `start_line` (1-indexed, `lines`
/// 0-indexed) — see module doc's "Known lexical-approximation limits" for the exact stop condition. A line
/// belongs to the block if it (trimmed) starts with `@`, OR if it continues an unbalanced parenthesized
/// argument list opened by an earlier line already in this block (so a multi-line
/// `@RequestMapping(value = "/x",\n  method = RequestMethod.GET)` is captured whole). Never panics: an
/// `start_line` past `lines.len()` (defensive — should not happen, `SourceSymbol::line` is always within the
/// source it was derived from) just yields an empty block.
fn annotation_block(lines: &[&str], start_line: u32) -> String {
    let mut out = String::new();
    let mut depth: i32 = 0;
    let mut idx = start_line.saturating_sub(1) as usize;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if depth == 0 && !trimmed.starts_with('@') {
            break;
        }
        out.push_str(line);
        out.push('\n');
        depth += line.matches('(').count() as i32 - line.matches(')').count() as i32;
        if depth < 0 {
            depth = 0;
        }
        idx += 1;
    }
    out
}

/// Every `@AnnotationName` (restricted to the names this module recognizes) in `block`, paired with its raw
/// argument text (`None` for a bare annotation with no parens at all, `Some("")` for empty parens `()`).
fn annotation_matches(block: &str) -> Vec<(String, Option<String>)> {
    annotation_re()
        .captures_iter(block)
        .map(|c| (c[1].to_string(), c.get(2).map(|m| m.as_str().to_string())))
        .collect()
}

/// The first `"..."` literal found in `args` — covers a positional `value` arg, a named `value = "..."` /
/// `path = "..."` attribute, and (as a documented simplification — see module doc) the first element of a
/// `value = {"/a", "/b"}` array, all with one regex.
pub(crate) fn first_quoted_string(args: &str) -> Option<String> {
    quoted_string_re().captures(args).map(|c| c[1].to_string())
}

/// `RequestMethod` enum constant names — the only tokens a bare (statically-imported) `method = POST`
/// attribute may legally carry. The exact-set membership check is what keeps the bare-token branch below
/// on the never-guess side: `method = SOME_CONSTANT` with a name outside this set stays ambiguous.
/// Deliberately WIDER than `zzop_core::HTTP_KEY_VERBS` (a pinned divergence, see tests): this mirrors
/// Spring's own enum, and an explicit `method = HEAD` attribute is a visible fact worth providing even
/// though no name-shaped consume vocabulary mints HEAD keys — verb-set membership here answers "is this
/// token Spring's enum", not "is this a cross-layer keying verb".
const REQUEST_METHOD_NAMES: &[&str] = &[
    "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE",
];

/// The verb named by a `RequestMethod.X` token anywhere in `args` (Spring's `method = RequestMethod.GET`
/// attribute, or the bare enum token — this extractor does not require the `method =` prefix itself, only
/// the unambiguous `RequestMethod.X` shape, since nothing else in a `@RequestMapping` argument list carries
/// that literal token), OR by a statically-imported bare constant in the `method = POST` shape
/// (`import static ...RequestMethod.POST;` — dogfood round 14's UsersApi idiom), accepted only when the
/// bare token is exactly one of `REQUEST_METHOD_NAMES` so an arbitrary constant reference never resolves.
/// `None` when neither shape is present — the ambiguous, ANY-verb case (see module doc).
fn request_method_verb(args: &str) -> Option<String> {
    if let Some(c) = request_method_re().captures(args) {
        return Some(c[1].to_uppercase());
    }
    bare_method_attr_re()
        .captures(args)
        .map(|c| c[1].to_string())
        .filter(|v| REQUEST_METHOD_NAMES.contains(&v.as_str()))
}

fn annotation_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)@(RestController|Controller|RequestMapping|GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping)\b(?:\s*\(([^()]*)\))?",
        )
        .unwrap()
    })
}

fn quoted_string_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""([^"]*)""#).unwrap())
}

fn request_method_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bRequestMethod\s*\.\s*([A-Za-z]+)").unwrap())
}

/// A bare `method = TOKEN` attribute (no `RequestMethod.` qualifier — the static-import idiom). The
/// captured TOKEN is only trusted after `REQUEST_METHOD_NAMES` membership (see `request_method_verb`).
fn bare_method_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bmethod\s*=\s*([A-Z]+)\b").unwrap())
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_http_provides`: every method-level mapping-annotation shape (bare / positional
    //! string / `value=` / `path=`), class-level `@RequestMapping` prefixing, `@RequestMapping` with an
    //! explicit `method` attribute, the ambiguous-no-method-attribute skip, the `@RestController`/
    //! `@Controller` class gate (including the negative — a non-controller class emits nothing), and a
    //! multi-method controller class shape end to end.
    use super::*;

    fn keys(out: &[IoProvide]) -> Vec<String> {
        out.iter().map(|p| p.key.clone()).collect()
    }

    #[test]
    fn bare_get_mapping_on_a_rest_controller_yields_an_empty_path_route() {
        let src = "@RestController\nclass C {\n  @GetMapping\n  void ping() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["GET /"]);
        assert_eq!(out[0].symbol.as_deref(), Some("ping"));
        assert_eq!(out[0].line, 3);
    }

    #[test]
    fn positional_string_arg_is_the_path() {
        let src = "@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["GET /x"]);
    }

    #[test]
    fn value_named_attribute_is_the_path() {
        let src = "@RestController\nclass C {\n  @PostMapping(value = \"/x\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["POST /x"]);
    }

    #[test]
    fn path_named_attribute_is_the_path() {
        let src = "@RestController\nclass C {\n  @PutMapping(path = \"/x\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["PUT /x"]);
    }

    #[test]
    fn every_mapping_annotation_maps_to_its_own_verb() {
        let src = "@RestController\nclass C {\n  @GetMapping(\"/a\")\n  void a() {}\n  @PostMapping(\"/b\")\n  void b() {}\n  @PutMapping(\"/c\")\n  void c() {}\n  @DeleteMapping(\"/d\")\n  void d() {}\n  @PatchMapping(\"/e\")\n  void e() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(
            got,
            vec!["DELETE /d", "GET /a", "PATCH /e", "POST /b", "PUT /c"]
        );
    }

    #[test]
    fn class_level_request_mapping_prefixes_every_method_path() {
        let src = "@RequestMapping(\"/authen\")\n@RestController\nclass CtrlAuthen {\n  @GetMapping(\"/getUserInfo\")\n  UserInfo getUserInfo() { return null; }\n}\n";
        let out = extract_http_provides("CtrlAuthen.java", src);
        assert_eq!(keys(&out), vec!["GET /authen/getUserInfo"]);
    }

    #[test]
    fn class_level_prefix_via_value_attribute_also_works() {
        let src = "@RequestMapping(value = \"/authen\")\n@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["GET /authen/x"]);
    }

    #[test]
    fn request_mapping_with_an_explicit_method_attribute_resolves_the_verb() {
        let src = "@RestController\nclass C {\n  @RequestMapping(value=\"/x\", method = RequestMethod.GET)\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["GET /x"]);
    }

    #[test]
    fn request_mapping_split_across_lines_still_resolves() {
        let src = "@RestController\nclass C {\n  @RequestMapping(value=\"/x\",\n    method = RequestMethod.POST)\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["POST /x"]);
    }

    // --- policy-value pins (rule-quality: T2 pin tests are the cross-boundary substitute for a
    // shared constant) ---

    #[test]
    fn method_annotation_verbs_are_pinned_to_the_core_verb_set() {
        // The `{Verb}Mapping` spelling can't literally share `zzop_core::HTTP_KEY_VERBS`, so this pin
        // is what makes a verb added to the core set an everywhere-at-once decision instead of drift.
        let table: std::collections::BTreeSet<&str> =
            METHOD_ANNOTATIONS.iter().map(|(_, verb)| *verb).collect();
        let core: std::collections::BTreeSet<&str> =
            zzop_core::HTTP_KEY_VERBS.iter().copied().collect();
        assert_eq!(
            table, core,
            "METHOD_ANNOTATIONS' verb column drifted from zzop_core::HTTP_KEY_VERBS — change both \
             deliberately or neither"
        );
    }

    #[test]
    fn bare_request_method_names_are_a_deliberate_superset_of_the_core_verb_set() {
        // Divergence pin: REQUEST_METHOD_NAMES mirrors Spring's OWN enum (8 names — an explicit
        // `method = HEAD` is a visible fact), deliberately wider than the 5-verb core keying set.
        // If this fails, either the core set grew past Spring's enum (decide how Java maps it) or
        // someone "unified" the two sets — both must be deliberate decisions.
        for verb in zzop_core::HTTP_KEY_VERBS {
            assert!(
                REQUEST_METHOD_NAMES.contains(verb),
                "core keying verb {verb} missing from Spring's RequestMethod name set"
            );
        }
        assert_eq!(
            REQUEST_METHOD_NAMES.len(),
            8,
            "REQUEST_METHOD_NAMES must stay exactly Spring's RequestMethod enum, not drift toward \
             the core keying set"
        );
    }

    #[test]
    fn request_mapping_with_a_statically_imported_bare_method_constant_resolves() {
        // `import static ...RequestMethod.POST;` then `method = POST` — round 14's UsersApi idiom.
        let src = "@RestController\nclass C {\n  @RequestMapping(path = \"/users\", method = POST)\n  void register() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["POST /users"]);
    }

    #[test]
    fn a_bare_method_token_outside_the_request_method_enum_stays_ambiguous() {
        // An arbitrary ALL-CAPS constant is NOT a RequestMethod name — never guessed into a verb.
        let src = "@RestController\nclass C {\n  @RequestMapping(path = \"/x\", method = CUSTOM)\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert!(
            out.is_empty(),
            "a non-RequestMethod bare token must stay ambiguous, got: {out:?}"
        );
    }

    #[test]
    fn request_mapping_without_a_method_attribute_is_skipped_not_guessed() {
        let src = "@RestController\nclass C {\n  @RequestMapping(\"/x\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert!(
            out.is_empty(),
            "an ambiguous @RequestMapping (no method attribute) must never guess-emit a verb, got: {out:?}"
        );
    }

    #[test]
    fn a_plain_class_with_the_controller_annotation_is_also_recognized() {
        let src = "@Controller\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["GET /x"]);
    }

    #[test]
    fn a_class_without_rest_controller_or_controller_emits_nothing() {
        let src = "class C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert!(
            out.is_empty(),
            "non-controller class must emit no provides, got: {out:?}"
        );
    }

    #[test]
    fn a_method_with_no_mapping_annotation_at_all_emits_nothing() {
        let src = "@RestController\nclass C {\n  void helper() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert!(out.is_empty());
    }

    #[test]
    fn path_variables_are_normalized_by_http_interface_key() {
        let src = "@RestController\nclass C {\n  @GetMapping(\"/users/{id}\")\n  void x() {}\n}\n";
        let out = extract_http_provides("C.java", src);
        assert_eq!(keys(&out), vec!["GET /users/{}"]);
    }

    #[test]
    fn empty_file_yields_no_provides() {
        assert!(extract_http_provides("E.java", "").is_empty());
    }

    // --- nested-paren parameter-list annotations no longer drop the route (v3 regression) ---

    #[test]
    fn user_controller_shape_with_annotated_and_header_params_yields_get_and_put_user() {
        let src = concat!(
            "@RestController\n",
            "@RequestMapping(path = \"/user\")\n",
            "class UserController {\n\n",
            "    @GetMapping\n",
            "    User currentUser(@AuthenticationPrincipal User u, @RequestHeader(value = \"Authorization\") String h) {\n",
            "        return null;\n    }\n\n",
            "    @PutMapping\n",
            "    User updateUser(@AuthenticationPrincipal User u, @RequestHeader(value = \"Authorization\") String h) {\n",
            "        return null;\n    }\n}\n",
        );
        let out = extract_http_provides("UserController.java", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(got, vec!["GET /user", "PUT /user"]);
    }

    #[test]
    fn articles_api_shape_with_request_param_default_value_params_yields_all_three_routes() {
        let src = concat!(
            "@RestController\n",
            "@RequestMapping(path = \"/articles\")\n",
            "class ArticlesApi {\n\n",
            "    @PostMapping\n",
            "    Article create(@Valid @RequestBody ArticleCreateRequest req) {\n",
            "        return null;\n    }\n\n",
            "    @GetMapping(path = \"feed\")\n",
            "    List<Article> feed(@RequestParam(value = \"offset\", defaultValue = \"0\") int offset) {\n",
            "        return null;\n    }\n\n",
            "    @GetMapping\n",
            "    List<Article> list(@RequestParam(value = \"offset\", defaultValue = \"0\") int offset) {\n",
            "        return null;\n    }\n}\n",
        );
        let out = extract_http_provides("ArticlesApi.java", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(
            got,
            vec!["GET /articles", "GET /articles/feed", "POST /articles"]
        );
    }

    // --- a multi-method controller class shape, end to end ---

    #[test]
    fn ctrl_authen_shape_yields_three_get_routes_under_the_authen_prefix() {
        let src = concat!(
            "package com.example.app.controllers;\n\n",
            "import org.springframework.web.bind.annotation.GetMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n\n",
            "@RequestMapping(\"/authen\")\n",
            "@RestController\n",
            "public class CtrlAuthen {\n\n",
            "    @GetMapping(\"/getGoogleRedirect\")\n",
            "    public String getGoogleRedirect() {\n        return \"\";\n    }\n\n",
            "    @GetMapping(\"/getUserInfo\")\n",
            "    public UserInfo getUserInfo() {\n        return null;\n    }\n\n",
            "    @GetMapping(\"/getSignout\")\n",
            "    public boolean getSignout() {\n        return true;\n    }\n}\n",
        );
        let out = extract_http_provides("CtrlAuthen.java", src);
        let mut got = keys(&out);
        got.sort();
        assert_eq!(
            got,
            vec![
                "GET /authen/getGoogleRedirect",
                "GET /authen/getSignout",
                "GET /authen/getUserInfo",
            ]
        );
        let user_info = out
            .iter()
            .find(|p| p.symbol.as_deref() == Some("getUserInfo"))
            .unwrap();
        assert_eq!(user_info.key, "GET /authen/getUserInfo");
    }
}
