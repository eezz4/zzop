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
fn one_fact_line_per_extension_plus_one_on_ramp_last() {
    let warnings = unparsed_extension_warning(&unparsed(&[
        ("sql", 2, &["a.sql", "b.sql"]),
        ("py", 1, &["c.py"]),
    ]));
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    // Unread-count descending: 2 `.sql` files ahead of 1 `.py`. The ORDER itself is
    // `fact_lines_are_ordered_by_unread_file_count_not_by_extension_name`'s subject; what this test
    // holds is the SHAPE — one line per extension, and the on-ramp note exactly once, last.
    assert!(warnings[0].contains(".sql"), "{warnings:?}");
    assert!(warnings[1].contains(".py"), "{warnings:?}");
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

/// The count in that line is a count of ADAPTER CANDIDATES, and a reader who takes it for "what this
/// run could not read" is off by the whole data/config group. Measured on macrozheng/mall: the line
/// said 8 extensions (.conf/.emmx/.mf/.pdb/.pdm/.pos/.rp/.sh — 21 files of mind-maps and binaries)
/// while 114 unread `.xml` MyBatis mappers holding 906 SQL statements were not in it, because they are
/// not something you write a language frontend for. Naming them here instead would put a `.md`/`.json`
/// row in almost every reply, so the exclusion stays and the line STATES it, naming the one reply
/// shape that does carry the excluded population (see the scoping note below for the ones that do not).
///
/// On regression (the sentence dropped as verbosity) the count silently becomes a coverage claim it
/// was never computed to support — which is precisely how two auditors read it.
///
/// The forwarding address is `coverageGaps` and ONLY `coverageGaps`, deliberately: this message is
/// built in a shared crate and the identical sentence reaches an MCP client, where `zzop coverage` is
/// a command that cannot be run and `unreadExtensions` sits on a reply with no MCP twin at all. A
/// pointer the reader cannot follow is worse than none, and `host_vocabulary`'s CLI-only-vocabulary
/// contract fails this file if the subcommand is ever named here again.
///
/// And that same rule applies to `coverageGaps`, which is why the pointer is SCOPED. The field is the
/// analyze shaper's invention; `zzop_summary::cross` builds its per-source entries from `warnings` +
/// `coverage` and never adds it, and the raw `zzop-facade` output has no shaper at all — while THIS
/// warning is emitted by the engine on every lane that walks a tree, cross and raw included. An
/// unqualified "rides the `coverageGaps` field of this same reply" was therefore false for two of the
/// lanes that receive it: the reader searched their own reply for a key that is not in it. The leg
/// below holds the qualification, not just the name, because the name alone is what regressed.
#[test]
fn the_on_ramp_note_says_what_its_count_excludes_and_where_that_population_is() {
    let w = on_ramp(&[("py", 1, &["c.py"])]);
    assert!(
        w.contains("NOT THE TREE'S UNREAD FILETYPES"),
        "the count must disown the reading it invites: {w}"
    );
    assert!(
        w.contains("coverageGaps"),
        "an exclusion with no forwarding address leaves the reader with nowhere to go: {w}"
    );
    assert!(
        w.contains("SHAPED analyze reply") && w.contains("carry no `coverageGaps` at all"),
        "this string reaches the cross join and the raw facade output too, and NEITHER carries \
         `coverageGaps` — an unscoped pointer sends those readers hunting for a key their reply does \
         not have: {w}"
    );
    // The pointer used to name SEVEN excluded groups and forward all of them to `coverageGaps`, which
    // carries only the source and data/config halves and only above two share floors. A prose-and-
    // images tree followed that pointer to an empty array — the same "pointer the reader cannot
    // follow" this test exists to prevent, one level in. This leg holds the partiality.
    assert!(
        w.contains("PART of that excluded") && w.contains("named by NEITHER channel"),
        "the forwarding address carries only part of the population the same sentence enumerates, \
         so a reader whose tree is all prose and images must not be sent to an empty array: {w}"
    );
    assert!(
        !w.contains("zzop coverage"),
        "a CLI subcommand is not a route an MCP reader of this same sentence can take: {w}"
    );
}

/// The anecdote behind the exclusion belongs in the source, not on every reply. It was 170 bytes of
/// one foreign repository's statistics — "measured on one 723-file tree… a 114-file .xml majority" —
/// re-serialized on every run that reaches this path, while `dispatch::NonSourceKind`'s doc and this
/// file's own already owned the measurement for anyone who can act on it.
///
/// Deleting evidence is the wrong move when the evidence is the CLAIM, so this pins the split: the
/// disclosure the reader needs (this count excludes the non-source filetypes, however large) stays,
/// the numbers that only justify it to a maintainer go. Without this leg the next reviewer reading
/// "state your evidence" restores the anecdote, and the wire pays for it again.
#[test]
fn the_on_ramp_note_states_the_exclusion_without_quoting_another_repositorys_measurements() {
    let w = on_ramp(&[("py", 1, &["c.py"])]);
    for anecdote in ["723-file", "114-file", "measured on one"] {
        assert!(
            !w.contains(anecdote),
            "{anecdote:?} is a maintainer's evidence, not the reader's disclosure — its owners are \
             `dispatch::NonSourceKind`'s doc and this test file: {w}"
        );
    }
    assert!(
        w.contains("classed non-source") && w.contains("EXCLUDED"),
        "what the anecdote was evidence FOR must still be on the wire: {w}"
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

/// The ORDER of these lines is derived from the one property every entry already carries — how many
/// files this run could not read — and not from the extension's name. The name is an accident of
/// spelling, and sorting on it puts the largest gap wherever its first letter happens to fall.
///
/// Measured before this landed: nocodb's `.vue` line (962 unread files) was the 42nd of 49 `warnings`
/// entries, below eight extensions of 1-10 files each, because "v" sorts last; koel's `.php` (1412
/// files) sat 10th, one line under a single `.psd`. The channel exists so a reader learns what this run
/// could not see, and the entry that answers that best was the one furthest from the first screen.
#[test]
fn fact_lines_are_ordered_by_unread_file_count_not_by_extension_name() {
    let warnings = unparsed_extension_warning(&unparsed(&[
        ("bash", 1, &["a.bash"]),
        ("vue", 962, &["a.vue"]),
        ("sh", 7, &["a.sh"]),
    ]));
    assert!(warnings[0].contains(".vue"), "{warnings:?}");
    assert!(warnings[1].contains(".sh"), "{warnings:?}");
    assert!(warnings[2].contains(".bash"), "{warnings:?}");
}

/// The tie-break, and the reason this order is TOTAL rather than merely deterministic by accident:
/// equal counts fall back to the extension name, so no pair of entries is ever left to the order the
/// caller happened to insert them in.
#[test]
fn equal_counts_break_the_tie_on_the_extension_name() {
    // The two tied extensions are chosen so that the name axis alone would put them on OPPOSITE sides
    // of the larger one: alphabetically `aa` leads and `zz` trails, so a run that still sorted by name
    // would report `aa, mm, zz` and this test would see it.
    let warnings = unparsed_extension_warning(&unparsed(&[
        ("zz", 4, &["b.zz"]),
        ("aa", 4, &["a.aa"]),
        ("mm", 9, &["a.mm"]),
    ]));
    assert!(warnings[0].contains(".mm"), "{warnings:?}");
    assert!(warnings[1].contains(".aa"), "{warnings:?}");
    assert!(warnings[2].contains(".zz"), "{warnings:?}");
}

/// The summary line names a SAMPLE of the extensions, and it must sample the order the fact lines above
/// it are actually in. Naming the five alphabetically-first while the entries lead with the largest
/// would make the one line a skimming reader does read point away from the gap the ordering just
/// surfaced — the same failure, moved one line down.
#[test]
fn the_on_ramp_note_samples_the_same_order_the_fact_lines_are_in() {
    let entries: &[(&str, usize, &[&str])] = &[
        ("bash", 1, &["a.bash"]),
        ("bats", 4, &["a.bats"]),
        ("db", 2, &["a.db"]),
        ("env", 4, &["a.env"]),
        ("eta", 10, &["a.eta"]),
        ("sh", 7, &["a.sh"]),
        ("vue", 962, &["a.vue"]),
    ];
    let w = on_ramp(entries);
    assert!(
        w.starts_with(
            "No native parser exists for 7 extension(s) in this tree (.vue, .eta, .sh, .bats, .env, \
             +2 more)"
        ),
        "{w}"
    );
}
