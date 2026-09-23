//! Unit tests for the tool-output shaping helpers (`shape_findings`/`shape_list`/`FindingFilters`).
//! `distinct_bucket_keys`' own tests live beside it in `bucket_keys.rs`.

use super::*;

fn finding(rule: &str, severity: &str, idx: usize) -> serde_json::Value {
    serde_json::json!({ "ruleId": rule, "severity": severity, "path": format!("f{idx}.ts") })
}

/// A FILTER narrows the window too, and until 2026-09-14 it did so leaving no trace (external review
/// round 22, ledger V246).
///
/// `truncated` covers the CAP. A `severity`/`rule` filter cuts earlier, in `window_order`, and wrote
/// nothing: measured on `corpus/frameworks/nest`, `analyze --severity critical` shipped `shown: 1`
/// beside `total: 326` with no `truncated` key and no key anywhere naming a filter. Worse than silent
/// — `shownMeaning` shipped "nothing is dropped" in the same reply, and `messageByIdMeaning` asserted
/// `truncated` was the ONLY key meaning rows were left out.
///
/// All four arms are here because each one is a different claim, and three of them are the ones an
/// implementation gets wrong: the unfiltered run must pay NOTHING (absence is the honest statement),
/// the cap alone must not masquerade as a filter, and the filter alone must not masquerade as a cap.
#[test]
fn a_filter_that_removes_rows_says_so_and_an_unfiltered_run_pays_nothing() {
    let findings: Vec<_> = (0..5)
        .map(|i| {
            finding(
                if i == 0 { "r" } else { "other" },
                if i == 0 { "critical" } else { "info" },
                i,
            )
        })
        .collect();
    let no_filter = FindingFilters {
        min_severity: None,
        rule: None,
        limit: Some(1000),
    };
    let by_severity = FindingFilters {
        min_severity: Some("critical".into()),
        rule: None,
        limit: Some(1000),
    };
    let by_rule = FindingFilters {
        min_severity: None,
        rule: Some("r".into()),
        limit: Some(1000),
    };
    let capped_only = FindingFilters {
        min_severity: None,
        rule: None,
        limit: Some(2),
    };

    // 1. Nothing filtered: the key must be ABSENT, not a zero. An always-present `elided: 0` is the
    //    field a reader has to check on every reply, which is the cost this shape refuses.
    let plain = shape(&findings, &no_filter);
    assert!(
        plain.get("filtered").is_none(),
        "an unfiltered run must not carry the key at all: {plain}"
    );

    // 2. A severity filter removed four of five.
    let sev = shape(&findings, &by_severity);
    assert_eq!(sev["shown"].as_array().unwrap().len(), 1);
    assert_eq!(sev["filtered"]["elided"], 4);
    assert_eq!(sev["filtered"]["applied"]["severity"], "critical");
    assert!(
        sev["filtered"]["applied"].get("rule").is_none(),
        "a knob that was not passed must not appear as applied: {}",
        sev["filtered"]["applied"]
    );
    // The counts stay over the FULL set — that is the half the filter must NOT move.
    assert_eq!(sev["total"], 5);

    // 3. A rule filter, so the key is not keyed to one knob.
    let rule = shape(&findings, &by_rule);
    assert_eq!(rule["filtered"]["applied"]["rule"], "r");
    assert_eq!(rule["filtered"]["elided"], 4);

    // 4. The CAP alone is not a filter. Without this the new key could be written on any narrowed
    //    window and `truncated` would have a duplicate that means something else.
    let capped = shape(&findings, &capped_only);
    assert!(
        capped.get("truncated").is_some() && capped.get("filtered").is_none(),
        "the cap writes `truncated` and only that: {capped}"
    );
}

/// `shape_findings` with NO manifest-declared build surface — the state every non-npm tree is genuinely in, and therefore the right default for every pin that
/// is not itself about the build tier. The tier tests below call `shape_findings` directly with a real
/// declaration; nothing else should, or a pin written about truncation starts also asserting an ordering.
fn shape(findings: &[serde_json::Value], filters: &FindingFilters) -> serde_json::Value {
    shape_findings(findings, filters, &Default::default())
}

#[test]
fn counts_stay_full_while_filter_narrows_shown() {
    let findings = vec![
        finding("a", "info", 0),
        finding("b", "critical", 1),
        finding("a", "warning", 2),
    ];
    let filters = FindingFilters {
        min_severity: Some("warning".into()),
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["total"], 3);
    assert_eq!(shaped["bySeverity"]["info"], 1); // full-set counts, not filtered
    let shown = shaped["shown"].as_array().unwrap();
    assert_eq!(shown.len(), 2);
    // severity-desc ordering: critical before warning
    assert_eq!(shown[0]["severity"], "critical");
    assert!(shaped.get("truncated").is_none()); // complete list => no truncation key
}

#[test]
fn truncation_is_disclosed_never_silent() {
    let findings: Vec<_> = (0..5).map(|i| finding("r", "info", i)).collect();
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: Some(2),
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 2);
    assert_eq!(shaped["truncated"]["shown"], 2);
    assert_eq!(shaped["truncated"]["totalMatching"], 5);
    // `shape_findings` is the ONE surface where `severity`/`rule`/`limit` genuinely move the cap, so
    // naming `limit` here is a remedy that works — see the `shape_list` pin below for why the other
    // capped lists must not copy this wording.
    assert!(shaped["truncated"]["hint"]
        .as_str()
        .unwrap()
        .contains("limit"));
}

/// Seals the seam that made an inert remedy possible: `shape_list` used to hardcode
/// `shape_findings`' hint, so `edgesTruncated`/`degradedTruncated` told callers to "raise limit"
/// when no tool argument moves either cap. The hint is now the CALLER's, echoed verbatim — a shared
/// default cannot come back without deleting this pin.
#[test]
fn shape_list_echoes_the_callers_hint_verbatim_instead_of_a_shared_default() {
    let items: Vec<serde_json::Value> = (0..5).map(|i| serde_json::json!({ "i": i })).collect();
    let hint = "`buckets.edges` carries the full, uncapped count";
    let (shown, truncated) = shape_list(&items, 2, hint);
    assert_eq!(shown.len(), 2);
    let truncated = truncated.expect("a capped list always discloses");
    assert_eq!(truncated["shown"], 2);
    assert_eq!(truncated["totalMatching"], 5);
    assert_eq!(truncated["hint"], hint);
    // A list that fits is not "truncated" — disclosure only when something was actually dropped.
    assert!(shape_list(&items, 5, hint).1.is_none());
}

