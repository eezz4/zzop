//! Java/Spring HTTP route PROVIDES extraction, read off the real `tree-sitter-java` CST — every
//! annotation is its own AST node here, already segmented and paren-balanced by the grammar (see
//! `annotations`' module doc for what that buys the argument reader).
//!
//! ## Scope (v1 — task-pinned)
//! - Method-level: `@GetMapping`/`@PostMapping`/`@PutMapping`/`@DeleteMapping`/`@PatchMapping`, verb
//!   implied by the annotation name. Path comes from a bare annotation (empty path), a positional string
//!   arg (`@GetMapping("/x")`), or a named `value = "/x"` / `path = "/x"` attribute — `value`/`path` are
//!   ALWAYS preferred over any other string-valued attribute (`produces`, `headers`, ...) that happens to
//!   appear earlier in the argument list (`annotations::route_path_arg`'s doc — a correctness fix over an
//!   earlier "first quoted string in the args, regardless of attribute name" keying).
//! - `@RequestMapping` at method level additionally requires an explicit `method = RequestMethod.X` (or
//!   any bare `RequestMethod.X` token), or the statically-imported form `method = POST` (bare token
//!   accepted only when it is exactly a `RequestMethod` enum constant name). With no `method` attribute
//!   the annotation is AMBIGUOUS — Spring itself would map every verb — and is silently skipped rather
//!   than guess-emitting all five. `method = {RequestMethod.GET, RequestMethod.POST}` (multiple verbs)
//!   yields one route PER verb, all sharing the same path.
//! - `@RequestMapping` at CLASS (or interface/enum/record/annotation-type) level prefixes every
//!   method-level path via plain string concatenation; `http_interface_key`'s slash-collapse
//!   normalization makes the join exact regardless of leading/trailing slashes on either side.
//! - Class gating: only methods inside a declaration whose OWN annotation set carries
//!   `@RestController`/`@Controller` produce a provide. A NESTED type gates INDEPENDENTLY of its
//!   enclosing type (its own annotations only) — pinned by
//!   `tests::a_non_literal_class_prefix_does_not_block_an_independently_gated_nested_type`.
//!
//! ## Known limits (v1 scope)
//! - `value = {"/a", "/b"}` / `@GetMapping({"/a", "/b"})` (an array of multiple paths on one annotation)
//!   captures only the FIRST path (`/a`) — a real Spring route registers under every path in the array;
//!   full multi-path expansion is deliberately out of scope (future work).
//! - Class gating only looks at annotations directly attached to THIS declaration's own header — an
//!   inherited annotation from a superclass, or a meta-annotation that itself carries `@RestController`
//!   (Spring supports composed annotations), is invisible here and not detected.
//!
//! See `annotations`' module doc for `route_path_arg`'s attribute-aware keying rationale, and
//! `tests`' "annotation shapes a text scan would mis-read" block for the paren-in-string and
//! blank-line-before-declaration shapes this reader handles because the grammar hands it whole nodes.

pub(crate) mod annotations;
mod extract;

pub use extract::extract_http_provides;

pub(crate) use annotations::{
    class_annotation_facts, method_route_states, route_path_arg, RoutePathState,
};

#[cfg(test)]
mod tests;
