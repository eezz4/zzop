//! Pinned node-kind vocabulary — crate root doc's tree-sitter discipline. Every grammar node-kind
//! string this crate's `Node::kind()` matches/compares against anywhere (`lang::*`, `adapters::*`,
//! `util`) is listed here; `tests::node_kinds_are_pinned_to_the_grammar` asserts each one is a REAL
//! kind in the compiled `tree_sitter_go::LANGUAGE`. Keep this list in exact sync with every `.kind()
//! == "..."` / `match node.kind() { ... }` string literal elsewhere in this crate — a mismatch here
//! (an entry that's no longer matched anywhere, or a matched kind missing from this list) is a review
//! smell, not just a test-coverage gap.
pub(crate) const PINNED_NODE_KINDS: &[&str] = &[
    // Root-level hopeless-input gate (crate root `parse_tree`)
    "package_clause",
    // Declarations (`lang::symbols`, `lang::imports`)
    "function_declaration",
    "method_declaration",
    "type_declaration",
    "type_spec",
    "type_alias",
    "struct_type",
    "interface_type",
    "const_declaration",
    "const_spec",
    "var_declaration",
    "var_spec",
    "var_spec_list",
    "import_declaration",
    "import_spec",
    "import_spec_list",
    "package_identifier",
    "dot",
    "blank_identifier",
    // Signatures / receivers (`lang::symbols`, `adapters::*` binding recognition)
    "parameter_declaration",
    "variadic_parameter_declaration",
    "pointer_type",
    "qualified_type",
    "statement_list",
    // Identifiers/literals (`lang::used_names`, `util::string_literal_text`)
    "identifier",
    "type_identifier",
    "selector_expression",
    "interpreted_string_literal",
    "raw_string_literal",
    // Statements/expressions (`lang::used_names`, `adapters::net_http`/`gin`/`http_clients`)
    "short_var_declaration",
    "assignment_statement",
    "expression_list",
    "call_expression",
];

#[cfg(test)]
mod tests;
