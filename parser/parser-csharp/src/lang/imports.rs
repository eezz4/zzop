//! `using`-directive extraction -> `zzop_core::ImportMap`. v1 scope: every `using_directive` reachable
//! at FILE or NAMESPACE-BLOCK scope — C#'s grammar allows a `using` directive both at file (top) level
//! and nested inside a `namespace { ... }` block (unlike Go/Java, where an import can only ever appear
//! at file scope), so this module's walk recurses THROUGH a block `namespace_declaration`'s own body
//! the same "namespace transparent" way `lang::symbols::walk_top_level` does, collecting every
//! `using_directive` found there. A `using` can never appear inside a type body, so the walk never
//! descends past a `declaration_list`.
//!
//! ## Grammar shape: `using_directive`'s target is UNFIELDED except for the alias form
//! `using_directive: seq(optional('global'), 'using', choice(seq(optional('unsafe'), field('name',
//! $.identifier), '=', $.type), seq(repeat(choice('static', 'unsafe')), $._name)), ';')` — the `name`
//! FIELD only exists for the aliased form (`using X = Y;`, where `name` = `X`); the plain/static form
//! has no field at all, so its target is found as the node's own remaining named child (whichever one
//! is NOT the `name` field, when present) rather than `child_by_field_name`. `global`/`static` are
//! anonymous tokens (`util::has_anonymous_child`), mirroring `lang::symbols`' identical anonymous-token
//! reads for C#'s repeated-keyword grammar shape.
//!
//! ## Binding-name convention (task-pinned, mirrors `zzop_parser_go`/`zzop_parser_java_21::lang::imports`'
//! own documented v1 approximations)
//! - **Aliased** (`using Sys = System.Text;` / `using X = System.Collections.Generic.List<int>;`):
//!   local name = the alias identifier (`Sys`/`X`); `specifier` = the aliased target's own verbatim
//!   source text (works for a plain dotted name AND a closed generic like `List<int>`, since
//!   `node_text` just reads the whole span); `original` = the same alias name as `local` — C# has no
//!   separate "original" to recover for an aliased NAMESPACE (unlike a Java static import's real method
//!   name), the same "own binding name IS the tracked name" convention
//!   `zzop_parser_java_21::lang::imports`'s doc pins for a plain Java import.
//! - **`using static X.Y;`** (brings every static member of `X.Y` into scope, no single local binding
//!   name): a synthetic, collision-free key (`__static_import_N__`), mirroring
//!   `zzop_parser_go::lang::imports`'s dot-import / `zzop_parser_java_21::lang::imports`'s glob-import
//!   synthetic-key convention exactly; `original` = `"*"`, reusing `ImportBinding::original`'s
//!   documented "namespace = `*`" convention.
//! - **`global using X;`**: modeled IDENTICAL to a plain `using X;` — `global` only changes the
//!   directive's PROJECT-WIDE reach (implicit in every file of the compilation), which a single-file
//!   `ImportMap` has no way to represent anyway; not itself a distinct binding shape.
//! - **Plain** (`using System.Text;`): local name = the target's LAST `.`-separated segment (`Text`) —
//!   the same "last-segment approximation" `zzop_parser_go::lang::imports`'s own doc documents for Go's
//!   import binding (C# has no `package`-clause-declared "real" local name to resolve against either,
//!   since a `using` brings a NAMESPACE into scope, not a single symbol).
//!
//! `deferred`/`type_only` are always `false` — C# has neither a lazy-import nor an erased-at-compile-
//! time-type-only import concept.

use tree_sitter::Node;
use zzop_core::{ImportBinding, ImportMap};

use crate::util::{has_anonymous_child, node_text, valid_named_children};

/// Extract this file's `using` bindings — see module doc. Empty on parse failure (never panics).
pub fn parse_imports(text: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(tree) = crate::parse_tree(text) else {
        return map;
    };
    let mut static_seq: u32 = 0;
    collect_usings(tree.root_node(), text, &mut map, &mut static_seq);
    map
}

/// Recurses through a block `namespace_declaration`'s own body — module doc's "namespace transparent"
/// scope.
fn collect_usings(node: Node, src: &str, map: &mut ImportMap, static_seq: &mut u32) {
    for child in valid_named_children(node) {
        match child.kind() {
            "using_directive" => emit_using(child, src, map, static_seq),
            "namespace_declaration" => {
                if let Some(body) = child.child_by_field_name("body") {
                    collect_usings(body, src, map, static_seq);
                }
            }
            _ => {}
        }
    }
}

fn emit_using(node: Node, src: &str, map: &mut ImportMap, static_seq: &mut u32) {
    let name_field = node.child_by_field_name("name");
    let Some(target) = valid_named_children(node)
        .into_iter()
        .find(|c| Some(c.id()) != name_field.map(|n| n.id()))
    else {
        return; // no target at all — never guessed.
    };
    let specifier = node_text(target, src).to_string();
    if specifier.is_empty() {
        return;
    }

    if let Some(alias) = name_field {
        let local = node_text(alias, src).to_string();
        let original = local.clone();
        insert(map, local, specifier, original);
        return;
    }

    if has_anonymous_child(node, "static") {
        insert(map, static_key(static_seq), specifier, "*".to_string());
        return;
    }

    let Some(last_segment) = specifier.rsplit('.').next().map(str::to_string) else {
        return;
    };
    // Key by the FULL specifier, not the last segment: C# legally permits `using A.Models; using
    // B.Models;` (types stay qualified — unlike Go/Java, which forbid two same-simple-name imports at
    // compile time), so a last-segment key would collide in the `ImportMap` BTreeMap and silently drop
    // one witnessed namespace from the dep graph AND the census. The full specifier is unique per
    // distinct namespace (only a redundant duplicate `using` collides, harmlessly); no C# `ImportMap`
    // consumer reads the key — `merge_csharp_dep_edges`/census both read `binding.specifier`.
    insert(map, specifier.clone(), specifier, last_segment);
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

fn static_key(seq: &mut u32) -> String {
    let key = format!("__static_import_{}__", *seq);
    *seq += 1;
    key
}

#[cfg(test)]
mod tests;
