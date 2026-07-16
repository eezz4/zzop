//! Java/Spring HTTP route PROVIDES extraction — AST-grade reimplementation of the old lexical
//! `zzop_parser_java::provides` extractor (parity-first, precision gains second — see `annotations`'
//! module doc for the specific lexical limits this crate no longer carries).
//!
//! ## Scope (v1 — identical to the old lexical crate's, task-pinned)
//! - Method-level: `@GetMapping`/`@PostMapping`/`@PutMapping`/`@DeleteMapping`/`@PatchMapping`, verb
//!   implied by the annotation name. Path comes from a bare annotation (empty path), a positional string
//!   arg, or a named `value = "/x"` / `path = "/x"` attribute — the FIRST quoted string in the argument
//!   list regardless of which attribute name it followed.
//! - `@RequestMapping` at method level additionally requires an explicit `method = RequestMethod.X` (or
//!   any bare `RequestMethod.X` token), or the statically-imported form `method = POST` (bare token
//!   accepted only when it is exactly a `RequestMethod` enum constant name). With no `method` attribute
//!   the annotation is AMBIGUOUS — Spring itself would map every verb — and is silently skipped rather
//!   than guess-emitting all five.
//! - `@RequestMapping` at CLASS (or interface/enum/record/annotation-type) level prefixes every
//!   method-level path via plain string concatenation; `http_interface_key`'s slash-collapse
//!   normalization makes the join exact regardless of leading/trailing slashes on either side.
//! - Class gating: only methods inside a declaration whose OWN annotation set carries
//!   `@RestController`/`@Controller` produce a provide. A NESTED type gates INDEPENDENTLY of its
//!   enclosing type (its own annotations only) — same as the old crate's span-scoped
//!   `enclosing_class` search, just AST-native here.
//!
//! ## Known limits carried forward from the old lexical crate (v1 scope, unchanged)
//! - `value = {"/a", "/b"}` (an array of multiple paths on one annotation) takes only the FIRST quoted
//!   string — a real Spring route registers under every path in the array; this extractor reports one.
//! - Class gating only looks at annotations directly attached to THIS declaration's own header — an
//!   inherited annotation from a superclass, or a meta-annotation that itself carries `@RestController`
//!   (Spring supports composed annotations), is invisible here and not detected.
//!
//! See `annotations`' module doc for the two lexical limits this AST-based reimplementation FIXES
//! (documented precision gains, not required for parity).

mod annotations;
mod extract;

pub use extract::extract_http_provides;

pub(crate) use annotations::{class_annotation_facts, first_quoted_string, method_route};

#[cfg(test)]
mod tests;
