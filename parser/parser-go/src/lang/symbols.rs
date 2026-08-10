//! Top-level `SourceSymbol` extraction — v1 scope: `source_file`'s own top-level named children only
//! (mirrors `zzop_parser_rust::lang::symbols`'s identical "top-level only" v1 scope — a `func` nested
//! inside another function body, or a type declared inside a function, is out of scope). A top-level
//! child that is itself an error/missing subtree is skipped (`util::valid_named_children`); a
//! grouped-declaration wrapper (`const (...)`/`type (...)`/`var (...)`) whose SPEC child is broken is
//! likewise skipped one spec at a time, never blanking the whole declaration.
//!
//! ## `exported`
//! Go's OWN rule (crate root doc references this too): the first Unicode letter of the name is
//! uppercase (`util::is_exported`) — there is no visibility keyword to inspect, unlike
//! `zzop_parser_rust`'s `pub`-spelling check.
//!
//! ## Kind mapping (judgment calls — `SourceSymbolKind` has no Go-shaped variants either)
//! - `func` (top-level) -> `Function`.
//! - `func (r *Recv) Name(...)` / `func (r Recv) Name(...)` (method) -> `Function`, named
//!   `"Recv.Name"` — the pointer `*` is stripped (`Recv`, not `*Recv`), mirroring
//!   `zzop_parser_rust::lang::symbols`'s `impl`-block `Type.method` convention exactly. A receiver
//!   whose type is anything other than a plain (possibly pointer-to) `type_identifier` — a generic
//!   receiver `func (r *Foo[T]) M()`, for instance — is never guessed at: that ONE method is skipped
//!   (not the whole file), the same "skip this item, not the file" discipline
//!   `zzop_parser_rust::lang::symbols::type_leaf_name` uses for a non-path `impl` self-type.
//! - `type X struct { ... }` -> `Class`; `type X interface { ... }` -> `Interface`; any other
//!   `type X <T>` spec (alias to a named/pointer/slice/map/... type) -> `Type`; `type X = Y` (a Go
//!   TYPE ALIAS, distinct grammar node from a defined type) -> `Type` too — an alias has no
//!   struct/interface shape of its own to distinguish.
//! - `const`/`var` top-level specs -> `Const` for BOTH: `SourceSymbolKind` has no "Variable" variant
//!   (the same gap `zzop_parser_rust::lang::symbols` documents for `static`), so a top-level `var` is
//!   mapped onto the closest existing kind, same as Rust's `static` -> `Const`. This intentionally
//!   erases the const/var distinction in the symbol kind (still recoverable from source if ever
//!   needed — `SourceSymbol` carries no raw-keyword field).
//!
//! ## Grouped declarations
//! `const ( A = 1; B = 2 )` / `var ( A = 1; B = 2 )` / `type ( X struct{}; Y int )` each emit one
//! symbol PER SPEC — and, since a single spec may itself declare several comma-separated names
//! (`const A, B = 1, 2`; `var X, Y int`), one symbol per NAME within that spec (all such names share
//! the spec's own line). This is a strict superset of "one symbol per spec" for the common
//! one-name-per-spec case the task brief's example uses, and the only choice that does not silently
//! drop a legally-declared name.
//!
//! ## `body_start`/`body_end`
//! `zzop_core::SourceSymbol`'s "Body span contract" owns the rule and this crate does not restate it:
//! `body_start` = the `func` declaration's own line, `body_end` = its body block's closing `}`.
//!
//! Only `Function`-kind symbols (top-level `func` and methods) get one, and that is a statement about
//! GO, not a gap: no Go declaration shape has a statement body other than a `func`. A `type X struct`
//! / `interface` / alias, and a `const`/`var` spec, carry a FIELD or VALUE list — projecting a span
//! over one would make `dsl::method_scan::gates::drop_outer_spans` treat it as scannable and claim a
//! per-member containment the language does not have, so every non-`Function` symbol here carries
//! `None`/`None` and must keep doing so (the same call `zzop_parser_rust::lang::symbols` makes for a
//! `struct`/`enum`/`trait`). A body-LESS func (a `//go:linkname`/cgo-style forward declaration,
//! syntactically legal but rare) is `None`/`None` for the same reason: there is no region.