#[test]
fn deterministic_order_same_input_same_output() {
    let findings = vec![
        finding("a", "warning", 0),
        finding("b", "warning", 1),
        finding("c", "critical", 2),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let one = serde_json::to_string(&shape(&findings, &filters)).unwrap();
    let two = serde_json::to_string(&shape(&findings, &filters)).unwrap();
    assert_eq!(one, two);
    let shaped = shape(&findings, &filters);
    let shown = shaped["shown"].as_array().unwrap();
    // critical first, then the two warnings in original order (stable tiebreak).
    assert_eq!(shown[0]["ruleId"], "c");
    assert_eq!(shown[1]["ruleId"], "a");
    assert_eq!(shown[2]["ruleId"], "b");
}

#[test]
fn unknown_severity_argument_is_a_named_error() {
    let args = serde_json::json!({ "severity": "sev-nope" });
    let err = FindingFilters::from_args(Some(&args)).unwrap_err();
    assert!(err.contains("sev-nope"));
    assert!(err.contains("critical"));
}

/// Boundary-value torture round: a `severity` NUMBER used to silently fall through `as_str()` and
/// drop the filter entirely (indistinguishable from "no severity argument at all") instead of hitting
/// the same "unknown severity" rejection a bad STRING gets.
#[test]
fn a_non_string_severity_hits_the_same_rejection_path_as_an_unknown_string() {
    let args = serde_json::json!({ "severity": 5 });
    let err = FindingFilters::from_args(Some(&args)).unwrap_err();
    assert!(err.contains("unknown severity 5"), "got: {err}");
    assert!(err.contains("critical"));
}

/// `limit: 0` is legal — "counts only, no findings listed" — and must NOT be treated as "not
/// provided" (which would fall back to the default cap of 50).
#[test]
fn limit_zero_is_legal_and_distinct_from_absent() {
    let args = serde_json::json!({ "limit": 0 });
    let filters = FindingFilters::from_args(Some(&args)).unwrap();
    assert_eq!(filters.limit, Some(0));
}

/// Boundary-value torture round: `-1`, `1001`, `999999`, `"50"` (string), and `3.7` (float) all used
/// to be silently accepted and behave as "no cap" (`as_u64()` returning `None` on every one of them
/// was treated as "argument omitted"). Every one must now be a named rejection instead.
#[test]
fn out_of_range_and_wrong_type_limit_values_are_all_named_rejections() {
    for bad in [
        serde_json::json!(-1),
        serde_json::json!(1001),
        serde_json::json!(999_999),
        serde_json::json!("50"),
        serde_json::json!(3.7),
    ] {
        let args = serde_json::json!({ "limit": bad });
        let err = match FindingFilters::from_args(Some(&args)) {
            Err(e) => e,
            Ok(_) => panic!("limit {bad} must be rejected, got Ok"),
        };
        assert!(
            err.contains("limit must be an integer between 0 and 1000"),
            "limit {bad}: got {err}"
        );
    }
}

#[test]
fn a_valid_in_range_limit_is_accepted() {
    let args = serde_json::json!({ "limit": 500 });
    let filters = FindingFilters::from_args(Some(&args)).unwrap();
    assert_eq!(filters.limit, Some(500));
}

/// The wire-neutral constructor is what a non-JSON host (the `zzop` CLI) reaches for: already-parsed
/// values in, no MCP `tools/call` object fabricated on the way. `None`s are the unfiltered default view.
#[test]
fn the_wire_neutral_constructor_takes_parsed_values_with_no_json_in_sight() {
    let unfiltered = FindingFilters::new(None, None, None).expect("no filters always construct");
    assert_eq!(unfiltered.min_severity, None);
    assert_eq!(unfiltered.rule, None);
    assert_eq!(unfiltered.limit, None);

    let filtered = FindingFilters::new(Some("warning"), Some("sql/nplus1"), Some(0))
        .expect("valid values construct");
    assert_eq!(filtered.min_severity.as_deref(), Some("warning"));
    assert_eq!(filtered.rule.as_deref(), Some("sql/nplus1"));
    // `0` survives as `Some(0)` here for the same reason it does through `from_args`: "counts only"
    // is a real request, never "not provided".
    assert_eq!(filtered.limit, Some(0));
}

/// The two constructors share ONE validation vocabulary — a wire-neutral caller must not be the lenient
/// door into the shaping layer. Both rejections are asserted to carry the SAME message the JSON lane
/// produces, so a future divergence fails here rather than shipping two answers.
#[test]
fn the_wire_neutral_constructor_rejects_exactly_what_the_json_lane_rejects() {
    let neutral_severity = FindingFilters::new(Some("sev-nope"), None, None).unwrap_err();
    let json_severity =
        FindingFilters::from_args(Some(&serde_json::json!({ "severity": "sev-nope" })))
            .unwrap_err();
    assert_eq!(neutral_severity, json_severity);
    assert!(
        neutral_severity.contains("critical"),
        "got: {neutral_severity}"
    );

    let neutral_limit = FindingFilters::new(None, None, Some(1001)).unwrap_err();
    let json_limit =
        FindingFilters::from_args(Some(&serde_json::json!({ "limit": 1001 }))).unwrap_err();
    assert_eq!(neutral_limit, json_limit);
    assert!(
        neutral_limit.contains("limit must be an integer between 0 and 1000"),
        "got: {neutral_limit}"
    );

    // An in-range limit at the exact cap is accepted by both.
    assert_eq!(
        FindingFilters::new(None, None, Some(1000)).unwrap().limit,
        Some(1000)
    );
}

/// A `rule` NUMBER used to fall through `as_str()` and silently drop the filter (indistinguishable
/// from "no `rule` argument") — now a named type error, same class as `path`/`configPath`/`pattern`.
#[test]
fn a_non_string_rule_argument_is_a_named_type_error() {
    let args = serde_json::json!({ "rule": 42 });
    let err = FindingFilters::from_args(Some(&args)).unwrap_err();
    assert_eq!(err, "`rule` must be a string (got 42)");
}

#[test]
fn rule_filter_is_exact() {
    let findings = vec![finding("a", "info", 0), finding("ab", "info", 1)];
    let filters = FindingFilters {
        min_severity: None,
        rule: Some("a".into()),
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 1);
    assert_eq!(shaped["shown"][0]["ruleId"], "a");
}

#[test]
fn zero_match_rule_filter_for_a_nonexistent_rule_id_gets_a_disclosure_note() {
    let findings = vec![finding("a", "info", 0), finding("b", "warning", 1)];
    let filters = FindingFilters {
        min_severity: None,
        rule: Some("typo-d-rule-id".into()),
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 0);
    let note = shaped["note"].as_str().expect("note must be present");
    assert!(note.contains("typo-d-rule-id"));
    assert!(note.contains("byRule"));
    // Discoverability chain: the note must name the `rule-catalog` contract resource by name, not
    // just say "the catalog" with no pointer to where it lives (D10: the catalog was not previously
    // served over MCP at all).
    assert!(note.contains("rule-catalog"));
}

#[test]
fn zero_match_rule_filter_for_a_real_rule_with_no_findings_gets_no_bad_id_note() {
    // Rule "a" fired elsewhere in the run (present in byRule) but every finding got filtered out by
    // severity — a real, quiet rule, not a bad id. Must NOT get the nonexistent-id disclosure.
    let findings = vec![finding("a", "info", 0)];
    let filters = FindingFilters {
        min_severity: Some("critical".into()),
        rule: Some("a".into()),
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 0);
    let note = shaped["note"].as_str().unwrap_or("");
    assert!(
        !note.contains("is not among this run's fired rule ids"),
        "the rule id is real — doubting it would send the reader after the wrong thing: {note}"
    );
    // The SEVERITY filter did come up empty, though, and that is the half this fixture exercises: the
    // floor is what emptied the result, so that is what the run explains.
    assert!(
        note.contains("severity filter 'critical' matched no findings"),
        "{note}"
    );
}

/// The reported asymmetry: `--rule <typo>` explained its empty result and `--severity critical` did
/// not, returning `shown: []` with no `note` key at all. The silent one is the more dangerous of the
/// pair, since an empty severity-filtered list reads as an all-clear.
#[test]
fn zero_match_severity_filter_names_what_the_run_did_produce() {
    let findings = vec![
        finding("a", "info", 0),
        finding("b", "warning", 1),
        finding("c", "warning", 2),
    ];
    let filters = FindingFilters {
        min_severity: Some("critical".into()),
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 0);
    let note = shaped["note"].as_str().expect("note must be present");
    // The census, so "nothing critical" is distinguishable from "nothing at all".
    assert!(
        note.contains("info: 1") && note.contains("warning: 2"),
        "{note}"
    );
    assert!(note.contains("NOT that nothing was found"), "{note}");
}

/// The other side of that distinction. A genuinely empty run must say so rather than printing an
/// empty census, because "no findings above critical" and "no findings" are different answers and the
/// reader is asking which one they got.
#[test]
fn a_severity_filter_on_a_run_with_no_findings_at_all_says_so() {
    let filters = FindingFilters {
        min_severity: Some("warning".into()),
        rule: None,
        limit: None,
    };
    let shaped = shape(&[], &filters);
    let note = shaped["note"].as_str().expect("note must be present");
    assert!(note.contains("no findings at any severity"), "{note}");
}

/// Both filters empty at once must produce BOTH notes. The single-assignment form this replaced would
/// have silently dropped whichever one lost the race.
#[test]
fn a_run_where_both_filters_come_up_empty_keeps_both_notes() {
    let findings = vec![finding("a", "info", 0)];
    let filters = FindingFilters {
        min_severity: Some("critical".into()),
        rule: Some("typo-d-rule-id".into()),
        limit: None,
    };
    let note = shape(&findings, &filters)["note"]
        .as_str()
        .expect("note must be present")
        .to_string();
    assert!(note.contains("typo-d-rule-id"), "{note}");
    assert!(note.contains("severity filter 'critical'"), "{note}");
}

/// The invalidation that keeps the channel from becoming noise: a filter that MATCHED something says
/// nothing. Without it, "explain an empty result" and "annotate every filtered query" are the same
/// change.
#[test]
fn a_severity_filter_that_matched_something_adds_no_note() {
    let findings = vec![finding("a", "critical", 0), finding("b", "info", 1)];
    let filters = FindingFilters {
        min_severity: Some("critical".into()),
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 1);
    assert!(shaped.get("note").is_none());
}

#[test]
fn a_rule_filter_that_actually_matches_gets_no_note() {
    let findings = vec![finding("a", "info", 0)];
    let filters = FindingFilters {
        min_severity: None,
        rule: Some("a".into()),
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 1);
    assert!(shaped.get("note").is_none());
}

/// The deployment-role tier order, all THREE tiers, ACROSS severity bands. Widened 2026-08-24 from the
/// two-tier form this test was born in (the 2026-08-09 U78 ruling: the credential rules keep scanning
/// test paths — a committed secret is a leak wherever it sits, and the catalog states that policy — but
/// the FIRST SCREEN should not be a wall of `password123` fixtures). The third tier is BUILD SURFACE, and
/// it is named by facts rather than guesses: the tree's own `package.json` `scripts` declaration, plus the
/// two path shapes no project can rename. Since 2026-08-25 the role is the OUTER key, so these tiers are
/// no longer a within-band ordering — the pin below puts a shipped `warning` ahead of a test-path
/// `critical`. Shaping-only in every tier: nothing is dropped, `total`/`bySeverity`/`byRule` are
/// identical with or without any of it, and BOTH demotions announce themselves.
#[test]
fn test_path_and_build_findings_sort_after_every_shipped_finding_and_are_counted() {
    let findings = vec![
        // Engine order is deliberately worst-first, so every assertion below is a real move.
        serde_json::json!({ "ruleId": "security/shell-exec-interpolation", "severity": "warning",
            "file": "scripts/create-sentry-release.js", "line": 12 }),
        serde_json::json!({ "ruleId": "security/hardcoded-secret", "severity": "warning",
            "file": "src/app/__tests__/login.spec.ts", "line": 3 }),
        serde_json::json!({ "ruleId": "security/hardcoded-secret", "severity": "warning",
            "file": "src/app/config.ts", "line": 9 }),
        serde_json::json!({ "ruleId": "db/nplus1", "severity": "critical",
            "file": "e2e/seed.ts", "line": 1 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    // The tree's own manifest declared this one file a build script; nothing else on the wire says so.
    let declared: std::collections::HashSet<&str> =
        ["scripts/create-sentry-release.js"].into_iter().collect();
    let shaped = shape_findings(&findings, &filters, &declared);

    let shown = shaped["shown"].as_array().unwrap();
    // All three roles in order — shipped, then test path, then build surface — and role OUTRANKS
    // severity (2026-08-25): the shipped `warning` is row one and the test-path `critical` is row two.
    // Severity still orders WITHIN a role, which is why the test-path `critical` precedes the test-path
    // `warning` rather than merely landing somewhere below the shipped row.
    assert_eq!(shown[0]["file"], "src/app/config.ts", "{shaped}");
    assert_eq!(shown[0]["severity"], "warning", "{shaped}");
    assert_eq!(shown[1]["file"], "e2e/seed.ts", "{shaped}");
    assert_eq!(shown[1]["severity"], "critical", "{shaped}");
    assert_eq!(
        shown[2]["file"], "src/app/__tests__/login.spec.ts",
        "{shaped}"
    );
    assert_eq!(
        shown[3]["file"], "scripts/create-sentry-release.js",
        "{shaped}"
    );

    // Both demotions are disclosed, each with the FULL-set count (test: the spec.ts warning and the e2e
    // critical; build: the manifest-declared script).
    assert_eq!(shaped["testPaths"]["count"], 2, "{shaped}");
    let meaning = shaped["testPaths"]["meaning"].as_str().unwrap();
    assert!(
        meaning.contains("still"),
        "meaning must say the findings are still real leaks: {meaning}"
    );
    assert_eq!(shaped["buildPaths"]["count"], 1, "{shaped}");
    let build_meaning = shaped["buildPaths"]["meaning"].as_str().unwrap();
    assert!(
        build_meaning.contains("still real findings") && build_meaning.contains("Nothing is dropped"),
        "the build tier must announce itself as an ordering demotion, not a filter: {build_meaning}"
    );

    // Counts never moved.
    assert_eq!(shaped["total"], 4);
    assert_eq!(shaped["bySeverity"]["warning"], 3);
}

/// The build tier's OTHER source: the two ecosystem-fixed path shapes, which need no manifest at all and
/// therefore reach a tree whose `package.json` declares nothing (or that has none). Separate from the pin
/// above on purpose — that one would still pass if `build_path_re` were never consulted.
#[test]
fn ci_descriptors_and_example_templates_are_build_surface_without_any_manifest_declaration() {
    let findings = vec![
        serde_json::json!({ "ruleId": "security/config-file-secret", "severity": "critical",
            "file": "apps/api/v2/.env.example", "line": 43 }),
        serde_json::json!({ "ruleId": "security/shell-exec-interpolation", "severity": "critical",
            "file": ".github/workflows/release.yml", "line": 7 }),
        serde_json::json!({ "ruleId": "security/config-file-secret", "severity": "critical",
            "file": "apps/api/v2/src/config.ts", "line": 5 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    // EMPTY declaration — a tree whose manifests named nothing, or that has no manifest at all. The path
    // shapes must still classify.
    let shaped = shape_findings(&findings, &filters, &Default::default());
    let shown = shaped["shown"].as_array().unwrap();
    assert_eq!(shown[0]["file"], "apps/api/v2/src/config.ts", "{shaped}");
    assert_eq!(shaped["buildPaths"]["count"], 2, "{shaped}");
    assert_eq!(shaped["total"], 3);
    assert!(shaped.get("testPaths").is_none(), "{shaped}");
}

/// Precedence, stated as a test because the two lower predicates genuinely overlap: a manifest-declared
/// script that also sits under `fixtures/` is BUILD surface (the manifest is a declaration, the directory
/// name is a name), and the two independent disclosure counts both include it without either claiming to
/// be a partition.
#[test]
fn build_surface_outranks_test_path_and_the_two_counts_may_overlap() {
    let findings = vec![
        serde_json::json!({ "ruleId": "r", "severity": "warning",
            "file": "fixtures/seed-db.ts", "line": 1 }),
        serde_json::json!({ "ruleId": "r", "severity": "warning",
            "file": "src/other.spec.ts", "line": 1 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let declared: std::collections::HashSet<&str> = ["fixtures/seed-db.ts"].into_iter().collect();
    let shaped = shape_findings(&findings, &filters, &declared);
    let shown = shaped["shown"].as_array().unwrap();
    assert_eq!(shown[0]["file"], "src/other.spec.ts", "{shaped}");
    assert_eq!(shown[1]["file"], "fixtures/seed-db.ts", "{shaped}");
    // Both counts see it: `testPaths` still counts every test path (its published sentence is about test
    // paths, and quietly excluding the overlap would make that sentence false).
    assert_eq!(shaped["testPaths"]["count"], 2, "{shaped}");
    assert_eq!(shaped["buildPaths"]["count"], 1, "{shaped}");
}

#[test]
fn a_run_with_no_test_path_findings_carries_no_test_paths_key() {
    // Additive-only, like `truncated`: the key exists exactly when it has something to say. A constant
    // `testPaths: {count: 0}` on every reply would be noise the primary reader (an agent) must skip.
    let findings = vec![
        serde_json::json!({ "ruleId": "a", "severity": "info", "file": "src/a.ts", "line": 1 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert!(shaped.get("testPaths").is_none(), "{shaped}");
}

/// The build tier's twin of the pin above, and the one that matters most on this axis: most trees have no
/// build surface at all, so ABSENT and `{"count": 0}` must not be the same bytes. An always-present zero
/// is a field every reply on every such tree makes an agent read and discard.
#[test]
fn a_run_with_no_build_surface_findings_carries_no_build_paths_key() {
    let findings = vec![
        serde_json::json!({ "ruleId": "a", "severity": "info", "file": "src/a.ts", "line": 1 }),
        serde_json::json!({ "ruleId": "b", "severity": "info", "file": "src/a.spec.ts", "line": 1 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert!(shaped.get("buildPaths").is_none(), "{shaped}");
    // ...and the reply is otherwise exactly what it was before this tier existed: the test-path key is
    // present and unchanged, so a tree with no build surface pays nothing for the axis.
    assert_eq!(shaped["testPaths"]["count"], 1, "{shaped}");
}

/// A manifest declaration that names a file NO finding sits on must change nothing at all — the axis
/// reads findings' own paths, never the declaration as a filter over the reply.
#[test]
fn a_declaration_that_matches_no_finding_leaves_the_reply_byte_identical() {
    let findings = vec![
        finding("a", "warning", 0),
        serde_json::json!({ "ruleId": "b", "severity": "warning", "file": "src/b.ts", "line": 2 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let declared: std::collections::HashSet<&str> =
        ["scripts/nothing-fires-here.ts"].into_iter().collect();
    let with = serde_json::to_string(&shape_findings(&findings, &filters, &declared)).unwrap();
    let without = serde_json::to_string(&shape(&findings, &filters)).unwrap();
    assert_eq!(with, without);
}

/// 🔴 THE ORDERING CONTRACT ITSELF: deployment role outranks severity.
///
/// This is the pin the 2026-08-25 blind audit bought. A reader who stops when the list stops paying
/// stops at the end of the `critical` band — so a `critical` band made entirely of fixtures and build
/// scripts is a first screen that found nothing, no matter how correct every row in it is. Role-first
/// ordering says: a finding in code the tree does not ship sorts below EVERY shipped finding, whatever
/// band it carries. Inside `SHIPPED` severity orders exactly as it always did, so a shipped `critical`
/// is still row one overall.
///
/// The engine index stays the last tiebreak (`crates/core/src/registry/merge.rs`), so the same analysis
/// still produces byte-identical output.
#[test]
fn deployment_role_outranks_severity_so_a_shipped_warning_beats_an_unshipped_critical() {
    let findings = vec![
        // Engine order is deliberately best-first here, so every assertion below is a real move.
        serde_json::json!({ "ruleId": "security/config-file-secret", "severity": "critical",
            "file": "apps/api/v2/.env.example", "line": 43 }),
        serde_json::json!({ "ruleId": "security/hardcoded-secret", "severity": "critical",
            "file": "packages/lib/__tests__/redaction.spec.ts", "line": 7 }),
        serde_json::json!({ "ruleId": "db/nplus1", "severity": "warning",
            "file": "packages/features/bookings/list.ts", "line": 88 }),
        serde_json::json!({ "ruleId": "perf/console-in-be", "severity": "info",
            "file": "packages/features/bookings/create.ts", "line": 12 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters, &Default::default());
    let order: Vec<&str> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        vec![
            // SHIPPED, severity-desc within the tier — unchanged from what it always was.
            "packages/features/bookings/list.ts",
            "packages/features/bookings/create.ts",
            // TEST_PATH — a `critical` that does not ship, below every shipped row including the `info`.
            "packages/lib/__tests__/redaction.spec.ts",
            // BUILD_SURFACE — last, as it was relative to test paths.
            "apps/api/v2/.env.example",
        ],
        "{shaped}"
    );
    // Presentation only: not one count moved.
    assert_eq!(shaped["total"], 4, "{shaped}");
    assert_eq!(shaped["bySeverity"]["critical"], 2, "{shaped}");
    assert_eq!(shaped["bySeverity"]["warning"], 1, "{shaped}");
    assert_eq!(shaped["bySeverity"]["info"], 1, "{shaped}");
}

/// RULE ROUND-ROBIN, the third ordering key (2026-08-26). Within one (deployment role, severity) band the
/// tiebreak used to be the engine index, which for a merged registry is the full path in lexicographic
/// order — so the first screen was not "the worst 40" but "the alphabetically-first 40", and a rule whose
/// findings all sit under a late-sorting directory could not reach it at any count. Measured on cal.com:
/// `db/pagination-no-orderby` first appeared at rank 8 and `schema/fk-no-index` at rank 239, same role,
/// same severity — 231 places decided entirely by the letters in a path.
///
/// The key is each finding's OCCURRENCE INDEX within its own (role, severity, ruleId) group, ascending:
/// every rule's 1st finding, then every rule's 2nd, and so on. Engine order remains the final tiebreak,
/// so the ordering is still a pure function of the pre-sort list and byte-identical across runs.
///
/// Computed and never emitted — the index is not a wire field, which is the line `output-philosophy` §12
/// draws (a scalar a caller can threshold on is the thing that is forbidden, not the act of ranking).
#[test]
fn rule_round_robin_orders_the_nth_finding_of_every_rule_before_any_rules_n_plus_first() {
    // Engine order is deliberately fully clustered per rule — exactly the shape that made an entire rule
    // unreachable on the first screen.
    let findings = vec![
        finding("a", "warning", 0),
        finding("a", "warning", 1),
        finding("a", "warning", 2),
        finding("b", "warning", 3),
        finding("b", "warning", 4),
        finding("c", "warning", 5),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    let order: Vec<&str> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        // round 1: a,b,c — round 2: a,b — round 3: a. Engine order breaks ties inside a round.
        vec!["f0.ts", "f3.ts", "f5.ts", "f1.ts", "f4.ts", "f2.ts"],
        "{shaped}"
    );
    // Presentation only, in both directions: not one count moved.
    assert_eq!(shaped["total"], 6, "{shaped}");
    assert_eq!(shaped["bySeverity"]["warning"], 6, "{shaped}");
    assert_eq!(shaped["byRule"]["a"], 3, "{shaped}");
    assert_eq!(shaped["byRule"]["b"], 2, "{shaped}");
    assert_eq!(shaped["byRule"]["c"], 1, "{shaped}");
}

/// RULE-IN-FILE, the fourth ordering key and the one that sits ABOVE the rule round-robin above
/// (2026-09-05). The round-robin fixed "the window is the alphabetically-first 40" but could not see the
/// duplication that remained, because that duplication is INSIDE a rule: measured by opening all 120
/// first-screen rows of three real projects against their source, 14 OF ONE PROJECT'S 40 ROWS were the
/// second or third instance of a judgment an earlier row had already delivered — one config file's dict
/// taking three slots, one lockfile package taking two, one controller shape taking three. A reader who
/// has read row 1 learns nothing from rows 2 and 3, and a forty-row budget spent fourteen times on
/// "I saw this already" is a first screen that found less than it says.
///
/// The key is each finding's occurrence index within its own (role, severity, ruleId, FILE) group: every
/// (rule, file) pair spends its first slot before any pair spends a second, and the rule round-robin then
/// orders the inside of each round. It could not have been the rule key alone (the duplication is inside
/// a rule) and it could not have been the file alone (one rule spread over twenty files would take twenty
/// round-one slots and starve every other rule — the exact failure the rule key exists to prevent).
///
/// A finding with NO file — cross-layer findings and hand-built values both reach here — keys on the same
/// empty string as every other file-less finding of its rule, which makes this index identical to the
/// round-robin's for that population and therefore a no-op on it. That is the intended reading: the key
/// can only ever separate findings it can prove sit in the same file.
#[test]
fn rule_in_file_gives_every_file_of_a_rule_a_slot_before_any_file_takes_a_second() {
    // Rule `a` fires three times in one file and once in a second; rule `b` twice in a third. Engine
    // order is clustered, which is the shape a per-file rule genuinely produces.
    let findings = vec![
        serde_json::json!({ "ruleId": "a", "severity": "warning", "file": "src/x.ts", "line": 1 }),
        serde_json::json!({ "ruleId": "a", "severity": "warning", "file": "src/x.ts", "line": 2 }),
        serde_json::json!({ "ruleId": "a", "severity": "warning", "file": "src/x.ts", "line": 3 }),
        serde_json::json!({ "ruleId": "a", "severity": "warning", "file": "src/y.ts", "line": 4 }),
        serde_json::json!({ "ruleId": "b", "severity": "warning", "file": "src/z.ts", "line": 5 }),
        serde_json::json!({ "ruleId": "b", "severity": "warning", "file": "src/z.ts", "line": 6 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    let order: Vec<String> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{}:{}",
                f["file"].as_str().unwrap(),
                f["line"].as_u64().unwrap()
            )
        })
        .collect();
    assert_eq!(
        order,
        // Round 1 is one row per (rule, file): x, z, then y — inside the round the rule round-robin
        // still leads, so a#1 and b#1 come before a's SECOND file. Round 2 is x, z. Round 3 is x.
        // The pin's whole point is `src/y.ts:4`: under the previous three keys it sat LAST, behind
        // both repeats of a file the reader had already been shown.
        vec![
            "src/x.ts:1",
            "src/z.ts:5",
            "src/y.ts:4",
            "src/x.ts:2",
            "src/z.ts:6",
            "src/x.ts:3",
        ],
        "{shaped}"
    );
    // Presentation only, in both directions: not one count moved, and nothing left the reply.
    assert_eq!(shaped["total"], 6, "{shaped}");
    assert_eq!(shaped["byRule"]["a"], 4, "{shaped}");
    assert_eq!(shaped["byRule"]["b"], 2, "{shaped}");
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 6, "{shaped}");
}

/// SITE, the fifth ordering key and the one that leads all three diversity keys (2026-09-05). It is
/// asked of no rule at all: how many findings — any rule's — already pointed at this exact file:line.
///
/// Same measurement as the key above, its second shape: two DIFFERENT rules fired on one line of one file
/// and took two of that project's forty first-screen slots, and their verdicts were OPPOSITE (one defect
/// worth fixing, one deliberate by design), so the pair was not even a corroboration — the reader had to
/// read both rows to learn that the second disagreed with the first. One line of source, two slots.
///
/// It leads because it is the stronger form of the same reader question. "Have I already been told this?"
/// is answered by the rule in the file; "have I already been sent HERE?" is answered by the site, and a
/// reader who has read a row is standing at its line whichever rule wrote it.
///
/// Nothing is merged and nothing may be: two rules on one line are two findings, both true, both counted,
/// both in the reply. The second one waits until every other place has had its turn — which is exactly
/// what makes the disagreement between them worth a slot when it arrives.
#[test]
fn a_second_rule_on_a_line_that_already_has_a_row_waits_for_every_other_place() {
    let findings = vec![
        serde_json::json!({ "ruleId": "a", "severity": "warning", "file": "src/x.ts", "line": 171 }),
        serde_json::json!({ "ruleId": "b", "severity": "warning", "file": "src/x.ts", "line": 171 }),
        serde_json::json!({ "ruleId": "a", "severity": "warning", "file": "src/y.ts", "line": 5 }),
        serde_json::json!({ "ruleId": "c", "severity": "warning", "file": "src/z.ts", "line": 9 }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    let order: Vec<String> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{} {}:{}",
                f["ruleId"].as_str().unwrap(),
                f["file"].as_str().unwrap(),
                f["line"].as_u64().unwrap()
            )
        })
        .collect();
    assert_eq!(
        order,
        // Every distinct site first — inside that round the rule round-robin still leads, which is why
        // `c` precedes `a`'s second finding. `b`, the second row on a line already spoken for, is last.
        // Under the four keys that preceded this one it sat at slot TWO, immediately under the row it
        // disagreed with.
        vec![
            "a src/x.ts:171",
            "c src/z.ts:9",
            "a src/y.ts:5",
            "b src/x.ts:171",
        ],
        "{shaped}"
    );
    // Presentation only, in both directions: nothing merged, nothing dropped, no count moved.
    assert_eq!(shaped["total"], 4, "{shaped}");
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 4, "{shaped}");
    assert_eq!(shaped["byRule"]["a"], 2, "{shaped}");
    assert_eq!(shaped["byRule"]["b"], 1, "{shaped}");
    assert_eq!(shaped["byRule"]["c"], 1, "{shaped}");
}

/// The site key's NEGATIVE canary, and the reason a missing file takes 0 instead of joining a shared
/// bucket: findings that share the ABSENCE of a file do not share a place. Cross-layer findings and
/// hand-built values both reach the shaper with no file, and if they collided on one site key the first
/// of them would take a round-one slot and the rest would sort behind every located finding in the tree —
/// a silent demotion of an entire population on the strength of a field they never carry.
#[test]
fn findings_with_no_file_do_not_collide_on_the_site_key() {
    let findings = vec![
        finding("a", "warning", 0),
        finding("a", "warning", 1),
        finding("b", "warning", 2),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    let order: Vec<&str> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    // Exactly the rule round-robin's answer, unchanged: both new keys are no-ops on a population with
    // no file.
    assert_eq!(order, vec!["f0.ts", "f2.ts", "f1.ts"], "{shaped}");
    assert_eq!(shaped["total"], 3, "{shaped}");
}

/// The WIRING pin for `byDirectory` — the arithmetic is `super::by_directory`'s to prove; what this
/// asserts is the property only the SHAPER can get wrong, and the one the channel's own note promises
/// in as many words: the fold runs over the FULL set, never over the filtered window.
///
/// It is the same mistake `bySeverity` would make if it were computed after `window_order`, and it is
/// the one that turns a disclosure into a lie: a reader who added `--severity critical` and read
/// "96.4% in `examples/`" would be reading a share of their own filter while the sentence beside it
/// says otherwise. The fixture puts the majority directory's findings BELOW the filter's floor, so a
/// filtered fold could not produce this answer.
#[test]
fn the_directory_fold_counts_the_full_set_and_not_the_filtered_window() {
    let mut findings: Vec<serde_json::Value> = (0..9)
        .map(|i| serde_json::json!({ "ruleId": "a", "severity": "info", "file": format!("examples/e{i}.ts") }))
        .collect();
    findings.push(serde_json::json!({ "ruleId": "a", "severity": "critical", "file": "lib/x.ts" }));
    let filters = FindingFilters {
        min_severity: Some("critical".into()),
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 1, "{shaped}");
    // `shape_findings` RETURNS the `findings` object, so this is the reply's `findings.byDirectory`.
    let rows = shaped["byDirectory"]["directories"]
        .as_array()
        .unwrap_or_else(|| panic!("byDirectory must ride beside byRule: {shaped}"));
    assert_eq!(rows[0]["dir"], "examples/", "{shaped}");
    assert_eq!(rows[0]["findings"], 9, "{shaped}");
    assert_eq!(rows[0]["sharePct"], 90.0, "{shaped}");
    assert!(
        shaped["byDirectory"]["basis"]
            .as_str()
            .is_some_and(|b| b.starts_with("10 finding(s)")),
        "the basis population is the full set too: {shaped}"
    );
}

/// The WIRING pin for `byRuleMeaning` (the legend's own content is pinned in
/// `super::by_rule_legend`'s tests). Unconditional and on BOTH lanes: `shape_findings` is the single
/// shaper behind `findings` and `crossLayerFindings`, so one call site proves both — and the fixture
/// here deliberately contains NO folding finding, which is exactly the reply whose reader would
/// otherwise infer that comparing these counts across rules is safe.
#[test]
fn every_by_rule_map_ships_its_legend_even_when_no_finding_folded() {
    let findings = vec![finding("a", "warning", 0), finding("b", "info", 1)];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters, &Default::default());
    let meaning = shaped["byRuleMeaning"]
        .as_str()
        .unwrap_or_else(|| panic!("byRuleMeaning must ride beside byRule: {shaped}"));
    assert!(meaning.contains("Counts FINDINGS, not places"), "{meaning}");
    // Presentation only: the legend adds no count and moves none.
    assert_eq!(shaped["total"], 2, "{shaped}");
    assert_eq!(shaped["byRule"]["a"], 1, "{shaped}");
}

/// The WIRING pin for `shownMeaning`, plus the POSITION pin for the one reading that makes it worth
/// bytes. `shown` is an ORDER and a reader takes it for a RANKING — that row 1 is likeliest to be real
/// and a window of forty is "the forty worst". Neither was ever claimed on the wire, which is the
/// defect: the claim was ABSENT, not false, and an absent claim gets supplied by the reader. So the
/// caveat leads and the five-key recital follows; a note that ends in the caveat is a note whose first
/// two thirds can be skimmed.
///
/// Unconditional, exactly like `byRuleMeaning`: the key it explains rides every reply, so the reader it
/// could mislead is on every reply too. The fixture here deliberately has nothing interesting about its
/// order — two findings, two rules, no repeat — which is precisely the reply whose reader would
/// otherwise never be told what the sequence does and does not mean.
#[test]
fn every_shown_list_ships_the_legend_that_says_it_is_not_a_ranking() {
    let findings = vec![finding("a", "warning", 0), finding("b", "info", 1)];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters, &Default::default());
    let meaning = shaped["shownMeaning"]
        .as_str()
        .unwrap_or_else(|| panic!("shownMeaning must ride beside shown: {shaped}"));
    assert!(
        meaning.starts_with("An ORDER, not a RANKING"),
        "the caveat has to survive a skim, so it leads: {meaning}"
    );
    assert!(
        meaning.contains("no per-finding confidence exists here"),
        "the note must keep WHY there is no quality order, not only that there is none: {meaning}"
    );
    // The FOLD: the full text lives in the reply-legends document, and a fold that ships both is two
    // copies. `crates/summary/tests/legend_fold.rs` proves this over real replies for every folded key;
    // this line is the local canary that the string on the wire is the note and not the body.
    assert!(
        !meaning.contains("Never as: the N worst things in this tree"),
        "the folded note must not carry the full text's closing line: {meaning}"
    );
    // Presentation only: the legend adds no count and moves none.
    assert_eq!(shaped["total"], 2, "{shaped}");
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 2, "{shaped}");
}

/// The round-robin's NEGATIVE canary, and the reason the counter is keyed by (role, severity, rule)
/// rather than by rule alone: a rule that fires in two severity bands restarts at 1 in each. Without the
/// band in the key, `a`'s third warning would carry index 2 and sort behind `b`'s first warning even
/// though `a`'s critical already spent that rule's turn in a DIFFERENT band — one band's traffic would
/// silently reorder another's. Also pins the no-op case: one rule per band leaves engine order untouched,
/// so a tree whose findings are already diverse sorts exactly as it did before this key existed.
#[test]
fn round_robin_counters_restart_per_severity_band_and_are_a_no_op_when_every_rule_fires_once() {
    let findings = vec![
        finding("a", "critical", 0),
        finding("a", "warning", 1),
        finding("b", "warning", 2),
        finding("a", "warning", 3),
        finding("b", "critical", 4),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    let order: Vec<&str> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        vec![
            // critical band: one `a` and one `b`, both occurrence 0 — engine order, untouched.
            "f0.ts", "f4.ts",
            // warning band: a's counter RESTARTED here, so a#1 and b#1 lead and a#2 follows.
            "f1.ts", "f2.ts", "f3.ts",
        ],
        "{shaped}"
    );
    assert_eq!(shaped["bySeverity"]["critical"], 2, "{shaped}");
    assert_eq!(shaped["bySeverity"]["warning"], 3, "{shaped}");
}

// ---------------------------------------------------------------------------------------------
// What the CUT swallowed, by severity (2026-08-26). The defect these five pin: on cal.com the SAME
// reply said `bySeverity: {critical: 6}` in one field and carried zero `critical` rows in a
// 1000-row `shown`, and no sentence anywhere said so. `ac9795b` filed this exact residue ("on the
// two trees over the 1000-row cap the demoted rows now leave the reply entirely"). Ordering is NOT
// what changes here — only what the reply says about its own cut.
// ---------------------------------------------------------------------------------------------

/// cal.com in miniature: every `critical` sits on demoted surface, so role-desc puts all of them
/// behind every shipped row, and a cap that lands inside the shipped band removes the whole
/// severity from `shown` while `bySeverity` keeps saying it is there.
#[test]
fn a_severity_the_cut_removed_entirely_is_named_with_its_count() {
    let findings = vec![
        serde_json::json!({ "ruleId": "a/one", "severity": "info", "file": "src/a.ts" }),
        serde_json::json!({ "ruleId": "b/two", "severity": "info", "file": "src/b.ts" }),
        serde_json::json!({ "ruleId": "c/three", "severity": "info", "file": "src/c.ts" }),
        serde_json::json!({ "ruleId": "security/private-key-committed", "severity": "critical",
            "file": "src/__tests__/redaction.spec.ts" }),
        serde_json::json!({ "ruleId": "security/shell-exec-interpolation", "severity": "critical",
            "file": ".github/workflows/release.yml" }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: Some(2),
    };
    let shaped = shape(&findings, &filters);

    // Precondition: the cut really did take the whole `critical` band.
    let shown = shaped["shown"].as_array().unwrap();
    assert_eq!(shown.len(), 2, "{shaped}");
    assert!(shown.iter().all(|f| f["severity"] == "info"), "{shaped}");
    assert_eq!(shaped["bySeverity"]["critical"], 2, "{shaped}");

    // The reply must SAY it.
    assert_eq!(
        shaped["truncated"]["severitiesNotShown"]["counts"]["critical"], 2,
        "the cut removed both criticals and the reply has to name that: {shaped}"
    );
    // ...and only that. `info` is in `shown`, so it is not silenced.
    assert!(
        shaped["truncated"]["severitiesNotShown"]["counts"]
            .get("info")
            .is_none(),
        "a severity with rows in `shown` was not silenced: {shaped}"
    );
    let meaning = shaped["truncated"]["severitiesNotShown"]["meaning"]
        .as_str()
        .unwrap();
    assert!(
        meaning.contains("zero rows in `shown`"),
        "the disclosure has to state its own membership rule: {meaning}"
    );
}

/// No severity is hardcoded. The day `warning` is the band the cut removes, the same key names
/// `warning` — this pin fails if the implementation ever asks "is it critical?".
#[test]
fn the_silenced_severity_is_computed_not_a_hardcoded_critical() {
    let findings = vec![
        serde_json::json!({ "ruleId": "a/one", "severity": "critical", "file": "src/a.ts" }),
        serde_json::json!({ "ruleId": "b/two", "severity": "warning",
            "file": "src/__tests__/x.spec.ts" }),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: Some(1),
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(
        shaped["truncated"]["severitiesNotShown"]["counts"]["warning"], 1,
        "{shaped}"
    );
    assert!(
        shaped["truncated"]["severitiesNotShown"]["counts"]
            .get("critical")
            .is_none(),
        "{shaped}"
    );
}

/// A cut that silenced nothing still ANSWERS — an empty object, never a missing key. A missing
/// sub-key would make "nothing was silenced" and "this build does not compute that" the same bytes,
/// which is the §1 failure this module exists to prevent. And a list that FITS says nothing at all:
/// `truncated` itself stays absent, so a tree under the cap pays no bytes for the axis.
#[test]
fn a_cut_that_silenced_no_severity_says_so_and_an_uncut_list_says_nothing() {
    let findings: Vec<_> = (0..5).map(|i| finding("r", "info", i)).collect();
    let cut = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(2),
        },
    );
    assert_eq!(
        cut["truncated"]["severitiesNotShown"]["counts"],
        serde_json::json!({}),
        "a cut that silenced nothing must still answer: {cut}"
    );

    let whole = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(5),
        },
    );
    assert!(whole.get("truncated").is_none(), "{whole}");
}

/// The population is the set the CAP was applied to, not the raw census. A caller who asked for
/// `severity: "warning"` removed `info` themselves; telling them `info` is "not shown" would be a
/// false disclosure, and a false disclosure teaches a reader to distrust true findings. The
/// `critical` band, which their filter ADMITTED and the cap then cut, is still named — that one is
/// information.
#[test]
fn a_filtered_run_names_only_what_the_cap_took_not_what_the_filter_took() {
    let findings = vec![
        serde_json::json!({ "ruleId": "a/one", "severity": "warning", "file": "src/a.ts" }),
        serde_json::json!({ "ruleId": "b/two", "severity": "warning", "file": "src/b.ts" }),
        serde_json::json!({ "ruleId": "c/three", "severity": "info", "file": "src/c.ts" }),
        serde_json::json!({ "ruleId": "security/private-key-committed", "severity": "critical",
            "file": "src/__tests__/redaction.spec.ts" }),
    ];
    let shaped = shape(
        &findings,
        &FindingFilters {
            min_severity: Some("warning".into()),
            rule: None,
            limit: Some(1),
        },
    );
    let counts = &shaped["truncated"]["severitiesNotShown"]["counts"];
    assert_eq!(counts["critical"], 1, "the cap took it: {shaped}");
    assert!(
        counts.get("info").is_none(),
        "the FILTER took `info`, not the cap — naming it here is a false disclosure: {shaped}"
    );
    // The unfiltered census is still right above, unchanged by any of this.
    assert_eq!(shaped["bySeverity"]["info"], 1, "{shaped}");
}

/// `raise the limit` is FALSE at the ceiling: a `limit` above `MAX_LIMIT` is a named usage error
/// (`filters::parse_limit`), so at 1000 the only remedies left are `severity` and `rule`. A hint
/// that names an inert remedy is the same defect `shape_list_echoes_the_callers_hint_verbatim`
/// already pins for the fixed-cap lists — it just had a second home here.
#[test]
fn the_hint_stops_offering_a_bigger_limit_once_the_limit_is_at_its_ceiling() {
    let findings: Vec<_> = (0..MAX_LIMIT + 1)
        .map(|i| finding("r", "info", i))
        .collect();

    let at_ceiling = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(MAX_LIMIT),
        },
    );
    let hint = at_ceiling["truncated"]["hint"].as_str().unwrap();
    assert!(
        !hint.contains("raise"),
        "at the ceiling `raise the limit` is advice that exits 2: {hint}"
    );
    // The ceiling is read from the CONSTANT, not typed in: a hint quoting a number the validator no
    // longer enforces is the same lie as an inert remedy, one `const` edit away.
    assert!(
        hint.contains("severity") && hint.contains("rule") && hint.contains(&MAX_LIMIT.to_string()),
        "the hint has to leave the caller with the remedies that DO work, and say why: {hint}"
    );

    let below = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(10),
        },
    );
    let hint = below["truncated"]["hint"].as_str().unwrap();
    assert!(
        hint.contains("raise the limit"),
        "below the ceiling raising it really does move the cap: {hint}"
    );
}

/// The count says a band left. It does not say WHAT left, and the exit code is where that gap is
/// measurable: `analyze --config <cal.com> --limit 1000 --fail-on critical` exits 3 naming
/// `6 critical` while no file, line or rule id of those six appears in the reply the same command
/// printed. A build breaks and its own artifact carries no evidence.
///
/// So a silenced severity names SITES, in the order `shown` itself uses. The two assertions below
/// are one invariant in two halves, and either alone is satisfiable by a lie: "some anchor exists"
/// is met by any row at all, and "the count is right" was already met before this key existed.
#[test]
fn a_silenced_severity_is_named_with_sites_not_only_a_count() {
    let findings = vec![
        serde_json::json!({ "ruleId": "a/one", "severity": "info", "file": "src/a.ts", "line": 1 }),
        serde_json::json!({ "ruleId": "b/two", "severity": "info", "file": "src/b.ts", "line": 2 }),
        serde_json::json!({ "ruleId": "security/private-key-committed", "severity": "critical",
            "file": "src/__tests__/redaction.spec.ts", "line": 12 }),
        serde_json::json!({ "ruleId": "security/shell-exec-interpolation", "severity": "critical",
            "file": ".github/workflows/release.yml", "line": 30 }),
    ];
    let shaped = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(2),
        },
    );

    // Precondition: this is the cal.com shape — the whole `critical` band is outside `shown`.
    let shown = shaped["shown"].as_array().unwrap();
    assert!(
        shown.iter().all(|f| f["severity"] != "critical"),
        "the cut has to have taken the whole band or this test proves nothing: {shaped}"
    );
    assert_eq!(shaped["bySeverity"]["critical"], 2, "{shaped}");

    let named = shaped["truncated"]["severitiesNotShown"]["firstOmitted"]["critical"]
        .as_array()
        .unwrap_or_else(|| {
            panic!(
                "a severity the cut removed entirely has to name sites, not only a count: {shaped}"
            )
        });
    // The rows named are the cut ones, in sort order — the demoted test path sorts ahead of the
    // build surface, exactly as `shown` would have ordered them had the cap reached that far.
    assert_eq!(named.len(), 2, "{shaped}");
    assert_eq!(
        named[0]["ruleId"], "security/private-key-committed",
        "{shaped}"
    );
    assert_eq!(
        named[0]["file"], "src/__tests__/redaction.spec.ts",
        "{shaped}"
    );
    assert_eq!(named[0]["line"], 12, "{shaped}");
    assert_eq!(
        named[1]["file"], ".github/workflows/release.yml",
        "{shaped}"
    );
    // No `message`. The cap exists to bound this reply, and re-admitting the biggest field a
    // finding carries through the disclosure would undo it.
    assert!(named[0].get("message").is_none(), "{shaped}");

    // A severity `shown` still carries needs no anchors — the reader can already see one.
    assert!(
        shaped["truncated"]["severitiesNotShown"]["firstOmitted"]
            .get("info")
            .is_none(),
        "`info` is visible in `shown`; naming it here would claim a silence that did not happen: {shaped}"
    );
}

/// A SAMPLE, and bounded — `--limit 0` silences every severity in the run, and an unbounded anchor
/// list would hand the token bomb this module exists to stop back through the disclosure door. The
/// count beside it stays exact, so the sample is stated rather than implied. And when the cut
/// silenced nothing the key is still `{}`, never missing, for the same reason `counts` is.
#[test]
fn the_named_sites_are_a_bounded_sample_beside_an_exact_count() {
    let findings: Vec<_> = (0..9)
        .map(|i| finding("security/private-key-committed", "critical", i))
        .collect();
    let shaped = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(0),
        },
    );
    let silenced = &shaped["truncated"]["severitiesNotShown"];
    assert_eq!(
        silenced["counts"]["critical"], 9,
        "the count is the whole set: {shaped}"
    );
    let named = silenced["firstOmitted"]["critical"]
        .as_array()
        .unwrap_or_else(|| {
            panic!("`--limit 0` silences every severity, so all of them have to be named: {shaped}")
        });
    assert!(
        named.len() < 9 && !named.is_empty(),
        "the sites are a bounded sample of that set, neither empty nor all of it: {shaped}"
    );
    let meaning = silenced["meaning"].as_str().unwrap();
    assert!(
        meaning.contains(&named.len().to_string()) && meaning.contains("SAMPLE"),
        "and the reply says the number is a sample and how big, from the constant rather than \
         typed in: {meaning}"
    );

    let nothing_silenced = shape(
        &(0..5).map(|i| finding("r", "info", i)).collect::<Vec<_>>(),
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(2),
        },
    );
    assert_eq!(
        nothing_silenced["truncated"]["severitiesNotShown"]["firstOmitted"],
        serde_json::json!({}),
        "an empty object, never a missing key — the same contract `counts` keeps: {nothing_silenced}"
    );
}

/// `counts` answers "how many rows left"; `ruleCounts` answers "how many RULES left", and the two
/// come apart exactly where it matters. Measured on `cases/trees/api-be` after the 2026-09-03 band
/// move: 30 silenced `info` rows drawn from NINETEEN distinct rules -- a reader handed only `30`
/// and three anchors reads "a few noisy rules" and is wrong about sixteen of them.
///
/// Both directions are pinned, because one alone is satisfiable by a bug. Equal counts would pass a
/// `ruleCounts` that just echoed `counts`, so the fixture below deliberately gives one severity MANY
/// rows from FEW rules; and a `ruleCounts` that counted the whole population rather than the cut
/// tail would also be wrong, so the shown rules must not be in it.
#[test]
fn the_silenced_band_names_how_many_rules_it_hid_not_only_how_many_rows() {
    // 9 critical rows from 3 rules, all cut by `--limit 0`.
    let mut findings = Vec::new();
    for rule in ["security/a-rule", "security/b-rule", "security/c-rule"] {
        for i in 0..3 {
            findings.push(finding(rule, "critical", i));
        }
    }
    let shaped = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(0),
        },
    );
    let silenced = &shaped["truncated"]["severitiesNotShown"];
    assert_eq!(
        silenced["counts"]["critical"], 9,
        "rows are counted whole: {shaped}"
    );
    assert_eq!(
        silenced["ruleCounts"]["critical"], 3,
        "and the RULES behind them are counted separately -- 9 rows, 3 rules, and a `ruleCounts` \
         that merely echoed `counts` would say 9 here: {shaped}"
    );
    let meaning = silenced["meaning"].as_str().unwrap();
    assert!(
        meaning.contains("ruleCounts") && meaning.contains("DISTINCT RULES"),
        "the reply says what the second number is, or it reads as a duplicate of the first: {meaning}"
    );

    // The tail, not the population: with a limit that SHOWS one rule, that rule must not be counted
    // among the hidden ones. `info` sorts below `critical`, so a limit of 1 shows a critical row and
    // silences the whole `info` band.
    let mixed = shape(
        &[
            finding("security/shown-rule", "critical", 1),
            finding("security/hidden-one", "info", 2),
            finding("security/hidden-two", "info", 3),
        ],
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: Some(1),
        },
    );
    let hidden = &mixed["truncated"]["severitiesNotShown"];
    assert_eq!(
        hidden["ruleCounts"],
        serde_json::json!({ "info": 2 }),
        "only the CUT tail is counted, and only for a severity `shown` holds none of -- the shown \
         rule contributes nothing here: {mixed}"
    );
}
// ---------------------------------------------------------------------------
// THE PER-REPLY PROSE FOLD (2026-08-31) — see `super::rule_prose` for the why.
// Two pins in OPPOSITE directions. "No text stored twice" alone is satisfiable by
// DELETING prose; "every text reachable" alone is satisfiable by shipping three
// copies. Only the pair forbids both, which is what makes this a fold and not a diet.
// ---------------------------------------------------------------------------

/// A finding carrying a real message, the way a rule with a fixed prescription emits one.
fn finding_msg(rule: &str, severity: &str, idx: usize, message: &str) -> serde_json::Value {
    serde_json::json!({
        "ruleId": rule, "severity": severity, "file": format!("f{idx}.ts"),
        "line": idx, "message": message,
    })
}

/// Resolve one shaped finding back to its text, the way every consumer is told to: read the
/// `messageRef` FIELD, index the table, and fall back to the inline message when the field is
/// absent. Nothing here parses the human sentence — that is the whole point of the field existing,
/// and a test that scraped prose would have quietly re-frozen the wording this repo publishes as free.
fn resolve(shaped: &serde_json::Value, f: &serde_json::Value) -> String {
    if let Some(parts) = f.get("templateParts") {
        return splice(shaped, f, parts);
    }
    match f.get("messageRef").and_then(|v| v.as_str()) {
        None => f["message"]
            .as_str()
            .expect("a message key that is always a string")
            .to_string(),
        Some(k) => shaped["ruleMessages"][k]
            .as_str()
            .unwrap_or_else(|| panic!("messageRef {k:?} resolves to nothing: {shaped}"))
            .to_string(),
    }
}

/// The template half of the same contract, spelled the way the legend spells it: look the segments
/// up by the finding's OWN `ruleId`, then interleave SEGMENT FIRST, ending on the last segment.
/// Written out rather than shared with the producer on purpose — a test that called the shipping
/// splice would agree with it by construction, including where both are wrong.
fn splice(shaped: &serde_json::Value, f: &serde_json::Value, parts: &serde_json::Value) -> String {
    let rule = f["ruleId"]
        .as_str()
        .expect("a folded finding names its rule");
    let segs = shaped["ruleMessageTemplates"][rule]
        .as_array()
        .unwrap_or_else(|| panic!("no template for rule {rule:?}: {shaped}"));
    let parts = parts.as_array().expect("templateParts is an array");
    assert_eq!(
        parts.len() + 1,
        segs.len(),
        "templateParts must be exactly one shorter than its template: {f}"
    );
    let mut out = segs[0].as_str().expect("a segment is a string").to_string();
    for (i, p) in parts.iter().enumerate() {
        out.push_str(p.as_str().expect("a part is a string"));
        out.push_str(segs[i + 1].as_str().expect("a segment is a string"));
    }
    out
}

/// Every prose text the reply STORES: an inline message is stored where it sits, and a
/// `ruleMessages` value is stored in the table. A pointer stores nothing — it is an address, and
/// its length is bounded by the assertion below rather than counted as prose here.
fn stored_prose(shaped: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    for f in shaped["shown"].as_array().expect("shown is an array") {
        if f.get("messageRef").is_none() && f.get("templateParts").is_none() {
            out.push(f["message"].as_str().unwrap_or("").to_string());
        }
    }
    if let Some(map) = shaped["ruleMessages"].as_object() {
        for v in map.values() {
            out.push(v.as_str().unwrap_or("").to_string());
        }
    }
    // Template SEGMENTS are stored prose too — that is the point of storing them once. The empty
    // ones are skipped because an empty leading and an empty trailing segment are two absences, not
    // two copies of a text, and counting them would fail this pin on a reply storing nothing twice.
    if let Some(map) = shaped["ruleMessageTemplates"].as_object() {
        for segs in map.values() {
            for v in segs.as_array().into_iter().flatten() {
                match v.as_str() {
                    Some("") | None => {}
                    Some(s) => out.push(s.to_string()),
                }
            }
        }
    }
    out
}

/// The same reply with the fold UNDONE: every pointer resolved back inline, `messageRef` removed,
/// and the table plus its legend dropped. This is byte-for-byte the reply the shaper would have
/// emitted had it folded nothing, which is what makes it the honest comparand for "did folding
/// actually pay?" — the question the fold exists to answer yes to.
fn unfold(shaped: &serde_json::Value) -> serde_json::Value {
    let table = shaped.get("ruleMessages").cloned();
    let mut out = shaped.clone();
    for f in out["shown"].as_array_mut().expect("shown is an array") {
        if f.get("templateParts").is_some() {
            f["message"] = serde_json::Value::String(splice(shaped, f, &f["templateParts"]));
            f.as_object_mut()
                .expect("a finding is an object")
                .remove("templateParts");
            continue;
        }
        let Some(k) = f
            .get("messageRef")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            continue;
        };
        let text = table
            .as_ref()
            .and_then(|t| t.get(&k))
            .cloned()
            .unwrap_or_else(|| panic!("messageRef {k:?} resolves to nothing: {shaped}"));
        f["message"] = text;
        f.as_object_mut()
            .expect("a finding is an object")
            .remove("messageRef");
    }
    let o = out.as_object_mut().expect("a shaped block is an object");
    o.remove("ruleMessages");
    o.remove("ruleMessagesMeaning");
    o.remove("ruleMessageTemplates");
    o.remove("ruleMessageTemplatesMeaning");
    out
}

