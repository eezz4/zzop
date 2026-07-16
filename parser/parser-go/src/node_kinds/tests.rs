use super::PINNED_NODE_KINDS;

/// A grammar upgrade that renames one of `PINNED_NODE_KINDS` fails HERE, loudly, instead of every
/// extractor that matches on the renamed kind silently returning nothing — crate root doc's tree-sitter
/// discipline.
#[test]
fn node_kinds_are_pinned_to_the_grammar() {
    let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    for kind in PINNED_NODE_KINDS {
        assert_ne!(
            lang.id_for_node_kind(kind, true),
            0,
            "node kind {kind:?} is no longer a named kind in tree-sitter-go — grammar upgrade broke a match"
        );
    }
}

/// `source_file` is the crate's implicit root assumption (`parse_tree`/every top-level walk starts
/// from `tree.root_node()`) — never matched by string comparison anywhere (no code needs to check it),
/// but still worth a direct grammar-shape sanity check here.
#[test]
fn root_kind_is_source_file() {
    let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
    assert_ne!(lang.id_for_node_kind("source_file", true), 0);
}
