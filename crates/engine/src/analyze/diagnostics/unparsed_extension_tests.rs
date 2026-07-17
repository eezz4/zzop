use super::unparsed_extension_warning;
use std::collections::BTreeMap;

#[test]
fn empty_map_warns_nothing() {
    assert!(unparsed_extension_warning(&BTreeMap::new()).is_empty());
}

#[test]
fn one_line_per_extension_in_sorted_order() {
    let mut unparsed = BTreeMap::new();
    unparsed.insert(
        "sql".to_string(),
        (2, vec!["a.sql".to_string(), "b.sql".to_string()]),
    );
    unparsed.insert("py".to_string(), (1, vec!["c.py".to_string()]));
    let warnings = unparsed_extension_warning(&unparsed);
    assert_eq!(warnings.len(), 2);
    // BTreeMap key order: "py" < "sql".
    assert!(warnings[0].contains(".py"), "{:?}", warnings);
    assert!(warnings[1].contains(".sql"), "{:?}", warnings);
}

#[test]
fn message_shape_names_count_extension_sample_and_the_overlays_config_knob() {
    let mut unparsed = BTreeMap::new();
    unparsed.insert("py".to_string(), (1, vec!["c.py".to_string()]));
    let warnings = unparsed_extension_warning(&unparsed);
    assert_eq!(warnings.len(), 1);
    let w = &warnings[0];
    assert!(
        w.starts_with("1 file(s) with extension .py have no native parser"),
        "{w}"
    );
    assert!(w.contains("c.py"), "{w}");
    assert!(w.contains("overlays: [...]"), "{w}");
    assert!(w.contains("zzop.config.jsonc"), "{w}");
    assert!(w.contains("adapterOverlays"), "{w}");
    assert!(w.contains("docs/NORMALIZED_AST.md"), "{w}");
}

#[test]
fn message_chains_the_gap_to_creation_with_the_minimal_on_ramp_and_embedded_contract() {
    // The funnel principle (output-philosophy, gap-to-creation): a gap warning must not end at disclosure —
    // it chains the user to BUILDING an adapter, and the default on-ramp is a minimal Mode B
    // overlay, never a full parser. Host-dialect aware: the contract docs ship inside the
    // binary (`zzop-mcp contract <name>`) for MCP-host users; repo users get the docs path.
    let mut unparsed = BTreeMap::new();
    unparsed.insert("py".to_string(), (1, vec!["c.py".to_string()]));
    let w = &unparsed_extension_warning(&unparsed)[0];
    assert!(w.contains("partial overlay"), "{w}");
    // Dual-audience pointers: every repo path carries an embedded-contract twin (a blind binary-only
    // MCP user has no examples/ or docs/ checkout), including the MCP resource URI form for hosts
    // without CLI access.
    assert!(w.contains("examples/ adapters"), "{w}");
    assert!(w.contains("zzop-mcp contract adapter-guide"), "{w}");
    assert!(w.contains("zzop-mcp contract envelope-guide"), "{w}");
    assert!(w.contains("zzop://contract/envelope-guide"), "{w}");
    assert!(w.contains("zzop-mcp contract envelope-schema"), "{w}");
    // Reachability honesty, both directions: a 2026-07-17 blind agent burned time hunting for a
    // Mode A entry point the binary then lacked (wording was corrected to "embedder API only");
    // the binary now HAS one (`zzop-mcp analyze-envelope` / MCP tool `analyze_envelope`), so the
    // wording names every reachable surface — a reword that drops one of them regresses to a
    // partial claim and fails here.
    assert!(w.contains("Mode A full-envelope analysis:"), "{w}");
    assert!(w.contains("napi `analyzeEnvelope`"), "{w}");
    assert!(w.contains("analyze-envelope"), "{w}");
    assert!(w.contains("`analyze_envelope`"), "{w}");
    assert!(
        !w.contains("Mode A/B"),
        "overlays must be correctly labeled Mode B only, got: {w}"
    );
}

#[test]
fn count_above_sample_len_appends_a_plus_n_more_suffix() {
    let mut unparsed = BTreeMap::new();
    // Collection caps the sample at 3 rels even though the real count is 5.
    unparsed.insert(
        "sql".to_string(),
        (
            5,
            vec![
                "a.sql".to_string(),
                "b.sql".to_string(),
                "c.sql".to_string(),
            ],
        ),
    );
    let warnings = unparsed_extension_warning(&unparsed);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].contains("a.sql, b.sql, c.sql, +2 more"),
        "{}",
        warnings[0]
    );
}

#[test]
fn count_equal_to_sample_len_has_no_more_suffix() {
    let mut unparsed = BTreeMap::new();
    unparsed.insert("sql".to_string(), (1, vec!["a.sql".to_string()]));
    let warnings = unparsed_extension_warning(&unparsed);
    assert!(!warnings[0].contains("more"), "{}", warnings[0]);
}

#[test]
fn two_calls_over_the_same_map_are_byte_for_byte_identical() {
    let mut unparsed = BTreeMap::new();
    unparsed.insert(
        "sql".to_string(),
        (2, vec!["a.sql".to_string(), "b.sql".to_string()]),
    );
    unparsed.insert("py".to_string(), (1, vec!["c.py".to_string()]));
    assert_eq!(
        unparsed_extension_warning(&unparsed),
        unparsed_extension_warning(&unparsed)
    );
}