/// `(folded, unfolded)` sizes in BOTH serializers this repo ships — `analyze`/`cross` pretty-print
/// their reply (`analyze/mod.rs`, `cross.rs`) while the envelope/MCP lane emits compact
/// (`facade/analyze.rs`). Pretty printing charges an indent and a newline for every field the fold
/// ADDS and nothing for the bytes it removes, so the two lanes disagree about whether a marginal
/// fold pays; a pin that measured only one of them would vouch for half the shipped output.
fn fold_sizes(shaped: &serde_json::Value) -> [(usize, usize); 2] {
    let plain = unfold(shaped);
    [
        (
            serde_json::to_string_pretty(shaped).unwrap().len(),
            serde_json::to_string_pretty(&plain).unwrap().len(),
        ),
        (
            serde_json::to_string(shaped).unwrap().len(),
            serde_json::to_string(&plain).unwrap().len(),
        ),
    ]
}

/// DIRECTION 0 — THE REASON THE FOLD EXISTS, stated as the thing it must never do the opposite of:
/// a reply is never LARGER folded than it would have been unfolded. Repetition alone does not make
/// folding pay. A folded finding still carries a `message` (a pointer sentence, ~175 bytes), gains a
/// `messageRef` field, and the reply grows a table entry plus a one-per-reply legend — so a text
/// short enough is cheaper repeated inline, and a fold gated on "did this repeat?" instead of "does
/// this SAVE?" runs backwards on exactly the small and rule-filtered replies a reader reaches for
/// first. Measured on the shipped catalog: 45 of 118 rule messages sit below the n=2 break-even and
/// 44 of those interpolate nothing, so they emit byte-identically every time and would fold ALWAYS.
#[test]
fn folding_never_grows_a_reply_when_the_repeated_text_is_short() {
    // A real short prescription — `browser/no-document-write` is the shortest shipped message.
    let prose = "document.write() blocks the parser and silently wipes the document when it runs \
                 after load. Build the node and append it to a parent you already hold, or set \
                 textContent on an element that is already in the tree.";
    assert!(
        prose.len() < 300,
        "this fixture is only meaningful while the text is SHORT ({} bytes)",
        prose.len()
    );
    let findings: Vec<_> = (0..2)
        .map(|i| finding_msg("browser/no-document-write", "warning", i, prose))
        .collect();
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    for (folded, plain) in fold_sizes(&shaped) {
        assert!(
            folded <= plain,
            "the fold GREW this reply by {} bytes ({folded} folded vs {plain} unfolded) — folding a \
             short repeated text costs more than the copy it removes, so this text must have been \
             left inline: {shaped}",
            folded as i64 - plain as i64
        );
    }
}

