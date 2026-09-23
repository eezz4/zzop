use super::*;

#[test]
fn bare_identifier_read_is_collected() {
    let src = "package main\n\nfunc main() {\n\tfoo()\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("foo"));
}

#[test]
fn selector_field_rightmost_is_collected() {
    let src = "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"hi\")\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("Println"));
    // The operand itself is a plain identifier read too.
    assert!(refs.contains("fmt"));
}

#[test]
fn type_reference_is_collected() {
    let src = "package main\n\ntype Foo struct{}\n\nfunc use(x Foo) {}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("Foo"));
}

#[test]
fn function_declaration_name_excluded() {
    let src = "package main\n\nfunc DoThing() {}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(!refs.contains("DoThing"));
}

#[test]
fn parameter_name_excluded_but_type_included() {
    let src = "package main\n\nfunc f(count int) {\n\tuse(count)\n}\n";
    let refs = parse_local_identifier_refs(src);
    // "count" IS used later as a read (call argument) — that occurrence must still be collected.
    assert!(refs.contains("count"));
    assert!(refs.contains("use"));
}

#[test]
fn const_and_var_spec_names_excluded() {
    let src = "package main\n\nconst Pi = 3\nvar Ready = false\n";
    let refs = parse_local_identifier_refs(src);
    assert!(!refs.contains("Pi"));
    assert!(!refs.contains("Ready"));
}

#[test]
fn type_spec_name_excluded_but_underlying_type_reference_included() {
    let src = "package main\n\ntype Wrapper struct{}\n\ntype Alias Wrapper\n";
    let refs = parse_local_identifier_refs(src);
    assert!(!refs.contains("Alias"));
    assert!(refs.contains("Wrapper"));
}

#[test]
fn short_var_declaration_left_excluded_but_right_included() {
    let src = "package main\n\nfunc f() {\n\tx := helper()\n\t_ = x\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("helper"));
    // `x` on the RIGHT of `_ = x` (a plain assignment, not a fresh binding) IS a read.
    assert!(refs.contains("x"));
}

#[test]
fn plain_reassignment_target_is_included() {
    let src = "package main\n\nfunc f() {\n\tvar x int\n\tx = 5\n\t_ = x\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("x"));
}

#[test]
fn parse_local_identifier_refs_empty_on_hopeless_input() {
    assert!(parse_local_identifier_refs("@@@ ### not go").is_empty());
}

#[test]
fn broken_statement_does_not_blank_out_valid_sibling_reads() {
    let src = "package main\n\nfunc main() {\n\tgood()\n\t&&& broken\n}\n";
    let refs = parse_local_identifier_refs(src);
    assert!(refs.contains("good"));
}

/// 📏 The cost of collecting identifiers must not scale with how DEEP they sit — review ledger V131,
/// and the reason [`super::walk`] threads the parent down instead of calling `node.parent()`.
///
/// This asserts a RATIO, not a wall-clock budget, and that is deliberate: both halves are measured
/// seconds apart in one process, so machine load, CPU scaling and a debug-vs-release build all divide
/// out. The ratio is not a proxy for the property — it IS the property. A per-node `node.parent()` is
/// O(depth), which is invisible in a shallow file and quadratic in a deep one, so only a comparison
/// between two DEPTHS at the same identifier count can see it.
///
/// 📏 Before the fix, one file with 20,000 identifiers at depth 4,000 took 41.8 s through `zzop analyze`
/// against 7.2 s for the already-fixed C# frontend; at 200,000 identifiers it passed 300 s and was
/// killed. The bound below sits far above what depth-independent code does.
#[test]
fn identifier_collection_does_not_get_slower_with_nesting_depth() {
    use std::time::Instant;

    const N: usize = 4_000;
    const DEPTH: usize = 2_000;
    const MAX_RATIO: u32 = 5;

    let args = std::iter::repeat_n("b", N).collect::<Vec<_>>().join(",");
    let deep = format!(
        "package p\nfunc f(b bool) {{ x := {}g({}); _ = x }}",
        "!".repeat(DEPTH),
        args
    );
    let shallow = format!("package p\nfunc f(b bool) {{ x := g({}); _ = x }}", args);

    let elapsed = |src: &str| {
        let t = Instant::now();
        let out = parse_local_identifier_refs(src);
        assert!(out.contains("b"), "input did not parse into references");
        t.elapsed().as_micros().max(1)
    };

    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let d = elapsed(&deep);
            let f = elapsed(&shallow);
            assert!(
                d <= f * u128::from(MAX_RATIO),
                "identifiers at depth {DEPTH} cost {d}us against {f}us shallow at {N} identifiers                  ({:.1}x, bound {MAX_RATIO}x) — a per-node parent lookup is back",
                d as f64 / f as f64
            );
        })
        .expect("spawn")
        .join()
        .expect("the walk itself must not recurse into trouble");
}
