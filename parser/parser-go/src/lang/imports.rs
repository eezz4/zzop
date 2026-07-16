//! Import extraction -> `zzop_core::ImportMap`. v1 scope: every top-level `import_declaration` in the
//! file — Go's grammar makes this inherently exhaustive (an `import` can only ever appear at file
//! scope, immediately after the `package` clause; there is no nested/function-local import the way
//! JS's `require()` or Python's in-function `import` allow), so unlike `lang::symbols`'s "top-level
//! only" v1 NARROWING, this module's top-level walk already covers every import the language permits.
//!
//! ## Grammar shape: `import_declaration` wraps its spec(s) ONE level deeper
//! `import "a/b/c"` (ungrouped) has a single `import_spec` child; `import ( "a/b/c"; d "e/f" )`
//! (grouped) has a single `import_spec_list` child that itself holds the `import_spec` children — the
//! same `_spec`-or-`_spec_list` asymmetry `lang::symbols::emit_var_declaration`'s doc calls out for
//! `var_declaration` (and for the identical reason: both grammar productions reuse one wrapper shape
//! for "one or many").
//!
//! ## Binding-name convention (task-pinned, a documented v1 approximation)
//! - **Aliased** (`import x "a/b/c"`): local name = `x` (a `package_identifier` in the spec's `name`
//!   field).
//! - **Plain** (`import "a/b/c"`, no `name` field at all): local name = the import path's LAST `/`-
//!   separated segment (`c`). This is a KNOWN v1 approximation, not the real Go rule — a package's
//!   actual local name is whatever `package` clause its own source files declare, which can legally
//!   differ from the path's last segment (`import "gopkg.in/yaml.v2"` binds the name `yaml`, not
//!   `yaml.v2`) — resolving that would require parsing the imported package's own files, out of a
//!   single-file extractor's reach. Same "last-segment ambiguity, resolved at the file boundary this
//!   crate cannot see past" shape `zzop_parser_rust::lang::resolve`'s module doc documents for its own
//!   `crate::a::b` last-segment case.
//! - **Dot** (`import . "a/b/c"`, `name` field is a `dot` node): injects every exported name into this
//!   file's scope directly — no single local binding name exists, mirroring
//!   `zzop_parser_rust::lang::imports::insert_glob`'s synthetic, collision-free map key for a Rust
//!   glob import. `original` is set to `"*"`, reusing `ImportBinding::original`'s own documented
//!   "namespace = `*`" convention.
//! - **Blank** (`import _ "a/b/c"`, `name` field is a `blank_identifier` node): a side-effect-only
//!   import (runs the package's `init()`s, binds nothing) — still a real edge (the file DOES depend on
//!   that package having been compiled/linked), recorded with the same synthetic-key treatment as a
//!   dot import; `original` is `"_"`.
//!
//! `specifier` is the import path string VERBATIM (`util::string_literal_text`, delimiters stripped,
//! no escape decoding — that helper's own doc explains why). `deferred`/`type_only` are always
//! `false`: Go has neither a lazy-import nor an erased-at-compile-time-type-only concept.

use tree_sitter::Node;
use zzop_core::{ImportBinding, ImportMap};

use crate::util::{node_text, string_literal_text, valid_named_children};

/// Extract this file's import bindings — see module doc. Empty on parse failure (never panics).
pub fn parse_imports(text: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(tree) = crate::parse_tree(text) else {
        return map;
    };
    let mut anon_seq: u32 = 0;
    for child in valid_named_children(tree.root_node()) {
        if child.kind() == "import_declaration" {
            emit_import_declaration(child, text, &mut map, &mut anon_seq);
        }
    }
    map
}

fn emit_import_declaration(node: Node, src: &str, map: &mut ImportMap, anon_seq: &mut u32) {
    for wrapper in valid_named_children(node) {
        match wrapper.kind() {
            "import_spec" => emit_spec(wrapper, src, map, anon_seq),
            "import_spec_list" => {
                for spec in valid_named_children(wrapper) {
                    if spec.kind() == "import_spec" {
                        emit_spec(spec, src, map, anon_seq);
                    }
                }
            }
            _ => {}
        }
    }
}

fn emit_spec(spec: Node, src: &str, map: &mut ImportMap, anon_seq: &mut u32) {
    let Some(path_node) = spec.child_by_field_name("path") else {
        return;
    };
    let Some(specifier) = string_literal_text(path_node, src) else {
        return;
    };
    let Some(last_segment) = specifier.rsplit('/').next().map(str::to_string) else {
        return; // an empty import path literal — nothing to bind, never guessed.
    };

    let name_field = spec
        .child_by_field_name("name")
        .filter(|n| !n.is_error() && !n.is_missing());
    match name_field {
        Some(name_node) if name_node.kind() == "package_identifier" => {
            let alias = node_text(name_node, src).to_string();
            insert(map, alias, specifier, last_segment);
        }
        Some(name_node) if name_node.kind() == "dot" => {
            insert(map, anon_key("dot", anon_seq), specifier, "*".to_string());
        }
        Some(name_node) if name_node.kind() == "blank_identifier" => {
            insert(map, anon_key("blank", anon_seq), specifier, "_".to_string());
        }
        _ => insert(map, last_segment.clone(), specifier, last_segment),
    }
}

fn insert(map: &mut ImportMap, local: String, specifier: String, original: String) {
    map.insert(
        local,
        ImportBinding {
            specifier,
            original,
            deferred: false,
            type_only: false,
        },
    );
}

/// A collision-free synthetic key for a dot/blank import — module doc's binding-name convention.
fn anon_key(label: &str, seq: &mut u32) -> String {
    let key = format!("__{label}_import_{}__", *seq);
    *seq += 1;
    key
}

#[cfg(test)]
mod tests;