/// The NON-ZERO control for the pin above, and the pin that stops "never grows" from being
/// satisfiable by never folding at all: a repeat long enough to pay must still SHRINK, strictly, in
/// both serializers, and must actually produce a table. Without this half the cheapest way to keep
/// every byte pin green is to delete the fold.
#[test]
fn folding_still_shrinks_a_reply_whose_repeated_text_is_long() {
    let prose = LONG_PROSE;
    let findings: Vec<_> = (0..4)
        .map(|i| finding_msg("security/hardcoded-secret", "critical", i, prose))
        .collect();
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert!(
        shaped.get("ruleMessages").is_some(),
        "a long text repeated four times must fold: {shaped}"
    );
    for (folded, plain) in fold_sizes(&shaped) {
        assert!(
            folded < plain,
            "the fold saved nothing here ({folded} folded vs {plain} unfolded) — a gate tuned until \
             it folds nothing passes every 'never grows' pin: {shaped}"
        );
    }
}

/// A prescription long enough that folding its repeats pays for the pointer, the `messageRef`
/// field, the table entry AND the one-per-reply legend. Its LENGTH is load-bearing now, which is
/// why it is a named constant every fold fixture shares: the median shipped rule message is 1,391
/// bytes, so this is what the fold's real population looks like, not an outlier chosen to pass.
const LONG_PROSE: &str = "\
Rotate this credential before you delete the line: the value is already in git history, so removing \
it from HEAD hides it without revoking it. Issue a replacement at the provider, deploy the \
replacement, confirm the new value is serving traffic, and only then revoke the old one — revoking \
first takes production down for as long as the deploy takes. Move the replacement out of the \
repository entirely: read it from the process environment at start-up, or from whatever secret \
store the deployment already has, and keep the name of the variable in the repository rather than \
its value. If this file is a committed example or fixture, the value is still a real credential to \
anyone who reads it; give the example an obviously fake value that cannot authenticate anywhere. \
This finding does not apply when the matched string is a placeholder the tooling substitutes at \
build time and the real value never reaches the repository — check the surrounding template before \
you rotate, because a rotation triggered by a placeholder costs an outage and fixes nothing.";

