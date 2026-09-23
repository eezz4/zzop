use super::*;

#[test]
fn a_field_access_chain_contributes_object_and_field_but_not_the_middle() {
    let refs = parse_local_identifier_refs("class C { void f() { System.out.println(\"hi\"); } }");
    assert!(refs.contains("System"));
    assert!(refs.contains("out"));
    assert!(refs.contains("println"));
}

#[test]
fn a_scoped_type_identifier_contributes_only_its_rightmost_segment() {
    let refs = parse_local_identifier_refs("class C { java.util.List<String> xs; }");
    assert!(refs.contains("List"));
    assert!(!refs.contains("java"));
    assert!(!refs.contains("util"));
}

#[test]
fn a_type_identifier_reference_is_collected() {
    let refs = parse_local_identifier_refs("class C { String s; }");
    assert!(refs.contains("String"));
}

#[test]
fn declared_names_are_excluded() {
    let refs = parse_local_identifier_refs("class C { void m(int p) { int local = p; } }");
    // "C" (class name), "m" (method name), "p" (parameter name), "local" (variable name) are all
    // declarations, never reads.
    assert!(!refs.contains("C"));
    assert!(!refs.contains("m"));
    assert!(!refs.contains("local"));
    // `p` IS a read on the RHS of `local = p` even though it's ALSO a parameter declaration elsewhere —
    // the exclusion only applies to the declaring occurrence itself, not every later mention.
    assert!(refs.contains("p"));
}

#[test]
fn a_method_call_name_is_collected_via_the_general_identifier_rule() {
    let refs = parse_local_identifier_refs("class C { void f() { helper(); } }");
    assert!(refs.contains("helper"));
}

#[test]
fn an_enum_constant_name_is_not_a_used_name() {
    let refs = parse_local_identifier_refs("enum E { RED, GREEN, BLUE }");
    assert!(!refs.contains("RED"));
}

#[test]
fn parse_failure_yields_an_empty_set() {
    assert!(parse_local_identifier_refs("\u{0}\u{1}not java{{{{").is_empty());
}

#[test]
fn empty_file_yields_an_empty_set() {
    assert!(parse_local_identifier_refs("").is_empty());
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
/// 📏 Before the fix, one file with 20,000 identifiers at depth 4,000 took 35.8 s through `zzop analyze`
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
        "class A {{ void f(boolean b) {{ var x = {}g({}); }} }}",
        "!".repeat(DEPTH),
        args
    );
    let shallow = format!("class A {{ void f(boolean b) {{ var x = g({}); }} }}", args);

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
