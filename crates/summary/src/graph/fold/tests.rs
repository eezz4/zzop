use super::*;

#[test]
fn no_fold_returns_the_path_untouched_and_discloses_nothing() {
    let f = Fold::of(None);
    assert!(!f.is_on());
    assert_eq!(
        f.rel("crates/engine/src/lib.rs"),
        "crates/engine/src/lib.rs"
    );
    assert_eq!(census(f, 10, 10, 0, 5, 5), "");
}

#[test]
fn depth_zero_is_treated_as_no_fold_so_a_slipped_value_cannot_name_one_box_for_the_tree() {
    let f = Fold::of(Some(0));
    assert!(
        !f.is_on(),
        "a 0 depth must not fold; the CLI rejects it earlier"
    );
    assert_eq!(f.rel("a/b/c.rs"), "a/b/c.rs");
}

#[test]
fn a_fold_keeps_the_leading_segments() {
    assert_eq!(Fold::of(Some(1)).rel("crates/engine/src/lib.rs"), "crates");
    assert_eq!(
        Fold::of(Some(2)).rel("crates/engine/src/lib.rs"),
        "crates/engine"
    );
    assert_eq!(
        Fold::of(Some(3)).rel("crates/engine/src/lib.rs"),
        "crates/engine/src"
    );
}

#[test]
fn a_path_shorter_than_the_depth_stands_for_itself_and_is_counted_as_such() {
    let f = Fold::of(Some(2));
    // One segment, no separator: there is nothing coarser to call it.
    assert_eq!(f.rel("README.md"), "README.md");
    assert!(f.is_unfoldable("README.md"));
    // Exactly at the depth: `a/b.rs` has one separator, fewer than 2, so it is its own box too.
    assert!(f.is_unfoldable("a/b.rs"));
    assert!(!f.is_unfoldable("a/b/c.rs"));
}

#[test]
fn the_census_separates_the_folds_loss_from_the_caps() {
    let out = census(Fold::of(Some(2)), 1337, 32, 0, 2583, 37);
    assert!(out.contains("--fold 2"), "{out}");
    assert!(out.contains("1337 file(s) drawn as 32 box(es)"), "{out}");
    assert!(out.contains("2583 file-level edge(s) as 37"), "{out}");
    // The label's meaning is stated in the picture, not left to a doc a reader does not have open.
    assert!(out.contains("NUMBER OF FILE-LEVEL EDGES"), "{out}");
    // The degenerate-fold warning is unconditional: it is what makes a one-box picture readable AS a
    // one-box picture rather than as "this tree has no structure".
    assert!(out.contains("convention rather than a fact"), "{out}");
    // Nothing unfoldable here, so that line must not appear — an always-on caveat teaches nothing.
    assert!(!out.contains("SINGLE FILE"), "{out}");
}

#[test]
fn unfoldable_boxes_are_named_only_when_some_exist() {
    let out = census(Fold::of(Some(2)), 10, 4, 2, 6, 3);
    assert!(
        out.contains("2 of those box(es) are a SINGLE FILE"),
        "{out}"
    );
}