/// DIRECTION 1 — the structural goal itself: each distinct message text worth folding is stored
/// exactly once in a reply. Deliberately NOT a byte target and NOT a ratio: the measured message
/// share of a reply runs from 6% to 88% with the finding count, so a ratio would be met by choosing
/// a small tree. A text is either stored twice or it is not, and that stays true of every tree.
///
/// The population is now "texts the byte gate accepted", which is why the fixture prose is long:
/// a short repeat is deliberately left stored N times, and the pin that keeps THAT honest is
/// [`folding_never_grows_a_reply_when_the_repeated_text_is_short`] above.
#[test]
fn no_prose_text_is_stored_twice_in_one_reply() {
    let prose = LONG_PROSE;
    let findings: Vec<_> = (0..4)
        .map(|i| finding_msg("security/hardcoded-secret", "critical", i, prose))
        .collect();
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);

    let stored = stored_prose(&shaped);
    // NEGATIVE CONTROL: an empty subject set must be RED, never a silent pass.
    assert!(
        !stored.is_empty(),
        "no prose was read at all — this pin would vouch for nothing: {shaped}"
    );
    let mut seen: std::collections::HashMap<&String, usize> = Default::default();
    for m in &stored {
        *seen.entry(m).or_default() += 1;
    }
    let repeated: Vec<(String, usize)> = seen
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(m, n)| (m.chars().take(48).collect::<String>(), *n))
        .collect();
    assert!(
        repeated.is_empty(),
        "these message texts are STORED more than once in ONE reply: {repeated:?} — the reply must \
         carry one copy of each distinct text and let every finding point at it ({shaped})"
    );
    // A pointer cannot smuggle prose back in: it is an address, so it stays short and bounded.
    for f in shaped["shown"].as_array().unwrap() {
        let m = f["message"]
            .as_str()
            .expect("every finding keeps a string `message`");
        assert!(
            !m.is_empty(),
            "a message key must never become empty: {shaped}"
        );
        if f.get("messageRef").is_some() {
            assert!(
                m.len() < 256,
                "a pointer must stay an address, not prose ({} bytes): {m}",
                m.len()
            );
        }
    }
}

