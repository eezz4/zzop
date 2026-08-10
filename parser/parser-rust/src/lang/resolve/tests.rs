use super::*;

/// `rust_import_candidates` with no manifest-declared target roots — the shape every test here except
/// the declared-root ones wants. Spelled once so adding an input to the resolver does not rewrite 28
/// call sites; the declared-root tests call the real function directly with a set.
fn candidates(specifier: &str, from_file: &str) -> Vec<String> {
    rust_import_candidates(specifier, from_file, &BTreeSet::new())
}

fn roots(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

#[test]
fn crate_path_anchors_at_the_rightmost_src_root_and_covers_item_vs_module() {
    // `crate::a::b` from `myapp/src/routes/users.rs` — rightmost `/src/` anchors at `myapp/src`.
    assert_eq!(
        candidates("crate::a::b", "myapp/src/routes/users.rs"),
        vec![
            "myapp/src/a/b.rs".to_string(),
            "myapp/src/a/b/mod.rs".to_string(),
            "myapp/src/a.rs".to_string(),
            "myapp/src/a/mod.rs".to_string(),
        ]
    );
}

#[test]
fn crate_path_three_deep() {
    assert_eq!(
        candidates("crate::a::b::c", "src/lib.rs"),
        vec![
            "src/a/b/c.rs".to_string(),
            "src/a/b/c/mod.rs".to_string(),
            "src/a/b.rs".to_string(),
            "src/a/b/mod.rs".to_string(),
        ]
    );
}

#[test]
fn crate_path_direct_item_falls_back_to_lib_and_main_not_src_dot_rs() {
    // `crate::VERSION` from the crate root itself — the crate root file is always `lib.rs`/`main.rs`,
    // never `src.rs`/`src/mod.rs`.
    assert_eq!(
        candidates("crate::VERSION", "myapp/src/lib.rs"),
        vec![
            "myapp/src/VERSION.rs".to_string(),
            "myapp/src/VERSION/mod.rs".to_string(),
            "myapp/src/lib.rs".to_string(),
            "myapp/src/main.rs".to_string(),
        ]
    );
}

#[test]
fn self_path_from_a_root_shaped_file_anchors_at_its_own_directory() {
    // `self::a` from `src/lib.rs` — lib.rs is root-shaped, so its children live in `src/` itself, and
    // this also happens to be crate-root level (single segment) -> lib.rs/main.rs fallback.
    assert_eq!(
        candidates("self::a", "src/lib.rs"),
        vec![
            "src/a.rs".to_string(),
            "src/a/mod.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/main.rs".to_string(),
        ]
    );
}

#[test]
fn self_path_from_a_non_root_file_anchors_in_a_same_named_child_directory() {
    // `self::a` from `src/routes.rs` — non-root basename, so its children live in `src/routes/`, NOT
    // `src/`. Not crate-root level, so the parent fallback is `src/routes.rs`/`src/routes/mod.rs`.
    assert_eq!(
        candidates("self::a", "src/routes.rs"),
        vec![
            "src/routes/a.rs".to_string(),
            "src/routes/a/mod.rs".to_string(),
            "src/routes.rs".to_string(),
            "src/routes/mod.rs".to_string(),
        ]
    );
}

#[test]
fn mod_decl_child_dir_subtlety_root_file_vs_non_root_file() {
    // The mandatory Rust-2018 subtlety this crate's resolve module must get right: `mod x;` (encoded
    // `self::x` by `lang::imports`) resolves differently depending on whether the DECLARING file is
    // root-shaped or not.
    assert_eq!(
        candidates("self::x", "src/lib.rs"),
        vec![
            "src/x.rs".to_string(),
            "src/x/mod.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/main.rs".to_string(),
        ],
        "mod x; from a root file (lib.rs) anchors directly in src/"
    );
    assert_eq!(
        candidates("self::x", "src/foo.rs"),
        vec![
            "src/foo/x.rs".to_string(),
            "src/foo/x/mod.rs".to_string(),
            "src/foo.rs".to_string(),
            "src/foo/mod.rs".to_string(),
        ],
        "mod x; from a non-root file (foo.rs) anchors in src/foo/, not src/"
    );
}

#[test]
fn super_path_walks_up_one_module_from_a_non_root_file() {
    // `super::a` from `app/routes/users.rs` (no `src/` segment present at all — crate_src_root falls
    // back to the tree root, so this is deliberately NOT crate-root level).
    assert_eq!(
        candidates("super::a", "app/routes/users.rs"),
        vec![
            "app/routes/a.rs".to_string(),
            "app/routes/a/mod.rs".to_string(),
            "app/routes.rs".to_string(),
            "app/routes/mod.rs".to_string(),
        ]
    );
}

#[test]
fn super_path_reaching_exactly_the_crate_root_also_gets_lib_main_fallback() {
    // `super::VERSION` from `src/foo.rs` walks up exactly to the crate root — the same lib.rs/main.rs
    // special-case `crate::VERSION` gets applies here too (module doc: the crate-root detection is
    // anchor-based, not head-keyword-based).
    assert_eq!(
        candidates("super::VERSION", "src/foo.rs"),
        vec![
            "src/VERSION.rs".to_string(),
            "src/VERSION/mod.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/main.rs".to_string(),
        ]
    );
}

#[test]
fn external_crate_head_yields_empty_vec() {
    assert!(candidates("serde::Deserialize", "src/lib.rs").is_empty());
    assert!(candidates("tokio::spawn", "src/lib.rs").is_empty());
}

#[test]
fn std_core_alloc_family_yields_empty_vec() {
    assert!(candidates("std::collections::HashMap", "src/lib.rs").is_empty());
    assert!(candidates("core::fmt::Debug", "src/lib.rs").is_empty());
    assert!(candidates("alloc::vec::Vec", "src/lib.rs").is_empty());
}

#[test]
fn bare_head_alone_with_no_further_segment_yields_empty_vec() {
    assert!(candidates("crate", "src/lib.rs").is_empty());
    assert!(candidates("self", "src/lib.rs").is_empty());
    assert!(candidates("super", "src/lib.rs").is_empty());
}

#[test]
fn candidates_are_deduped_and_never_contain_duplicates() {
    let out = candidates("crate::a::b", "src/lib.rs");
    let mut seen = std::collections::HashSet::new();
    assert!(out.iter().all(|c| seen.insert(c.clone())), "{out:?}");
}

#[test]
fn no_src_segment_falls_back_to_the_tree_root_for_crate_paths() {
    assert_eq!(
        candidates("crate::a", "flatlayout/lib.rs"),
        vec![
            "a.rs".to_string(),
            "a/mod.rs".to_string(),
            "lib.rs".to_string(),
            "main.rs".to_string(),
        ]
    );
}

// --- `#[path = "..."]` module declarations ---
//
// The literal anchors at `dirname(from_file)`, NOT at the convention's child-anchor dir. Every case
// below is a shape this repo actually ships; the two that are not (absolute, climbing out) pin the
// refusals.

#[test]
fn path_attr_resolves_relative_to_the_declaring_files_own_directory() {
    // The shape that made a module look dead to a grep for its own stem: the module is named `tests`
    // but its file is `resolve_tests.rs`.
    assert_eq!(
        candidates("#path::resolve_tests.rs", "parser/p/src/lang/resolve.rs"),
        vec!["parser/p/src/lang/resolve_tests.rs".to_string()]
    );
}

#[test]
fn path_attr_does_not_use_the_convention_child_anchor_dir() {
    // `resolve.rs` is a NON-root basename, so `self::tests` would anchor one segment deeper, at
    // `.../resolve/`. `#[path]` does not. This asserts the two disagree — the reason the head exists.
    let by_attr = candidates("#path::tests.rs", "parser/p/src/lang/resolve.rs");
    let by_convention = candidates("self::tests", "parser/p/src/lang/resolve.rs");
    assert_eq!(by_attr, vec!["parser/p/src/lang/tests.rs".to_string()]);
    assert!(
        by_convention.contains(&"parser/p/src/lang/resolve/tests.rs".to_string()),
        "{by_convention:?}"
    );
    assert!(!by_convention.contains(&by_attr[0]), "{by_convention:?}");
}

#[test]
fn path_attr_walks_up_out_of_the_crate_and_lands_in_a_sibling_tree() {
    // Eight parser crates share one file this way; it was drawn as an island.
    assert_eq!(
        candidates(
            "#path::../../tests/input_strategy.rs",
            "parser/parser-rust/tests/no_panic_proptest.rs"
        ),
        vec!["parser/tests/input_strategy.rs".to_string()]
    );
}

#[test]
fn path_attr_without_an_rs_suffix_names_that_exact_file_and_nothing_else() {
    // rustc reads the literal AS A FILENAME — no extension inference, no `mod.rs` fallback. Verified
    // against rustc 1.97.1: an extensionless file named `alt` compiles, while a tree with only `alt.rs`
    // fails with `couldn't read src\alt`.
    //
    // This test asserted the OPPOSITE until 2026-08-06 (`["src/alt.rs", "src/alt/mod.rs"]`), pinning a
    // behaviour rustc does not have. Those two candidates can only ever match a file the declaration did
    // not name — an invented edge, which is the one outcome this resolver refuses everywhere else.
    assert_eq!(
        candidates("#path::alt", "src/lib.rs"),
        vec!["src/alt".to_string()]
    );
}

#[test]
fn path_attr_climbing_above_the_tree_root_is_refused_not_clamped() {
    // An in-tree path invented for an out-of-tree target is exactly the guess this crate refuses.
    assert!(candidates("#path::../../x.rs", "src/lib.rs").is_empty());
}

#[test]
fn path_attr_refuses_absolute_and_empty_literals() {
    assert!(candidates("#path::/etc/x.rs", "src/lib.rs").is_empty());
    assert!(candidates("#path::", "src/lib.rs").is_empty());
    assert!(candidates("#path::C:/x.rs", "src/lib.rs").is_empty());
}

#[test]
fn path_attr_head_is_not_reachable_as_an_ordinary_rust_path() {
    // `#` cannot begin an identifier, so no real specifier can land in the arm. Guards the choice of
    // spelling: if the head ever became identifier-shaped, this test is where that shows up.
    assert!(!PATH_ATTR_HEAD
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_'));
    assert!(PATH_ATTR_PREFIX.starts_with(PATH_ATTR_HEAD));
}

// --- manifest-declared target roots -------------------------------------------------------------------
// A cargo target root is a CRATE root whatever it is named, so both `self::` (its children are siblings)
// and `crate::` (it IS the crate) must anchor at its own directory. Judged by basename alone, the file
// reads as a 2018-style directory module and both anchors land one segment too deep — measured on this
// repo as 12 self-edges plus 77 files with no incoming edge.

#[test]
fn a_declared_target_root_anchors_its_children_as_siblings_not_in_a_named_subdir() {
    let declared = roots(&["rules/dsl/db/db.rs"]);
    // `mod queries;` inside the pack target — `lang::imports` encodes it as `self::queries`.
    assert_eq!(
        rust_import_candidates("self::queries", "rules/dsl/db/db.rs", &declared),
        vec![
            // The real target — a SIBLING of the declaring file, not `db/db/queries.rs`.
            "rules/dsl/db/queries.rs".to_string(),
            "rules/dsl/db/queries/mod.rs".to_string(),
            // The item-vs-module fallback: the convention pair first (they simply do not exist here
            // and are dropped by the existence check downstream), then the declared file.
            "rules/dsl/db/lib.rs".to_string(),
            "rules/dsl/db/main.rs".to_string(),
            "rules/dsl/db/db.rs".to_string(),
        ],
        "a declared root's children must be resolved from its own directory"
    );
}

#[test]
fn a_declared_target_beside_a_real_lib_rs_does_not_evict_the_convention_roots() {
    // The exclusive-else defect, exercised: `[[bin]] path = "src/tool.rs"` declared in the SAME dir
    // as an ordinary `src/lib.rs`. `use crate::thing` from a sibling module used to offer ONLY the
    // declared bin as the item-vs-module fallback — `lib.rs`/`main.rs` were evicted, so the edge
    // minted for an item declared in lib.rs landed on the bin. Convention first, declared appended;
    // nonexistent candidates are filtered by the existence check downstream
    // (`zzop-engine`'s `resolve_rust_import` takes the first candidate present in the tree).
    let declared = roots(&["src/tool.rs"]);
    assert_eq!(
        rust_import_candidates("crate::thing", "src/foo.rs", &declared),
        vec![
            "src/thing.rs".to_string(),
            "src/thing/mod.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/main.rs".to_string(),
            "src/tool.rs".to_string(),
        ]
    );
}

#[test]
fn without_the_declaration_the_same_file_reads_as_a_directory_module_and_finds_itself() {
    // The defect, kept as a test so the fix cannot be silently undone: with an EMPTY root set the third
    // candidate is the declaring file itself, which is exactly the self-edge that was being emitted.
    let got = candidates("self::queries", "rules/dsl/db/db.rs");
    assert_eq!(got[0], "rules/dsl/db/db/queries.rs");
    assert!(
        got.contains(&"rules/dsl/db/db.rs".to_string()),
        "the item-vs-module fallback lands on the declaring file: {got:?}"
    );
}

#[test]
fn crate_anchors_at_the_declared_roots_directory_when_there_is_no_src_segment() {
    // A submodule of a pack target saying `use crate::x`. With no `src/` anywhere above it, the old
    // reading anchored at the TREE ROOT and produced `x.rs` — an edge to a file in another project.
    let declared = roots(&["rules/dsl/db/db.rs"]);
    assert_eq!(
        rust_import_candidates(
            "crate::writes::helper",
            "rules/dsl/db/queries.rs",
            &declared
        ),
        vec![
            "rules/dsl/db/writes/helper.rs".to_string(),
            "rules/dsl/db/writes/helper/mod.rs".to_string(),
            "rules/dsl/db/writes.rs".to_string(),
            "rules/dsl/db/writes/mod.rs".to_string(),
        ]
    );
}

#[test]
fn the_longer_of_src_and_declared_wins_because_the_two_nest_both_ways() {
    // A declared target INSIDE `src/` — `[[bin]] path = "src/bin/tool.rs"`. For a file beside it the
    // declared reading is deeper than `src/` and must win.
    let declared = roots(&["packages/mcp/src/bin/zzop-mcp.rs"]);
    assert_eq!(
        rust_import_candidates("crate::opt", "packages/mcp/src/bin/helper.rs", &declared)[0],
        "packages/mcp/src/bin/opt.rs",
    );
    // ...and for a file OUTSIDE that bin's directory, `src/` must still win — the declared root's
    // directory is not an ancestor, so it never enters the comparison.
    assert_eq!(
        rust_import_candidates("crate::opt", "packages/mcp/src/lib.rs", &declared)[0],
        "packages/mcp/src/opt.rs",
    );
}

#[test]
fn a_declared_root_elsewhere_in_the_tree_does_not_reanchor_an_unrelated_file() {
    let declared = roots(&["rules/dsl/db/db.rs"]);
    assert_eq!(
        candidates("crate::a::b", "crates/engine/src/lib.rs"),
        rust_import_candidates("crate::a::b", "crates/engine/src/lib.rs", &declared),
        "an unrelated declared root must change nothing"
    );
}
