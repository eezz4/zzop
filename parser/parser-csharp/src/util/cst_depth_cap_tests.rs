//! The CST-depth cap, tested where the cap can actually be observed.
//!
//! 🔴 The billion-byte stack is not decoration. tree-sitter's own parse needs roughly 15 KB per
//! nesting level — measured: 4,100 levels is the ceiling on a 64 MiB thread and 70,000 fits in 1 GB —
//! so on a default test thread it returns an ERROR TREE long before the cap could ever be reached, and
//! the assertions below pass whether or not the cap exists. This test was written that way first and
//! the invalidation drill caught it (review ledger V129).

/// 📏 A tree past the cap is refused, and an ordinary deep one is not. BOTH halves are the test:
/// before the cap this input aborted the PROCESS, and a cap that also refused real code would trade a
/// crash for silence — the same failure wearing a different hat.
#[test]
fn a_tree_past_the_depth_cap_is_refused_and_an_ordinary_deep_one_is_not() {
    std::thread::Builder::new()
        .stack_size(1024 * 1024 * 1024)
        .spawn(|| {
            let deep = format!(
                "class C {{ void M(bool b) {{ var x = {}1; }} }}",
                "b ? 1 : ".repeat(20_000)
            );
            let ordinary = format!(
                "class C {{ void M(bool b) {{ var x = {}1; }} }}",
                "b ? 1 : ".repeat(64)
            );
            assert!(
                crate::parse_tree(&deep).is_none(),
                "a tree past the cap must be refused, not walked"
            );
            assert!(
                crate::parse_tree(&ordinary).is_some(),
                "ordinary nesting must still parse -- the cap is not a ban on depth"
            );
        })
        .expect("spawn")
        .join()
        .expect("the depth check itself must not recurse");
}