/// DIRECTION 2 — nothing was deleted: every finding's ORIGINAL message is reconstructible from this
/// reply alone, byte-identically, with no second request. This is the pin that stops DIRECTION 1
/// from being satisfiable by shortening prose, and it is the wire half of the requirement's own
/// retirement condition ("stop if shrinking removes an honesty the reader needed").
#[test]
fn every_folded_message_is_reconstructible_byte_identically_from_the_same_reply() {
    let prose = LONG_PROSE;
    let other = "This route mutates state with no auth evidence on its own handler.";
    let findings = vec![
        finding_msg("security/hardcoded-secret", "critical", 0, prose),
        finding_msg("security/hardcoded-secret", "critical", 1, prose),
        finding_msg("security/hardcoded-secret", "critical", 3, prose),
        finding_msg("mutating-route-no-auth", "warning", 2, other),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);

    let mut rebuilt: Vec<String> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| resolve(&shaped, f))
        .collect();
    let mut original = vec![
        prose.to_string(),
        prose.to_string(),
        prose.to_string(),
        other.to_string(),
    ];
    rebuilt.sort();
    original.sort();
    assert_eq!(
        rebuilt, original,
        "the reconstructed set must be byte-identical to the pre-fold set: {shaped}"
    );
    // The fold really happened here — otherwise this pin is green on a build that folds nothing.
    let table = shaped["ruleMessages"]
        .as_object()
        .expect("the repeated text must fold")
        .clone();
    assert_eq!(
        table.len(),
        1,
        "only the repeated text folds; the singleton stays inline: {shaped}"
    );
    // No orphan rows: every table entry is pointed at by at least one finding.
    for k in table.keys() {
        let pointed = shaped["shown"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.get("messageRef").and_then(|v| v.as_str()) == Some(k.as_str()));
        assert!(
            pointed,
            "ruleMessages[{k:?}] is an orphan — no finding points at it: {shaped}"
        );
    }
    // The table ships its legend, the contract `byRule`/`byRuleMeaning` already keeps: the pointer
    // stays an address only because the explanation rides once, here.
    let meaning = shaped["ruleMessagesMeaning"]
        .as_str()
        .unwrap_or_else(|| panic!("ruleMessages must ride beside its legend: {shaped}"));
    assert!(meaning.contains("byte-identical"), "{meaning}");
    assert!(meaning.contains("never truncation"), "{meaning}");
    // And the legend names the FIELD a consumer resolves through, not a sentence to scrape.
    assert!(meaning.contains("messageRef"), "{meaning}");
}

