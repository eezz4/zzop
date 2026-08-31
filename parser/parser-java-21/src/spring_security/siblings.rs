//! The SCOPE layer of [`super::extract_spring_security_posture`]: the two chain-level hazard checks in
//! [`super::clauses`] read ONE builder-chain spine, but Java lets the same `HttpSecurity` object be
//! configured by SIBLING STATEMENTS, and Spring applies both spellings to the same filter chain:
//!
//! ```java
//! http.authorizeHttpRequests(r -> r.anyRequest().authenticated())
//!     .formLogin(f -> f.loginPage("/thing/open").permitAll());  // ONE chain — the spine walk sees it
//!
//! http.authorizeHttpRequests(r -> r.anyRequest().authenticated());
//! http.formLogin(f -> f.loginPage("/thing/open").permitAll());  // TWO statements — it does NOT
//! ```
//!
//! Measured before this module existed, on a tree whose only mutating route is `POST /thing/open`: the
//! one-chain form bailed (`configurer-permit-all`, 2 findings kept) and the split form returned a posture
//! with an EMPTY `permit_all`, reading that route as authenticated (0 findings). `POST /thing/open` is
//! genuinely open — Spring's `PermitAllSupport` opens `loginPage`, which doubles as the login-PROCESSING
//! url — so the split form was a silent false CLEAR on an unguarded mutating route. The same split hid a
//! `securityMatcher`, applying a path-LOCAL posture tree-wide to routes outside its scope.
//!
//! **What relates two statements**: the SOURCE TEXT of the base their chains are rooted at (`http`). Text,
//! not a resolved type, because this pass has no type resolution — and text is the conservative choice
//! here in both directions: a sibling rooted at a different name is left alone (no over-bail on unrelated
//! locals), while the two ways that identity could still be wrong — a chain with no nameable receiver at
//! all, and a second name bound to the same object — are themselves bails
//! ([`SpringPostureBail::SiblingScope`]) rather than assumptions.
//!
//! **How far the scan reaches**: the enclosing METHOD declaration, not the entrypoint's own block. A
//! `securityMatcher` one nesting level out (in the method body while the entrypoint sits in an `if`) is
//! the same hazard, and stopping at the nearest block would miss it.
//!
//! Nothing here collects clauses: a second `authorizeHttpRequests(..)` in a sibling statement is a second
//! chain root and has already bailed as [`SpringPostureBail::MultipleChains`] before this runs.

use tree_sitter::Node;

use crate::util::node_text;

use super::clauses::{spine, spine_hazard};
use super::{chain_root, SpringPostureBail};

/// Re-run the chain-level hazard checks over every OTHER builder chain in the entrypoint's enclosing
/// method that is rooted at the same base name. `chain` is the entrypoint's own chain root, already
/// walked by [`super::clauses::walk_chain`], and is the one chain skipped here.
pub(super) fn scan(chain: Node, src: &str) -> Result<(), SpringPostureBail> {
    // No nameable receiver: no sibling statement can be shown to be about a DIFFERENT object, and a
    // missed hazard clears findings while a bail keeps them.
    let Some(base) = chain_base(chain, src) else {
        return Err(SpringPostureBail::SiblingScope("chain-base".into()));
    };
    let mut stack = vec![enclosing_method(chain)];
    while let Some(node) = stack.pop() {
        if binds_an_alias(node, base, src) {
            return Err(SpringPostureBail::SiblingScope("alias".into()));
        }
        if node.kind() == "method_invocation"
            && node.id() != chain.id()
            && chain_root(node).id() == node.id()
            && chain_base(node, src) == Some(base)
        {
            for link in spine(node) {
                let name = link
                    .child_by_field_name("name")
                    .map_or("", |n| node_text(n, src));
                if let Some(bail) = spine_hazard(name, link.child_by_field_name("arguments"), src) {
                    return Err(bail);
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    Ok(())
}

/// The source text of the object a builder chain is rooted at (`http` in `http.csrf().disable()`), or
/// `None` when the innermost link has no receiver at all. Only chain ROOTS are asked, so a sub-invocation
/// of the entrypoint's own chain is never mistaken for a sibling.
fn chain_base<'a>(chain: Node, src: &'a str) -> Option<&'a str> {
    let mut cur = chain;
    loop {
        let object = cur.child_by_field_name("object")?;
        if object.kind() != "method_invocation" {
            return Some(node_text(object, src));
        }
        cur = object;
    }
}

/// Whether `node` binds a SECOND name to the builder (`HttpSecurity h = http;`, `h = http;`). Under such
/// an alias a hazard could reach the same object through a name this scan does not follow, and nothing
/// here can prove the two names are different objects — so it bails.
fn binds_an_alias(node: Node, base: &str, src: &str) -> bool {
    let value = match node.kind() {
        "variable_declarator" => node.child_by_field_name("value"),
        "assignment_expression" => node.child_by_field_name("right"),
        _ => return false,
    };
    value.is_some_and(|v| node_text(v, src) == base)
}

/// The subtree a sibling statement may live in: the method (or constructor) declaration containing the
/// chain. Falls back to the whole file when the chain is in neither (a field initializer) — the wider
/// scope can only add bails, never remove one.
fn enclosing_method(chain: Node) -> Node {
    let mut cur = chain;
    while let Some(parent) = cur.parent() {
        if matches!(
            parent.kind(),
            "method_declaration" | "constructor_declaration"
        ) {
            return parent;
        }
        cur = parent;
    }
    cur
}
