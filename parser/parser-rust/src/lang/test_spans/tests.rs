//! `extract_test_spans` unit tests. Every assertion below is a LINE-RANGE claim, so each fixture is
//! written with its line numbers counted from 1 in a comment — an off-by-one here would silence a rule on
//! a shipped line, which is the failure this whole channel exists to avoid.

use super::extract_test_spans;

/// `(start, end)` pairs a line falls inside — the question every consumer actually asks.
fn covers(spans: &[(u32, u32)], line: u32) -> bool {
    spans.iter().any(|&(s, e)| s <= line && line <= e)
}

#[test]
fn inline_cfg_test_mod_is_one_span_covering_its_whole_body() {
    let src = "\
fn shipped() {                       // 1
    let q = \"SELECT * FROM users\"; // 2
}                                    // 3
                                     // 4
#[cfg(test)]                         // 5
mod tests {                          // 6
    #[test]                          // 7
    fn t() {                         // 8
        let q = \"SELECT * FROM u\"; // 9
    }                                // 10
}                                    // 11
";
    let spans = extract_test_spans("src/lib.rs", src);
    assert_eq!(spans, vec![(5, 11)], "one span for the whole gated module");
    assert!(!covers(&spans, 2), "the shipped line must stay judged");
    assert!(covers(&spans, 9), "the fixture line must be covered");
}

#[test]
fn bare_test_attribute_on_a_free_function_is_covered() {
    // The shape of `crates/core/src/dsl/tests_line_scan.rs`: a file that is a test module only because
    // its PARENT declared it so, whose functions still each carry `#[test]`. The parent's declaration is
    // invisible here; the attribute is not.
    let src = "\
use super::*;      // 1
                   // 2
#[test]            // 3
fn t() {           // 4
    assert!(true); // 5
}                  // 6
";
    let spans = extract_test_spans("src/dsl/tests_line_scan.rs", src);
    assert_eq!(spans, vec![(3, 6)]);
}

#[test]
fn runner_attributes_are_covered_without_enumerating_runners() {
    for attr in ["#[tokio::test]", "#[sqlx::test]", "#[actix_web::test]"] {
        let src = format!("{attr}\nasync fn t() {{\n    let _ = 1;\n}}\n");
        let spans = extract_test_spans("src/lib.rs", &src);
        assert_eq!(
            spans,
            vec![(1, 4)],
            "{attr} must be recognized as test-only"
        );
    }
}

#[test]
fn cfg_not_test_is_shipping_code_and_yields_no_span() {
    // The one case whose obvious reading is backwards: this code is compiled OUT of the test build and
    // INTO the release binary, so covering it would delete a real judgment.
    let src = "\
#[cfg(not(test))]                     // 1
fn ship() {                           // 2
    let q = \"SELECT * FROM users\";  // 3
}                                     // 4
";
    assert!(extract_test_spans("src/lib.rs", src).is_empty());
}

#[test]
fn cfg_all_test_and_feature_is_still_test_only() {
    let src = "#[cfg(all(test, feature = \"x\"))]\nmod t {\n    fn f() {}\n}\n";
    assert_eq!(extract_test_spans("src/lib.rs", src), vec![(1, 4)]);
}

// --- the cfg predicate is read as STRUCTURE, not as a flat ident search ------------------------------
// Each case below was answered WRONG by the earlier "contains `test` and does not contain `not`" test:
// the `all(test, not(...))` pair read as shipping (so `adapters::raw_sql` minted a fixture's SQL as a
// deployed table), and the `any(test, feature)` case read as test-only (so a shipped helper's findings
// were deleted whenever a user enabled the feature). Both directions are pinned, in both polarities.

#[test]
fn cfg_all_test_and_a_negated_sibling_is_still_test_only() {
    // `all(...)` is a conjunction: ONE test-implying conjunct is enough, and the `not(...)` beside it
    // negates something that is not `test`, so it says nothing about the test build either way.
    for pred in [
        "all(test, not(miri))",
        "all(test, not(target_os = \"windows\"))",
        "all(not(miri), test)",
        "all(test, not(any(miri, feature = \"x\")))",
    ] {
        let src = format!("#[cfg({pred})]\nmod t {{\n    fn f() {{}}\n}}\n");
        assert_eq!(
            extract_test_spans("src/lib.rs", &src),
            vec![(1, 4)],
            "cfg({pred}) compiles only in a test build"
        );
    }
}