/// The NO-OP canary, and the reason singletons are left alone: a reply whose every message is unique
/// grows no table, no legend, no `messageRef` and no pointer. Without this the fold could cost bytes
/// on exactly the trees it does not help — a table entry plus a pointer where one inline string stood.
#[test]
fn a_reply_with_no_repeated_message_carries_no_fold_table() {
    let findings = vec![
        finding_msg(
            "unimported-export",
            "info",
            0,
            "`parseUser` is exported and imported nowhere.",
        ),
        finding_msg(
            "unimported-export",
            "info",
            1,
            "`parseOrder` is exported and imported nowhere.",
        ),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let shaped = shape(&findings, &filters);
    assert!(
        shaped.get("ruleMessages").is_none(),
        "nothing repeated — absent, not empty: {shaped}"
    );
    assert!(
        shaped.get("ruleMessagesMeaning").is_none(),
        "no table means no orphan legend: {shaped}"
    );
    assert_eq!(
        shaped["shown"][0]["message"], "`parseUser` is exported and imported nowhere.",
        "{shaped}"
    );
    assert_eq!(
        shaped["shown"][1]["message"], "`parseOrder` is exported and imported nowhere.",
        "{shaped}"
    );
    assert!(shaped["shown"][0].get("messageRef").is_none(), "{shaped}");
    // These two DO share a template ("`parse" + "` is exported and imported nowhere.") — and it is
    // worth far less than the pointer and the list it would cost, so the template lane must decline
    // it too. Without this line the negative control only covers half the fold.
    assert!(
        shaped.get("ruleMessageTemplates").is_none(),
        "a template worth less than its pointer must stay inline: {shaped}"
    );
    assert!(
        shaped.get("ruleMessageTemplatesMeaning").is_none(),
        "no table means no orphan legend: {shaped}"
    );
    assert!(
        shaped["shown"][0].get("templateParts").is_none(),
        "{shaped}"
    );
}

// ---------------------------------------------------------------------------
// THE TEMPLATE FOLD — the same two opposed pins, for the prose the exact fold structurally cannot
// reach. A rule that writes its finding's own subject into its prescription emits a different string
// every time, so keying on the whole message folds none of it; splitting the message into what every
// finding shares and what each one adds folds it WITHOUT deleting a per-finding fact. The claim that
// such prose "cannot be folded because folding erases per-finding facts" is what these pins refute:
// the erasure is a property of one key choice, not of the prose.
// ---------------------------------------------------------------------------

/// The five models here differ ONLY in an interpolated name, which is the shape the exact fold gives
/// up on. Four things are asserted together because any three of them are satisfiable by a cheat:
/// (a) it really templated — without it every byte pin below is green on a build that folds nothing;
/// (b) every original message rebuilds byte-identically from this reply alone; (c) no finding holds
/// two addresses for one text; (d) the reply actually got smaller, in both serializers.
#[test]
fn interpolated_messages_of_one_rule_fold_to_one_template_and_rebuild_byte_identically() {
    let msg = |model: &str| {
        format!(
            "Model `{model}` declares a column named like a foreign key with no declared relation. \
             {LONG_PROSE} Add the relation to `{model}`, or rename the column so it stops reading \
             as one."
        )
    };
    let models = ["User", "Booking", "EventType", "Membership", "Webhook"];
    let findings: Vec<_> = models
        .iter()
        .enumerate()
        .map(|(i, m)| finding_msg("schema/implicit-fk", "warning", i, &msg(m)))
        .collect();
    let shaped = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: None,
        },
    );

    // (a) IT ACTUALLY TEMPLATED.
    let table = shaped["ruleMessageTemplates"]
        .as_object()
        .unwrap_or_else(|| {
            panic!(
                "five messages differing only in a model name must fold to one template: {shaped}"
            )
        });
    assert_eq!(
        table.len(),
        1,
        "one rule, one template: {:?}",
        table.keys().collect::<Vec<_>>()
    );
    let segs = table["schema/implicit-fk"]
        .as_array()
        .expect("a template is an array of segments");
    assert!(
        segs.len() >= 2,
        "a template with no gap is the exact fold's population, not this one: {segs:?}"
    );
    // The segments hold only what every finding shares — the per-finding value is on the finding.
    for model in models {
        for s in segs {
            assert!(
                !s.as_str().expect("a segment is a string").contains(model),
                "a template segment carries the per-finding value {model:?}, so it is not shared: {s}"
            );
        }
    }
    // ... and it is on the finding, which is the half that makes this a fold and not a diet.
    assert!(
        shaped["shown"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| serde_json::to_string(&f["templateParts"])
                .unwrap()
                .contains("EventType")),
        "no finding kept its own subject: {shaped}"
    );

    // (b) NOTHING WAS DELETED.
    let mut rebuilt: Vec<String> = shaped["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| resolve(&shaped, f))
        .collect();
    let mut original: Vec<String> = models.iter().map(|m| msg(m)).collect();
    rebuilt.sort();
    original.sort();
    assert_eq!(
        rebuilt, original,
        "the rebuilt set must be byte-identical to the pre-fold set: {shaped}"
    );

    // (c) ONE ADDRESS PER FINDING.
    for f in shaped["shown"].as_array().unwrap() {
        assert!(f.get("templateParts").is_some(), "{f}");
        assert!(
            f.get("messageRef").is_none(),
            "a finding must not hold two addresses for one text: {f}"
        );
        let m = f["message"].as_str().expect("message stays a string");
        assert!(
            !m.is_empty() && m.len() < 320,
            "a pointer must stay an address, not prose ({} bytes): {m}",
            m.len()
        );
        assert!(
            m.contains("ruleMessageTemplates") && m.contains("templateParts"),
            "{m}"
        );
    }

    // (d) THE REPLY GOT SMALLER, in both serializers this repo ships.
    for (folded, plain) in fold_sizes(&shaped) {
        assert!(
            folded < plain,
            "the template fold saved nothing here ({folded} folded vs {plain} unfolded) — a gate \
             tuned until it folds nothing passes every 'never grows' pin: {shaped}"
        );
    }

    // (e) The legend rides with the table and states the rule a consumer implements.
    let meaning = shaped["ruleMessageTemplatesMeaning"]
        .as_str()
        .unwrap_or_else(|| panic!("the template table must ride beside its legend: {shaped}"));
    for token in [
        "templateParts",
        "ruleId",
        "byte-identical",
        "never truncation",
    ] {
        assert!(
            meaning.contains(token),
            "the legend never names {token:?}: {meaning}"
        );
    }
}

