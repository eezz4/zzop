//! `csharp_namespaces_of` — every namespace this file declares, dotted, for the engine's
//! namespace -> files dependency index (the C# analogue of Java's `java_package_of`, except a single
//! C# file may declare MULTIPLE distinct namespaces, unlike Java's one-package-per-file rule, so this
//! returns a `Vec` rather than an `Option<String>`).
//!
//! Both namespace forms contribute: a file-scoped `namespace Foo.Bar;` (C# 10+, applies to the rest of
//! the file) and a block `namespace Foo.Bar { ... }`. A block namespace's name is already a single
//! (possibly dotted) node span (`namespace Foo.Bar { }` is ONE `namespace_declaration`, not two nested
//! ones) — nested BLOCKS (`namespace A { namespace B { } }`) each contribute their own fully-qualified
//! entry (`"A"` AND `"A.B"`), joined with the enclosing namespace's own dotted prefix.

use tree_sitter::Node;

use crate::util::{node_text, valid_named_children};

/// Extract every namespace declared in `text` — see module doc. Empty on parse failure.
pub fn csharp_namespaces_of(text: &str) -> Vec<String> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect(tree.root_node(), text, &[], &mut out);
    out
}

fn collect(node: Node, src: &str, prefix: &[String], out: &mut Vec<String>) {
    for child in valid_named_children(node) {
        match child.kind() {
            "namespace_declaration" => {
                let Some(name_node) = child.child_by_field_name("name") else {
                    continue;
                };
                let mut full = prefix.to_vec();
                full.push(node_text(name_node, src).to_string());
                out.push(full.join("."));
                if let Some(body) = child.child_by_field_name("body") {
                    collect(body, src, &full, out);
                }
            }
            "file_scoped_namespace_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let mut full = prefix.to_vec();
                    full.push(node_text(name_node, src).to_string());
                    out.push(full.join("."));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
