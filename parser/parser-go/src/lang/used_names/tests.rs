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
/// # What this arm covers, and what it does not — stated because its own message used to overclaim
/// Its fixture is `x := g(b,b,…)`, so all N identifiers are children of an `argument_list`. That
/// exercises `is_declared_var_name`'s `DECLARES_NAME_FIELD` arm, which runs for EVERY identifier
/// regardless of parent kind — and 📏 the drill confirms this arm does its job: planting the V131
/// regression (`visit` calling `node.parent()` per identifier) takes it from passing to **48.8x**.
///
/// What it cannot see: the `short_var_declaration` path, because `is_short_var_decl_left` returns
/// on its first line for an `argument_list` parent. Exactly ONE identifier here (the `x`) has an
/// `expression_list` parent. The sibling arm below carries that population — 8,000 of them.
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
                "identifiers at depth {DEPTH} cost {d}us against {f}us shallow at {N} identifiers \
                 ({:.1}x, bound {MAX_RATIO}x) — the NAME-FIELD declaration path is paying for depth \
                 again (`is_declared_var_name`'s `DECLARES_NAME_FIELD` arm). This fixture does NOT \
                 cover the grandparent path; its sibling \
                 `identifiers_under_an_expression_list_do_not_get_slower_with_nesting_depth` does",
                d as f64 / f as f64
            );
        })
        .expect("spawn")
        .join()
        .expect("the walk itself must not recurse into trouble");
}

/// How many `identifier` nodes in `src` have an `expression_list` as their DIRECT parent — the
/// population that actually reaches [`super::is_short_var_decl_left`]'s grandparent check.
///
/// This exists because the sibling test above cannot reach that call AT ALL, and nothing said so.
/// Its fixture is `g(b,b,…)`, so every one of its N identifiers is a child of an `argument_list`;
/// `is_short_var_decl_left` returns on its first line (`parent.kind() != "expression_list"`) for
/// every single one. Exactly ONE identifier in that whole fixture — the `x` of `x := …` — has an
/// `expression_list` parent. So the assert's own message — it read *"a per-node parent lookup is
/// back"* until 2026-10-08 — named a mechanism its fixture is structurally blind to (external
/// review round 8).
///
/// Conflating the two populations is how one fixture came to carry a promise about both. Each arm
/// now names the path it actually covers, in its own failure message.
fn expression_list_identifiers(src: &str) -> usize {
    let Some(tree) = crate::parse_tree(src) else {
        return 0;
    };
    let mut n = 0usize;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "expression_list" {
                    n += 1;
                }
            }
        }
        let mut c = node.walk();
        stack.extend(node.children(&mut c));
    }
    n
}

/// The arm the test above cannot be: identifiers whose parent IS an `expression_list`, so the
/// grandparent check runs, nested deep so that a grandparent LOOKUP would be expensive.
///
/// `_ = b` puts BOTH operands under an `expression_list` (Go's assignment binds `left` and `right`
/// to one each), which is why the triggering count below is 2x the statement count and not 1x.
#[test]
fn identifiers_under_an_expression_list_do_not_get_slower_with_nesting_depth() {
    use std::time::Instant;

    const N: usize = 4_000;
    const DEPTH: usize = 400;
    // 📏 The sibling's bound, and now for the sibling's reason: a wide margin on both sides.
    // Drill on Windows (2026-10-14, i7-10750H, three runs each, machine quiet) at this DEPTH and N:
    //
    //     clean tree            1.05x · 0.83x · 1.08x
    //     V131 regression       38.35x · 38.03x · 37.82x   (`visit` asking `node.parent()`)
    //
    // This arm used to need a bound of its own (18 on a Mac, 28 on Windows) because the grandparent
    // was LOOKED UP — an O(depth) residual that put the clean ratio at 14x on one machine and 20x on
    // another, too close to the regressed one for any single number. The residual is gone: the walk
    // threads the grandparent down like the parent. If this arm ever needs more than 5 again, a
    // lookup has come back — find it rather than raising the number.
    const MAX_RATIO: u32 = 5;

    let stmts = std::iter::repeat_n("_ = b", N)
        .collect::<Vec<_>>()
        .join("; ");
    let deep = format!(
        "package p\nfunc f(b bool) {{ {} {stmts} {} }}",
        "{ ".repeat(DEPTH),
        " }".repeat(DEPTH)
    );
    let shallow = format!("package p\nfunc f(b bool) {{ {stmts} }}");

    // POPULATION FLOOR, before any timing. A ratio over two arms that reach the code path zero
    // times is a number about nothing, and it is indistinguishable from a clean one: both arms
    // return fast and the assert passes. This is the exact hole the sibling test has.
    for (label, src) in [("deep", &deep), ("shallow", &shallow)] {
        let got = expression_list_identifiers(src);
        assert_eq!(
            got,
            N * 2,
            "REFUSING TO JUDGE: the {label} arm reaches is_short_var_decl_left's grandparent lookup \
             {got} time(s), not {} — the fixture no longer exercises the path this test is named \
             after, so its timing would vouch for code nothing ran",
            N * 2
        );
    }

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
                "{} identifiers under an expression_list at depth {DEPTH} cost {d}us against {f}us \
                 shallow ({:.2}x, bound {MAX_RATIO}x) — something on this path is looking a parent or                  grandparent up instead of taking the one the walk threads down",
                N * 2,
                d as f64 / f as f64
            );
        })
        .expect("spawn")
        .join()
        .expect("the walk itself must not recurse into trouble");
}