/// A rule whose two messages share only a few words must NOT template. The opposite of the pin
/// above and the reason the gate is bytes rather than "did these have anything in common": every
/// pair of English sentences shares SOMETHING, so a similarity gate would template every rule in
/// every reply and pay a pointer plus a list for each one.
#[test]
fn a_rule_whose_messages_barely_overlap_stays_inline() {
    let findings = vec![
        finding_msg("r", "info", 0, "Delete this file, or import it somewhere."),
        finding_msg(
            "r",
            "info",
            1,
            "This route mutates state with no auth on it.",
        ),
        finding_msg(
            "r",
            "info",
            2,
            "Bind this parameter instead of interpolating it.",
        ),
    ];
    let shaped = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: None,
        },
    );
    assert!(
        shaped.get("ruleMessageTemplates").is_none(),
        "these share almost nothing — templating them costs bytes: {shaped}"
    );
    for f in shaped["shown"].as_array().unwrap() {
        assert!(f.get("templateParts").is_none(), "{f}");
    }
    assert_eq!(
        shaped["shown"][0]["message"], "Delete this file, or import it somewhere.",
        "{shaped}"
    );
}

/// The fold judges the WIRE, not the full set: a text repeated across findings the cap dropped is
/// not repeated in the reply, so it must not fold. Pins that the fold runs AFTER `shown` is cut —
/// running it before would put a table entry in the reply for prose the reply does not contain, and
/// would point the one surviving finding away from its own text for no saving at all.
///
/// The prose is LONG and the same five findings are shaped a second time WITHOUT the cap, because
/// the byte gate gives this pin a second way to pass: a short text folds nowhere, capped or not, so
/// a short fixture here would be green whether or not the fold ever consults the window. The
/// control is what makes the capped arm mean "the cap did it".
#[test]
fn the_fold_judges_the_shown_window_not_the_full_finding_set() {
    let prose = LONG_PROSE;
    let findings: Vec<_> = (0..5).map(|i| finding_msg("r", "info", i, prose)).collect();
    let uncapped = shape(
        &findings,
        &FindingFilters {
            min_severity: None,
            rule: None,
            limit: None,
        },
    );
    assert!(
        uncapped.get("ruleMessages").is_some(),
        "CONTROL: these same five findings DO fold when the window holds them all — without this \
         the assertion below passes on a build that folds nothing: {uncapped}"
    );
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: Some(1),
    };
    let shaped = shape(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 1, "{shaped}");
    assert_eq!(
        shaped["shown"][0]["message"], prose,
        "one shown finding is not a repeat: {shaped}"
    );
    assert!(shaped.get("ruleMessages").is_none(), "{shaped}");
    // Counts are untouched by the fold in both directions — the contract every other shaping step
    // in this module keeps.
    assert_eq!(shaped["total"], 5, "{shaped}");
    assert_eq!(shaped["byRule"]["r"], 5, "{shaped}");
    assert_eq!(shaped["truncated"]["totalMatching"], 5, "{shaped}");
}
