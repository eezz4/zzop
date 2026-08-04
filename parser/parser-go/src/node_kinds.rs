//! Pinned node-kind vocabulary — crate root doc's tree-sitter discipline. Every grammar node-kind
//! string this crate's `Node::kind()` matches/compares against anywhere (`lang::*`, `adapters::*`,
//! `util`) is listed here; `tests::node_kinds_are_pinned_to_the_grammar` asserts each one is a REAL
//! kind in the compiled `tree_sitter_go::LANGUAGE`. Keep this list in exact sync with every `.kind()
//! == "..."` / `match node.kind() { ... }` string literal elsewhere in this crate — a mismatch here
//! (an entry that's no longer matched anywhere, or a matched kind missing from this list) is a review
//! smell, not just a test-coverage gap.
//!
//! Both directions are now MACHINE-checked, which is why the paragraph above is a rule rather than a
//! hope: `tests::node_kinds_are_pinned_to_the_grammar` walks this list against the grammar, and
//! `tests::every_grammar_node_kind_literal_in_this_crate_is_pinned` walks the crate's own source text
//! back against this list. The reverse direction was added 2026-07-28 and immediately found six kinds
//! the code matched and this list had never heard of (`comment`, `composite_literal`,
//! `field_declaration`, `field_declaration_list`, `return_statement`, `unary_expression`) — a list
//! that only ever validated itself.
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
    // Struct field walk (`adapters::gorm` — model field discovery)
    "field_declaration_list",
    "field_declaration",
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
    // Selector halves read by `lang::call_sites::package_selector` (`fmt.Println` / `os.Getenv`)
    "field_identifier",
    "type_identifier",
    "selector_expression",
    "interpreted_string_literal",
    "raw_string_literal",
    // Statements/expressions (`lang::used_names`, `adapters::net_http`/`gin`/`http_clients`)
    "short_var_declaration",
    "assignment_statement",
    "expression_list",
    "call_expression",
    "return_statement",
    // Composite-literal / address-of unwrapping (`adapters::gorm`, `adapters::http_clients::instances`)
    "composite_literal",
    "unary_expression",
    // Trivia skipped when reading a declaration's children (`lang::symbols`)
    "comment",
    // Loop-body line spans (`lang::loop_spans`) — the single node kind covering every Go loop form
    // (classic/condition-only/infinite/range), per that module's own doc.
    "for_statement",
];

#[cfg(test)]
mod tests;
