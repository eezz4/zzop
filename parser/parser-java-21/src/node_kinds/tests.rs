use super::{PINNED_ANONYMOUS_KEYWORDS, PINNED_NODE_KINDS};

/// A grammar upgrade that renames one of `PINNED_NODE_KINDS` fails HERE, loudly, instead of every
/// extractor that matches on the renamed kind silently returning nothing — crate root doc's tree-sitter
/// discipline.
#[test]
fn node_kinds_are_pinned_to_the_grammar() {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    for kind in PINNED_NODE_KINDS {
        assert_ne!(
            lang.id_for_node_kind(kind, true),
            0,
            "node kind {kind:?} is no longer a named kind in tree-sitter-java — grammar upgrade broke a match"
        );
    }
}

/// Same guarantee, for the anonymous keyword tokens `util::has_modifier_keyword` compares against
/// (looked up with `named: false` — module doc).
#[test]
fn anonymous_keywords_are_pinned_to_the_grammar() {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    for kw in PINNED_ANONYMOUS_KEYWORDS {
        assert_ne!(
            lang.id_for_node_kind(kw, false),
            0,
            "keyword token {kw:?} is no longer an anonymous token in tree-sitter-java — grammar upgrade broke a match"
        );
    }
}

/// `program` is the crate's implicit root assumption (`parse_tree`/every top-level walk starts from
/// `tree.root_node()`) — never matched by string comparison anywhere, but still worth a direct
/// grammar-shape sanity check here, mirroring `zzop_parser_go::node_kinds::tests::root_kind_is_source_file`.
#[test]
fn root_kind_is_program() {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    assert_ne!(lang.id_for_node_kind("program", true), 0);
}
