use super::*;

#[test]
fn crate_path_anchors_at_the_rightmost_src_root_and_covers_item_vs_module() {
    // `crate::a::b` from `myapp/src/routes/users.rs` — rightmost `/src/` anchors at `myapp/src`.
    assert_eq!(
        rust_import_candidates("crate::a::b", "myapp/src/routes/users.rs"),
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
        rust_import_candidates("crate::a::b::c", "src/lib.rs"),
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
        rust_import_candidates("crate::VERSION", "myapp/src/lib.rs"),
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
        rust_import_candidates("self::a", "src/lib.rs"),
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
        rust_import_candidates("self::a", "src/routes.rs"),
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
        rust_import_candidates("self::x", "src/lib.rs"),
        vec![
            "src/x.rs".to_string(),
            "src/x/mod.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/main.rs".to_string(),
        ],
        "mod x; from a root file (lib.rs) anchors directly in src/"
    );
    assert_eq!(
        rust_import_candidates("self::x", "src/foo.rs"),
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
        rust_import_candidates("super::a", "app/routes/users.rs"),
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
        rust_import_candidates("super::VERSION", "src/foo.rs"),
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
    assert!(rust_import_candidates("serde::Deserialize", "src/lib.rs").is_empty());
    assert!(rust_import_candidates("tokio::spawn", "src/lib.rs").is_empty());
}

#[test]
fn std_core_alloc_family_yields_empty_vec() {
    assert!(rust_import_candidates("std::collections::HashMap", "src/lib.rs").is_empty());
    assert!(rust_import_candidates("core::fmt::Debug", "src/lib.rs").is_empty());
    assert!(rust_import_candidates("alloc::vec::Vec", "src/lib.rs").is_empty());
}

#[test]
fn bare_head_alone_with_no_further_segment_yields_empty_vec() {
    assert!(rust_import_candidates("crate", "src/lib.rs").is_empty());
    assert!(rust_import_candidates("self", "src/lib.rs").is_empty());
    assert!(rust_import_candidates("super", "src/lib.rs").is_empty());
}

#[test]
fn candidates_are_deduped_and_never_contain_duplicates() {
    let out = rust_import_candidates("crate::a::b", "src/lib.rs");
    let mut seen = std::collections::HashSet::new();
    assert!(out.iter().all(|c| seen.insert(c.clone())), "{out:?}");
}

#[test]
fn no_src_segment_falls_back_to_the_tree_root_for_crate_paths() {
    assert_eq!(
        rust_import_candidates("crate::a", "flatlayout/lib.rs"),
        vec![
            "a.rs".to_string(),
            "a/mod.rs".to_string(),
            "lib.rs".to_string(),
            "main.rs".to_string(),
        ]
    );
}
