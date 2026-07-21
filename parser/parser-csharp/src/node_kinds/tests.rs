use super::{PINNED_ANONYMOUS_KEYWORDS, PINNED_NODE_KINDS};

/// A grammar upgrade that renames one of `PINNED_NODE_KINDS` fails HERE, loudly, instead of every
/// extractor that matches on the renamed kind silently returning nothing — crate root doc's tree-sitter
/// discipline.
#[test]
fn node_kinds_are_pinned_to_the_grammar() {
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    for kind in PINNED_NODE_KINDS {
        assert_ne!(
            lang.id_for_node_kind(kind, true),
            0,
            "node kind {kind:?} is no longer a named kind in tree-sitter-c-sharp — grammar upgrade broke a match"
        );
    }
}

/// See module doc — anonymous keyword tokens, looked up with `named: false`.
#[test]
fn anonymous_keywords_are_pinned_to_the_grammar() {
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    for kw in PINNED_ANONYMOUS_KEYWORDS {
        assert_ne!(
            lang.id_for_node_kind(kw, false),
            0,
            "anonymous token {kw:?} is no longer a token in tree-sitter-c-sharp — grammar upgrade broke a match"
        );
    }
}

/// `compilation_unit` is the crate's implicit root assumption (`parse_tree`/every top-level walk
/// starts from `tree.root_node()`) — never matched by string comparison anywhere, but still worth a
/// direct grammar-shape sanity check here.
#[test]
fn root_kind_is_compilation_unit() {
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    assert_ne!(lang.id_for_node_kind("compilation_unit", true), 0);
}
