use super::*;

fn refs(text: &str) -> BTreeSet<String> {
    parse_local_identifier_refs(text)
}

#[test]
fn method_body_reads_are_collected() {
    let src = "class C { void M() { var x = Helper.Compute(1); } }";
    let out = refs(src);
    assert!(out.contains("Compute"));
    assert!(out.contains("Helper"));
    // Declaring names excluded.
    assert!(!out.contains("C"));
    assert!(!out.contains("M"));
}

#[test]
fn qualified_name_type_reference_contributes_only_rightmost_segment() {
    // A dotted TYPE reference is a real `qualified_name` node (a distinct grammar shape from a
    // runtime member-access chain, module doc) — only its rightmost segment is collected.
    let src = "class C { void M() { System.Text.StringBuilder x = null; } }";
    let out = refs(src);
    assert!(out.contains("StringBuilder"));
    assert!(!out.contains("System"));
    assert!(!out.contains("Text"));
}

#[test]
fn member_access_expression_chain_collects_every_segment() {
    // Unlike a `qualified_name`, a runtime member-access chain (`System.Console.WriteLine(...)`) is
    // NESTED `member_access_expression` all the way down — no special-casing needed (module doc's
    // "free ride" note), so every segment is a genuine reference, mirroring
    // `zzop_parser_java_21::lang::used_names`'s identical behavior for a chained Java `field_access`.
    let src = "class C { void M() { System.Console.WriteLine(1); } }";
    let out = refs(src);
    assert!(out.contains("System"));
    assert!(out.contains("Console"));
    assert!(out.contains("WriteLine"));
}

#[test]
fn variable_declarator_name_excluded_but_type_reference_included() {
    let src = "class C { void M() { Foo x = null; } }";
    let out = refs(src);
    assert!(out.contains("Foo"));
    assert!(!out.contains("x"));
}

#[test]
fn using_alias_name_excluded() {
    let src = "using Sys = System.Text;\nclass C {}\n";
    let out = refs(src);
    assert!(!out.contains("Sys"));
    assert!(out.contains("Text"));
}

#[test]
fn parameter_name_excluded_but_parameter_type_included() {
    let src = "class C { void M(Foo bar) {} }";
    let out = refs(src);
    assert!(out.contains("Foo"));
    assert!(!out.contains("bar"));
}

#[test]
fn empty_on_parse_failure() {
    assert!(refs("\u{0}\u{1}not csharp{{{{").is_empty());
}

/// 📏 The DEEP shape must not cost dramatically more than the FLAT shape at the same identifier
/// count — review ledger V116, and the reason [`walk`] threads `parent_kind` down instead of
/// calling `node.parent()`.
///
/// This asserts a RATIO, not a wall-clock budget, and that is deliberate: both halves are measured
/// seconds apart in one process, so machine load, CPU scaling and a debug-vs-release build all
/// divide out. The ratio is not a proxy for the property — it IS the property. A per-node
/// `node.parent()` is O(depth), which is invisible in a flat file and quadratic in a nested one, so
/// only a shape COMPARISON can see it; an absolute bound would either be flaky or so loose it would
/// pass the quadratic code.
///
/// Measured at n = 4,000 (release): deep 62 ms against flat 104 ms — the deep file is FASTER, and
/// 2.8× smaller. With `node.parent()` restored the same two inputs cost 9,040 ms against 111 ms.
/// So the bound below sits ~8× above what correct code does and ~16× below what the defect does.
///
/// The big explicit stack is about a DIFFERENT open question, not about this bound: [`walk`]
/// recurses once per CST level, and a 1 MB thread overflows at ~2,000 levels. That is filed
/// separately — here it just keeps this test measuring the thing it claims to measure.
#[test]
fn nested_conditionals_cost_no_more_than_flat_ones_at_the_same_identifier_count() {
    use std::time::Instant;

    const N: usize = 4_000;
    const MAX_RATIO: u32 = 5;

    let deep = format!(
        "class C {{ void M(bool b) {{ var x = {}1; }} }}",
        "b ? 1 : ".repeat(N)
    );
    let flat = {
        let body: String = (0..N).map(|i| format!("var v{i} = b ? 1 : 0;\n")).collect();
        format!("class C {{ void M(bool b) {{\n{body}}} }}")
    };

    let elapsed = |src: &str| {
        let t = Instant::now();
        let out = parse_local_identifier_refs(src);
        // Non-vacuity: a parse failure would return an empty set instantly and pass any bound.
        assert!(out.contains("b"), "input did not parse into references");
        t.elapsed().as_micros().max(1)
    };

    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let d = elapsed(&deep);
            let f = elapsed(&flat);
            assert!(
                d <= f * u128::from(MAX_RATIO),
                "nested conditionals cost {d}us against {f}us flat at {N} identifiers \
                 ({:.1}x, bound {MAX_RATIO}x) — a per-node parent lookup is back",
                d as f64 / f as f64
            );
        })
        .expect("spawn")
        .join()
        .expect("deep-vs-flat probe panicked");
}