use tree_sitter::Node;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::util::{end_line_of, is_exported, line_of, node_text, valid_named_children};

/// Extract this file's top-level symbols — see module doc. Empty on parse failure (never panics, and
/// never on a partial in-file error — module doc's "skip this item, not the file").
pub fn parse_symbols(rel: &str, text: &str) -> Vec<SourceSymbol> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in valid_named_children(tree.root_node()) {
        emit_top_level(rel, child, text, &mut out);
    }
    out
}

fn emit_top_level(rel: &str, node: Node, src: &str, out: &mut Vec<SourceSymbol>) {
    match node.kind() {
        "function_declaration" => {
            if let Some(sym) = function_symbol(rel, node, src) {
                out.push(sym);
            }
        }
        "method_declaration" => {
            if let Some(sym) = method_symbol(rel, node, src) {
                out.push(sym);
            }
        }
        "type_declaration" => emit_type_declaration(rel, node, src, out),
        "const_declaration" => emit_spec_group(rel, node, src, SourceSymbolKind::Const, out),
        "var_declaration" => emit_var_declaration(rel, node, src, out),
        _ => {}
    }
}

fn function_symbol(rel: &str, node: Node, src: &str) -> Option<SourceSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, src).to_string();
    let (body_start, body_end) = node
        .child_by_field_name("body")
        .map(|b| body_line_range(node, b))
        .unwrap_or((None, None));
    Some(SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported: is_exported(&name),
        name,
        kind: SourceSymbolKind::Function,
        line: line_of(node),
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    })
}

/// `func (r *Recv) Name(...)` / `func (r Recv) Name(...)` -> `"Recv.Name"` — module doc's method
/// mapping. `None` when the receiver's type isn't a plain (possibly pointer-to) named type (skip this
/// one method, never the file).
fn method_symbol(rel: &str, node: Node, src: &str) -> Option<SourceSymbol> {
    let receiver_list = node.child_by_field_name("receiver")?;
    let receiver_decl = valid_named_children(receiver_list).into_iter().next()?;
    let receiver_type = receiver_decl.child_by_field_name("type")?;
    let recv_name = receiver_type_name(receiver_type, src)?;

    let method_name_node = node.child_by_field_name("name")?;
    let method_name = node_text(method_name_node, src);
    let name = format!("{recv_name}.{method_name}");
    let (body_start, body_end) = node
        .child_by_field_name("body")
        .map(|b| body_line_range(node, b))
        .unwrap_or((None, None));
    Some(SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported: is_exported(method_name),
        name,
        kind: SourceSymbolKind::Function,
        line: line_of(node),
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    })
}

/// The receiver's own type name: `type_identifier` directly (value receiver), or a `pointer_type`
/// wrapping one (pointer receiver, `*` stripped). Any other shape (a generic receiver's
/// `generic_type`, ...) -> `None`, module doc's "skip this one method" rule.
fn receiver_type_name(ty: Node, src: &str) -> Option<String> {
    match ty.kind() {
        "type_identifier" => Some(node_text(ty, src).to_string()),
        "pointer_type" => {
            let inner = valid_named_children(ty).into_iter().next()?;
            (inner.kind() == "type_identifier").then(|| node_text(inner, src).to_string())
        }
        _ => None,
    }
}

/// The span for a `func`/method that HAS a body: the declaration's own line through the body block's
/// closing `}` — `zzop_core::SourceSymbol`'s "Body span contract". A body-less `func` (a
/// `//go:linkname`/cgo-style forward declaration) never reaches here and keeps `None`/`None`.
///
/// This reads NOTHING inside the block, and that deletion is the point. The previous version walked to
/// `statement_list` and took its first/last non-`comment` named child, which made the span depend on
/// `tree-sitter-go`'s treatment of `comment` as an "extra" splice-able anywhere a statement can
/// appear — a leading comment shifted `body_start` (or bailed the whole span to `None`), a trailing
/// one shifted `body_end`, and a comment-only body erased the symbol's scannability entirely. Three
/// regression tests existed for those three shapes; none of them can recur now, because the boundaries
/// come from the declaration and the brace rather than from the body's contents.
fn body_line_range(decl: Node, block: Node) -> (Option<u32>, Option<u32>) {
    (Some(line_of(decl)), Some(end_line_of(block)))
}

