//! Pinned node-kind vocabulary — crate root doc's tree-sitter discipline, mirroring
//! `zzop_parser_go`/`zzop_parser_java_21::node_kinds`'s identical role. Every grammar NAMED node-kind
//! string this crate's `Node::kind()` matches/compares against anywhere is listed in
//! [`PINNED_NODE_KINDS`]; every ANONYMOUS keyword/token string (`"public"`, `"static"`, `"const"`, ...)
//! matched via `util::has_modifier`/`util::has_anonymous_child` is listed separately in
//! [`PINNED_ANONYMOUS_KEYWORDS`] — `tree_sitter::Language::id_for_node_kind`'s `named` parameter
//! distinguishes the two lookups, so one combined list would silently pass a renamed anonymous token
//! (looked up with `named: true`, always `0` regardless of whether the token still exists).
//!
//! The REVERSE direction — a kind the code matches that never reached this list — is machine-checked
//! too, by `tests::every_grammar_node_kind_literal_in_this_crate_is_pinned` (added 2026-07-28). This
//! crate's list was already complete; its Go and Java siblings' were not, which is exactly why a
//! forward-only pin is not enough: it can only ever validate itself.
pub(crate) const PINNED_NODE_KINDS: &[&str] = &[
    // Root-level hopeless-input gate (crate root `parse_tree`/`TOP_LEVEL_DECLARATION_KINDS`)
    "using_directive",
    "namespace_declaration",
    "file_scoped_namespace_declaration",
    "class_declaration",
    "interface_declaration",
    "struct_declaration",
    "enum_declaration",
    "record_declaration",
    "delegate_declaration",
    "global_statement",
    // Loop spans (`lang::loop_spans`)
    "for_statement",
    "foreach_statement",
    "while_statement",
    "do_statement",
    // Symbols (`lang::symbols`)
    "declaration_list",
    "enum_member_declaration_list",
    "enum_member_declaration",
    "method_declaration",
    "constructor_declaration",
    "property_declaration",
    "field_declaration",
    "variable_declaration",
    "variable_declarator",
    // `crate::project::collect::declarator_value` skips a declarator's `bracketed_argument_list` when
    // locating the initializer expression.
    "bracketed_argument_list",
    "accessor_list",
    "modifier",
    // Imports (`lang::imports`)
    "identifier",
    "qualified_name",
    "generic_name",
    "alias_qualified_name",
    // Attributes (`util::attributes_of`, `adapters::provides`)
    "attribute_list",
    "attribute",
    "attribute_argument_list",
    "attribute_argument",
    // Literals/strings (`util::string_literal_text`, `lang::used_names`, `adapters::http_clients`)
    "string_literal",
    "interpolated_string_expression",
    "string_content",
    "interpolation",
    // EF Core db-table provides (`adapters::ef_core`): `DbSet<T>` property-type reading.
    "type_argument_list",
    "nullable_type",
    // Calls/expressions (`adapters::provides`, `adapters::http_clients`, `lang::used_names`)
    "invocation_expression",
    "member_access_expression",
    "argument_list",
    "argument",
    "object_creation_expression",
    "local_declaration_statement",
    "parameter",
];

/// See module doc — anonymous keyword/token strings, looked up with `named: false`.
pub(crate) const PINNED_ANONYMOUS_KEYWORDS: &[&str] =
    &["public", "static", "const", "readonly", "global", "partial"];

#[cfg(test)]
mod tests;
