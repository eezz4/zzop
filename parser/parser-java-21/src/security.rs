//! Spring Security **method-security** annotation extraction -> guarded route registration LINES, for the
//! `mutating-route-no-auth` rule's decorator-guard exemption (the Java analog of NestJS `@UseGuards`,
//! consumed via the same framework-neutral `(file, line)` exemption set).
//!
//! ## What it recognizes
//! A controller route method is access-controlled when it — OR the controller class that owns it — carries
//! a Spring method-security annotation: `@PreAuthorize` / `@PostAuthorize` (SpEL access rules),
//! `@Secured` (role list), or `@RolesAllowed` (JSR-250). A class-level annotation guards EVERY route
//! method the controller declares; a method-level one guards just that method. These annotations gate the
//! handler BEFORE its body runs — metadata the whole-repo call-graph BFS structurally cannot see (a
//! decorator/annotation application is not a call edge), exactly the blind spot the rule documents for
//! NestJS `@UseGuards` and route-level middleware.
//!
//! ## Line contract (why this matches the route provide)
//! For each guarded route method this emits `line_of(method_declaration)` — the SAME node
//! `provides::extract::walk_member` anchors its `IoProvide` on (a Spring route's line is its method's
//! first-modifier/annotation line). So a line this returns is exactly the `(file, line)` the rule tests
//! each provide against, and the exemption fires. A guarded method that is NOT a route (no mapping
//! annotation) is skipped — only real registered routes matter to the rule.
//!
//! ## Precision
//! Recognition is by annotation NAME only (`@PreAuthorize`'s SpEL argument is never interpreted — a
//! present annotation IS the access gate, `permitAll()`-style always-open SpEL is a rare, deliberate
//! author choice not modeled here). This is an EXEMPTION producer for a security rule, so its failure mode
//! is one-directional and safe: a missed annotation (an alias, a meta-annotation, a non-controller class)
//! only fails to exempt a route — the rule keeps its finding — it never clears a route that lacks a real
//! gate. Global-config auth (a `SecurityFilterChain` / `WebSecurityConfigurerAdapter` builder DSL) is a
//! separate, whole-application posture NOT handled here (its per-route mapping is undecidable from this
//! per-method view — the same residual the rule documents for NestJS global guards).

use tree_sitter::Node;

use crate::lang::symbols::is_type_decl_kind;
use crate::provides::annotations::{class_annotation_facts, method_route_states};
use crate::util::{annotation_name, annotations_of, line_of, modifiers_of, valid_named_children};

/// Spring method-security annotation simple names — a present one (class- or method-level) marks the
/// route access-controlled. `@PreFilter`/`@PostFilter` are element FILTERING, not an access gate, so are
/// deliberately excluded.
const SECURITY_ANNOTATIONS: &[&str] = &["PreAuthorize", "PostAuthorize", "Secured", "RolesAllowed"];

/// Extract the registration lines of Spring controller routes guarded by a method-security annotation
/// (method- or class-level). Empty on parse failure (never panics). See module doc for the line contract.
pub fn extract_spring_guarded_lines(rel: &str, text: &str) -> Vec<u32> {
    let _ = rel; // symmetry with the other extractors' signature; the rule keys (file, line) caller-side.
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in valid_named_children(tree.root_node()) {
        if is_type_decl_kind(child.kind()) {
            walk_type(child, text, &mut out);
        }
    }
    out
}

/// One class-shaped declaration: reads its OWN controller + class-level-security facts (never an
/// ancestor's — a nested type gates independently), then walks its members.
fn walk_type(node: Node, src: &str, out: &mut Vec<u32>) {
    let mods = modifiers_of(node);
    let is_controller = class_annotation_facts(mods, src).is_controller;
    let class_guarded = has_security_annotation(mods, src);
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    walk_body(body, src, is_controller, class_guarded, out);
}

fn walk_body(body: Node, src: &str, is_controller: bool, class_guarded: bool, out: &mut Vec<u32>) {
    for child in valid_named_children(body) {
        if child.kind() == "enum_body_declarations" {
            for member in valid_named_children(child) {
                walk_member(member, src, is_controller, class_guarded, out);
            }
        } else {
            walk_member(child, src, is_controller, class_guarded, out);
        }
    }
}

fn walk_member(
    node: Node,
    src: &str,
    is_controller: bool,
    class_guarded: bool,
    out: &mut Vec<u32>,
) {
    if is_type_decl_kind(node.kind()) {
        walk_type(node, src, out); // nested type gates on its OWN annotations, not the enclosing class's
        return;
    }
    // Route MEMBERSHIP gate: emit a guarded-line for any method the WHOLE-CORPUS provides pass could key as
    // a route, so the exemption lines up with `run_java_provides_project_pass`'s output (the set
    // `mutating-route-no-auth` filters on). Uses `method_route_states` (non-empty for ANY recognized mapping
    // annotation) rather than the per-file `method_route` (which DROPS a NonLiteral path): the whole-corpus
    // pass now RESOLVES a method-path constant (`@PostMapping(ApiPaths.CREATE)`), so a NonLiteral-path method
    // IS a route there and must stay exempted when guarded. Over-emitting a line for a method whose path
    // turns out to be out-of-corpus (dropped whole-corpus too) is a harmless no-op exemption — there is no
    // provide at that line to falsely clear; UNDER-emitting would false-positive a guarded route.
    if !is_controller
        || !matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration"
        )
    {
        return;
    }
    let mods = modifiers_of(node);
    if method_route_states(mods, src).is_empty() {
        return;
    }
    if class_guarded || has_security_annotation(mods, src) {
        out.push(line_of(node)); // the method_declaration line = the route provide's own anchor line
    }
}

/// True when `modifiers`' own directly-attached annotations include a Spring method-security annotation.
fn has_security_annotation(modifiers: Option<Node>, src: &str) -> bool {
    annotations_of(modifiers).iter().any(|ann| {
        annotation_name(*ann, src).is_some_and(|n| SECURITY_ANNOTATIONS.contains(&n.as_str()))
    })
}

#[cfg(test)]
mod tests;
