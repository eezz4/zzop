use super::unparsed_extension_warning;
use std::collections::BTreeMap;

fn unparsed(entries: &[(&str, usize, &[&str])]) -> BTreeMap<String, (usize, Vec<String>)> {
    entries
        .iter()
        .map(|(ext, count, rels)| {
            (
                (*ext).to_string(),
                (*count, rels.iter().map(|r| (*r).to_string()).collect()),
            )
        })
        .collect()
}

/// The single on-ramp entry — always last, one per run regardless of extension count.
fn on_ramp(entries: &[(&str, usize, &[&str])]) -> String {
    unparsed_extension_warning(&unparsed(entries))
        .pop()
        .expect("an on-ramp note is always emitted")
}

#[test]
fn empty_map_warns_nothing() {
    // No gap, no note: the on-ramp must not appear on a tree that has nothing to disclose.
    assert!(unparsed_extension_warning(&BTreeMap::new()).is_empty());
}

#[test]
fn one_fact_line_per_extension_in_sorted_order_plus_one_on_ramp() {
    let warnings = unparsed_extension_warning(&unparsed(&[
        ("sql", 2, &["a.sql", "b.sql"]),
        ("py", 1, &["c.py"]),
    ]));
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    // BTreeMap key order: "py" < "sql".
    assert!(warnings[0].contains(".py"), "{warnings:?}");
    assert!(warnings[1].contains(".sql"), "{warnings:?}");
    assert!(warnings[2].starts_with("No native parser exists for 2 extension(s)"));
}

#[test]
fn per_extension_lines_carry_only_their_own_facts() {
    // The measured defect: a repo with .env.development/.env.example/.env.production/.sh printed the
    // ENTIRE adapter/overlay guidance once per extension. Each fact line must now end at its own facts.
    let warnings = unparsed_extension_warning(&unparsed(&[
        (
            "env",
            3,
            &[".env.development", ".env.example", ".env.production"],
        ),
        ("sh", 1, &["deploy.sh"]),
    ]));
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    for fact in &warnings[..2] {
        for prescription in [
            "overlays: [...]",
            "zzop.config.jsonc",
            "adapterOverlays",
            "partial overlay",
            "contract envelope-guide",
            "docs/NORMALIZED_AST.md",
            "analyze_envelope",
        ] {
            assert!(
                !fact.contains(prescription),
                "guidance `{prescription}` must be stated once per run, not per extension: {fact}"
            );
        }
    }
}

#[test]
fn a_fact_line_names_its_count_extension_and_sample() {
    let warnings = unparsed_extension_warning(&unparsed(&[("py", 1, &["c.py"])]));
    assert!(
        warnings[0].starts_with("1 file(s) with extension .py have no native parser"),
        "{}",
        warnings[0]
    );
    assert!(warnings[0].contains("c.py"), "{}", warnings[0]);
}

#[test]
fn the_on_ramp_note_names_the_config_knob_and_the_minimal_first_step() {
    let w = on_ramp(&[("py", 1, &["c.py"])]);
    assert!(w.contains("overlays: [...]"), "{w}");
    assert!(w.contains("zzop.config.jsonc"), "{w}");
    assert!(w.contains("adapterOverlays"), "{w}");
    assert!(w.contains("docs/NORMALIZED_AST.md"), "{w}");
    assert!(w.contains("partial overlay"), "{w}");
}

#[test]
fn the_on_ramp_note_chains_the_gap_to_creation_in_both_dialects() {
    // The funnel principle (output-philosophy, gap-to-creation): a gap warning must not end at disclosure —
    // it chains the user to BUILDING an adapter, guide -> validate -> example, and the default on-ramp is a
    // minimal Mode B overlay, never a full parser. Host-dialect aware: the contract docs ship inside the
    // binary (`zzop contract <name>`) for MCP-host users; repo users get the docs path. De-duplicating the
    // guidance must never cost a link in that chain — this test is what makes the de-duplication safe.
    let w = on_ramp(&[("py", 1, &["c.py"])]);
    assert!(w.contains("examples/ adapters"), "{w}");
    assert!(w.contains("zzop contract adapter-guide"), "{w}");
    assert!(w.contains("zzop contract envelope-guide"), "{w}");
    assert!(w.contains("zzop://contract/envelope-guide"), "{w}");
    assert!(w.contains("zzop contract envelope-schema"), "{w}");
    assert!(w.contains("zzop contract example-envelope"), "{w}");
    // The checker step of the funnel, in both dialects — a reader who writes an overlay must be able to
    // find out whether it is well-formed BEFORE wiring it in.
    assert!(w.contains("zzop validate-envelope"), "{w}");
    assert!(w.contains("`validate_envelope`"), "{w}");
    // Reachability honesty: a 2026-07-17 blind agent burned time hunting for a Mode A entry point
    // the binary then lacked (wording was corrected to "embedder API only"); the binary now HAS
    // one (`zzop analyze-envelope` / MCP tool `analyze_envelope`), so the wording names every
    // reachable surface — a reword that drops one of them regresses to a partial claim and fails
    // here. (The removed napi `analyzeEnvelope` binding is deliberately NOT named — naming an
    // unreachable surface is the same honesty regression in the other direction.)
    assert!(w.contains("Mode A full-envelope analysis:"), "{w}");
    assert!(w.contains("analyze-envelope"), "{w}");
    assert!(w.contains("`analyze_envelope`"), "{w}");
    assert!(
        !w.contains("Mode A/B"),
        "overlays must be correctly labeled Mode B only, got: {w}"
    );
}

#[test]
fn the_on_ramp_note_caps_the_named_extensions_and_counts_the_rest() {
    let empty: &[&str] = &[];
    let entries: Vec<(&str, usize, &[&str])> = ["a", "b", "c", "d", "e", "f", "g"]
        .into_iter()
        .map(|ext| (ext, 1usize, empty))
        .collect();
    let w = on_ramp(&entries);
    assert!(
        w.starts_with(
            "No native parser exists for 7 extension(s) in this tree (.a, .b, .c, .d, .e, +2 more)"
        ),
        "{w}"
    );
}

#[test]
fn the_on_ramp_note_omits_the_more_suffix_when_every_extension_is_named() {
    let w = on_ramp(&[("py", 1, &["c.py"]), ("sql", 1, &["a.sql"])]);
    assert!(
        w.starts_with("No native parser exists for 2 extension(s) in this tree (.py, .sql) —"),
        "{w}"
    );
}

#[test]
fn count_above_sample_len_appends_a_plus_n_more_suffix() {
    // Collection caps the sample at 3 rels even though the real count is 5.
    let warnings =
        unparsed_extension_warning(&unparsed(&[("sql", 5, &["a.sql", "b.sql", "c.sql"])]));
    assert!(
        warnings[0].contains("a.sql, b.sql, c.sql, +2 more"),
        "{}",
        warnings[0]
    );
}

#[test]
fn count_equal_to_sample_len_has_no_more_suffix() {
    let warnings = unparsed_extension_warning(&unparsed(&[("sql", 1, &["a.sql"])]));
    assert!(!warnings[0].contains("more"), "{}", warnings[0]);
}

#[test]
fn two_calls_over_the_same_map_are_byte_for_byte_identical() {
    let map = unparsed(&[("sql", 2, &["a.sql", "b.sql"]), ("py", 1, &["c.py"])]);
    assert_eq!(
        unparsed_extension_warning(&map),
        unparsed_extension_warning(&map)
    );
}