#[test]
fn cfg_any_test_or_a_feature_is_shipped_code_and_yields_no_span() {
    // The over-suppression direction, and the reason `any(...)` cannot reuse `all(...)`'s rule: turning
    // `testkit` on ships this module, so a span here would silently delete every finding inside it.
    let src = "#[cfg(any(test, feature = \"testkit\"))]\npub mod helpers {\n    pub fn h() {}\n}\n";
    assert!(extract_test_spans("src/lib.rs", src).is_empty());
}

#[test]
fn cfg_any_whose_every_branch_implies_test_is_test_only() {
    // The other half of the `any(...)` rule — a disjunction IS test-only when no branch escapes `test`.
    let src = "#[cfg(any(test, all(test, feature = \"x\")))]\nmod t {\n    fn f() {}\n}\n";
    assert_eq!(extract_test_spans("src/lib.rs", src), vec![(1, 4)]);
}

#[test]
fn cfg_all_with_a_negated_test_is_shipping_code() {
    // `not(test)` implies nothing about `test` being on, so no conjunct implies test here.
    let src = "#[cfg(all(not(test), feature = \"x\"))]\nfn ship() {}\n";
    assert!(extract_test_spans("src/lib.rs", src).is_empty());
}

#[test]
fn an_unmodelled_cfg_predicate_reads_as_shipping_rather_than_as_a_guess() {
    // Unknown means shipping — silence is the safe answer for the fact channels and merely noisy for
    // the rule packs, whereas guessing "test" would delete real judgments.
    for pred in ["feature = \"test\"", "miri", "target_os = \"linux\"", "()"] {
        let src = format!("#[cfg({pred})]\nfn ship() {{}}\n");
        assert!(
            extract_test_spans("src/lib.rs", &src).is_empty(),
            "cfg({pred}) is not a proof of test-only"
        );
    }
}

#[test]
fn inner_cfg_test_gates_the_whole_file() {
    let src = "#![cfg(test)]\n\nfn helper() {\n    let _ = 1;\n}\n";
    let spans = extract_test_spans("src/lib.rs", src);
    assert_eq!(
        spans,
        vec![(1, 6)],
        "one span from line 1 to the file's line count"
    );
    assert!(covers(&spans, 4));
}

#[test]
fn a_test_fn_inside_an_impl_block_is_covered() {
    let src = "\
struct S;               // 1
impl S {                // 2
    fn ship(&self) {}   // 3
    #[test]             // 4
    fn t() {            // 5
        let _ = 1;      // 6
    }                   // 7
}                       // 8
";
    let spans = extract_test_spans("src/lib.rs", src);
    assert_eq!(spans, vec![(4, 7)]);
    assert!(!covers(&spans, 3), "the shipped method stays judged");
}

#[test]
fn a_gated_module_yields_one_span_not_one_per_nested_item() {
    let src = "\
#[cfg(test)]        // 1
mod tests {         // 2
    #[test]         // 3
    fn a() {}       // 4
    #[test]         // 5
    fn b() {}       // 6
    fn helper() {}  // 7
}                   // 8
";
    assert_eq!(
        extract_test_spans("src/lib.rs", src),
        vec![(1, 8)],
        "the visitor must stop descending once an enclosing item is gated"
    );
}

#[test]
fn a_non_test_helper_in_a_parent_declared_test_file_is_not_covered() {
    // The documented BOUNDARY, pinned so a later reader does not mistake silence for coverage: nothing in
    // this text says the file is a test file, so the path axis stays the owner of that claim.
    let src = "fn helper() -> &'static str {\n    \"SELECT * FROM users\"\n}\n";
    assert!(extract_test_spans("src/dsl/tests_line_scan.rs", src).is_empty());
}

#[test]
fn shipped_code_after_a_gated_module_is_not_swallowed_by_it() {
    // The over-suppression direction. A span that ran to end-of-file instead of end-of-ITEM would look
    // identical on every fixture where the test module is last — which is nearly all of them — and would
    // silently delete real judgments in the minority where it is not.
    let src = "\
#[cfg(test)]                          // 1
mod tests {                           // 2
    fn t() {}                         // 3
}                                     // 4
                                      // 5
pub fn ship() -> &'static str {       // 6
    \"SELECT * FROM users\"           // 7
}                                     // 8
";
    let spans = extract_test_spans("src/lib.rs", src);
    assert_eq!(spans, vec![(1, 4)]);
    assert!(
        !covers(&spans, 7),
        "the shipped literal below the module stays judged"
    );
}

#[test]
fn an_unparseable_file_yields_no_span_so_nothing_is_silenced() {
    assert!(extract_test_spans("src/lib.rs", "fn f(:\n").is_empty());
}

#[test]
fn a_file_with_no_tests_at_all_yields_nothing() {
    assert!(extract_test_spans("src/lib.rs", "fn f() { let _ = 1; }\n").is_empty());
}
