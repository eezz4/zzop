//! Pinned node-kind vocabulary — crate root doc's tree-sitter discipline, mirroring
//! `zzop_parser_go::node_kinds`'s identical role. Every grammar NAMED node-kind string this crate's
//! `Node::kind()` matches/compares against anywhere is listed in [`PINNED_NODE_KINDS`]; every ANONYMOUS
//! keyword-token string (`"public"`, `"static"`, ...) matched via `util::has_modifier_keyword` is listed
//! separately in [`PINNED_ANONYMOUS_KEYWORDS`] — `tree_sitter::Language::id_for_node_kind`'s `named`
//! parameter distinguishes the two lookups, so one combined list would silently pass a renamed anonymous
//! token (looked up with `named: true`, always `0` regardless of whether the token still exists).
//! `tests::node_kinds_are_pinned_to_the_grammar` / `tests::anonymous_keywords_are_pinned_to_the_grammar`
//! assert each against the compiled `tree_sitter_java::LANGUAGE`.
//!
//! The REVERSE direction — a kind the code matches that never reached this list — is machine-checked
//! too, by `tests::every_grammar_node_kind_literal_in_this_crate_is_pinned` (added 2026-07-28; it
//! immediately found `variable_declarator`/`enum_constant`/`field_access`/`superclass`). Without it a
//! forward-only pin can only ever validate itself.
pub(crate) const PINNED_NODE_KINDS: &[&str] = &[
    // Root-level hopeless-input gate (crate root `parse_tree`/`TOP_LEVEL_DECLARATION_KINDS`)
    "package_declaration",
    "import_declaration",
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
    "module_declaration",
    // Imports (`lang::imports`)
    "asterisk",
    // Loop spans (`lang::loop_spans`)
    "for_statement",
    "enhanced_for_statement",
    "while_statement",
    "do_statement",
    // Symbols (`lang::symbols`)
    "enum_body_declarations",
    "method_declaration",
    "constructor_declaration",
    "compact_constructor_declaration",
    "field_declaration",
    "constant_declaration",
    "modifiers",
    // Identifiers/types (`util`, `lang::symbols`, `lang::used_names`, `project::collect`)
    "identifier",
    "type_identifier",
    "scoped_identifier",
    "scoped_type_identifier",
    "generic_type",
    // Calls (`lang::calls`)
    "method_invocation",
    // Process-exec constructor shape (`lang::call_sites` — `new ProcessBuilder(...)`)
    "object_creation_expression",
    "local_variable_declaration",
    "formal_parameter",
    // Declarator/receiver shapes (`lang::used_names`, `spring_security`)
    "variable_declarator",
    "enum_constant",
    "field_access",
    // Superclass node (`util`'s own tests walk to it by KIND; `project::collect` reaches the same node
    // through the `superclass` FIELD, which is a separate grammar vocabulary — see the reverse pin's
    // field-name stripping)
    "superclass",
    // Literals/annotations (`util`)
    "string_literal",
    "annotation",
    "marker_annotation",
];

/// See module doc — anonymous keyword tokens, looked up with `named: false`.
pub(crate) const PINNED_ANONYMOUS_KEYWORDS: &[&str] =
    &["public", "protected", "private", "static", "final"];

#[cfg(test)]
mod tests;