/// `type_declaration`'s named children are `type_spec`/`type_alias` DIRECTLY (grouped or not — the
/// grammar never wraps them in a `type_spec_list`) — module doc's "grouped declarations" section.
fn emit_type_declaration(rel: &str, node: Node, src: &str, out: &mut Vec<SourceSymbol>) {
    for spec in valid_named_children(node) {
        let sym = match spec.kind() {
            "type_spec" => type_spec_symbol(rel, spec, src),
            "type_alias" => type_alias_symbol(rel, spec, src),
            _ => None,
        };
        if let Some(sym) = sym {
            out.push(sym);
        }
    }
}

fn type_spec_symbol(rel: &str, spec: Node, src: &str) -> Option<SourceSymbol> {
    let name_node = spec.child_by_field_name("name")?;
    let name = node_text(name_node, src).to_string();
    let ty = spec.child_by_field_name("type")?;
    let kind = match ty.kind() {
        "struct_type" => SourceSymbolKind::Class,
        "interface_type" => SourceSymbolKind::Interface,
        _ => SourceSymbolKind::Type,
    };
    Some(plain_symbol(rel, name, kind, line_of(spec)))
}

fn type_alias_symbol(rel: &str, spec: Node, src: &str) -> Option<SourceSymbol> {
    let name_node = spec.child_by_field_name("name")?;
    let name = node_text(name_node, src).to_string();
    Some(plain_symbol(
        rel,
        name,
        SourceSymbolKind::Type,
        line_of(spec),
    ))
}

/// `const_declaration`'s named children are `const_spec` DIRECTLY (grouped or not) — module doc.
fn emit_spec_group(
    rel: &str,
    node: Node,
    src: &str,
    kind: SourceSymbolKind,
    out: &mut Vec<SourceSymbol>,
) {
    for spec in valid_named_children(node) {
        emit_spec_names(rel, spec, src, kind, out);
    }
}

/// `var_declaration` wraps its spec(s) ONE level deeper: a single `var_spec` (ungrouped) OR one
/// `var_spec_list` (grouped `var (...)`) that itself holds the `var_spec` children — the ONE grammar
/// asymmetry between `const_declaration`/`type_declaration` (direct multiple children) and
/// `var_declaration`/`import_declaration` (single child, spec-or-list) that this crate must unwrap
/// explicitly. See `lang::imports`' own doc for the `import_declaration` counterpart.
fn emit_var_declaration(rel: &str, node: Node, src: &str, out: &mut Vec<SourceSymbol>) {
    for wrapper in valid_named_children(node) {
        match wrapper.kind() {
            "var_spec" => emit_spec_names(rel, wrapper, src, SourceSymbolKind::Const, out),
            "var_spec_list" => emit_spec_group(rel, wrapper, src, SourceSymbolKind::Const, out),
            _ => {}
        }
    }
}

/// One symbol per comma-separated name in a `const_spec`/`var_spec`'s `name` field — module doc's
/// "grouped declarations" section.
///
/// The `kind() == "identifier"` gate is load-bearing: tree-sitter-go 0.25 attaches the `name` FIELD
/// to a `const_spec`'s COMMA tokens as well (`var_spec` does not), so without it `const A, B = …`
/// emitted a third, ghost symbol spelled `","` (id `a.go#,`) into the graph/dead-export/count
/// machinery — the same fielded-comma quirk `lang::string_literals`' name collection filters for.
fn emit_spec_names(
    rel: &str,
    spec: Node,
    src: &str,
    kind: SourceSymbolKind,
    out: &mut Vec<SourceSymbol>,
) {
    let line = line_of(spec);
    let mut cursor = spec.walk();
    for name_node in spec.children_by_field_name("name", &mut cursor) {
        if name_node.is_error() || name_node.is_missing() || name_node.kind() != "identifier" {
            continue;
        }
        let name = node_text(name_node, src).to_string();
        out.push(plain_symbol(rel, name, kind, line));
    }
}

fn plain_symbol(rel: &str, name: String, kind: SourceSymbolKind, line: u32) -> SourceSymbol {
    SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported: is_exported(&name),
        name,
        kind,
        line,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
