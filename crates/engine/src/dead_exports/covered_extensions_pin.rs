//! T2 policy-value pin (rule-quality.md §6 substitute for a T1 shared symbol): `rules-http`'s
//! `mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS` is a hand-maintained duplicate of
//! [`is_ts_source_ext`]'s accepted extension set, plus the NON-TS languages whose parsers now feed the
//! shared call graph — `"java"`, Python's `"py"`/`"pyi"`, and `"rs"` (that crate depends on `zzop_core` only, so
//! it cannot call this private fn directly — its own doc says as much). Lives here, not in
//! `crates/engine/tests/`, because [`is_ts_source_ext`] is `pub(crate)`: an external integration-test
//! crate cannot see it, only a unit test inside this same module can. If this fails, either
//! `is_ts_source_ext` grew/shrank an extension and the rule's list needs the same edit, or the rule's
//! list drifted on its own — either way, re-justify both sides together.
use super::is_ts_source_ext;

/// The deliberate, documented additions beyond [`is_ts_source_ext`] — kept as one list so a future
/// lift edits ONE place here and one place in the rule crate, and the two pins below both read it.
/// Order matters: it is the order they appear in the rule's own constant.
const NON_TS_CALL_GRAPH_EXTENSIONS: &[&str] = &["java", "py", "pyi", "rs"];

#[test]
fn call_graph_covered_extensions_equals_is_ts_source_ext_plus_non_ts_lifts() {
    let rule_list = zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS;
    for ext in NON_TS_CALL_GRAPH_EXTENSIONS {
        assert!(
            rule_list.contains(ext),
            "the rule's list must still carry its deliberate addition {ext:?} beyond \
             is_ts_source_ext, got: {rule_list:?}"
        );
    }
    for ext in rule_list {
        if NON_TS_CALL_GRAPH_EXTENSIONS.contains(ext) {
            continue; // deliberate, documented additions beyond is_ts_source_ext
        }
        assert!(
            is_ts_source_ext(&format!("x.{ext}")),
            "rule's CALL_GRAPH_COVERED_EXTENSIONS lists {ext:?}, but is_ts_source_ext does not \
             accept it — the two hand-kept duplicates have drifted apart"
        );
    }
    // The reverse direction: every extension is_ts_source_ext accepts (enumerated from its own
    // match arm — see that fn's source) must also be in the rule's list.
    for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"] {
        assert!(
            rule_list.contains(&ext),
            "is_ts_source_ext accepts {ext:?}, but rule's CALL_GRAPH_COVERED_EXTENSIONS does \
             not list it — the two hand-kept duplicates have drifted apart: {rule_list:?}"
        );
    }
}

/// The SECOND hand-kept duplicate of the same set, and the narrower one:
/// `http_scan::WRITE_SITE_COVERED_EXTENSIONS` names the languages whose parser actually fills
/// `SourceSymbol::write_sites`, which is exactly [`is_ts_source_ext`] with NONE of the
/// [`NON_TS_CALL_GRAPH_EXTENSIONS`] — Java and Python feed the shared call graph but no write sites.
/// `unsafe-read-endpoint`/`non-idempotent-write` publish that list in their finding messages, so a
/// drift here would publish a false sightline (claiming a language is covered when its write-site
/// list is structurally empty) — the exact silent-failure the sightline exists to close.
#[test]
fn write_site_covered_extensions_equals_is_ts_source_ext_with_no_call_graph_lifts() {
    let write_list = zzop_rules_http::http_scan::WRITE_SITE_COVERED_EXTENSIONS;
    for ext in NON_TS_CALL_GRAPH_EXTENSIONS {
        assert!(
            !write_list.contains(ext),
            "write_sites is produced by the TypeScript parser alone — listing {ext:?} here would \
             publish a sightline claiming that language's routes are checked when its parser stubs \
             write_sites: {write_list:?}"
        );
    }
    for ext in write_list {
        assert!(
            is_ts_source_ext(&format!("x.{ext}")),
            "WRITE_SITE_COVERED_EXTENSIONS lists {ext:?}, but is_ts_source_ext does not accept \
             it — no TypeScript-dispatched file carries that extension, so the published \
             sightline names a language the write-site producer never sees"
        );
    }
    for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"] {
        assert!(
            write_list.contains(&ext),
            "is_ts_source_ext accepts {ext:?} (so parser-typescript DOES compute write_sites for \
             it), but WRITE_SITE_COVERED_EXTENSIONS omits it — the published sightline \
             UNDER-claims and would tell a user their own files are dark: {write_list:?}"
        );
    }
    // The two rule-side lists differ by exactly the call-graph lifts — the difference the sightline
    // prose calls out ("the sibling rule covers Java/Python, this one does not").
    let call_graph = zzop_rules_http::mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS;
    let only_in_call_graph: Vec<&str> = call_graph
        .iter()
        .filter(|e| !write_list.contains(e))
        .copied()
        .collect();
    assert_eq!(
        only_in_call_graph, NON_TS_CALL_GRAPH_EXTENSIONS,
        "the call-graph-covered set must exceed the write-site-covered set by exactly the \
         documented non-TS lifts; any other difference means one of the two published sightlines \
         is now wrong"
    );
}
