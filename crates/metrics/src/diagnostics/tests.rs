//! Exercises `build_diagnostics`: a healthy run produces no warnings, and each degenerate signal (zero
//! or sparse dep edges, zero extracted symbols, all-abstract modules, zero-commit/zero-change windows,
//! shallow history, untagged commit types) produces its own targeted warning without false-triggering
//! on healthy runs. Also covers the `git: Option<GitDiagnosticsInput>` gate itself: `None` must suppress
//! every git-window warning outright, regardless of the non-git counts.
use super::*;

fn healthy_git() -> GitDiagnosticsInput {
    GitDiagnosticsInput {
        total_changes: 500,
        tagged_changes: 320,
        fix_changes: 110,
        commits: 200,
        since: None,
    }
}

fn healthy() -> DiagnosticsInput {
    DiagnosticsInput {
        files: 100,
        dep_edges: 250,
        symbols: 400,
        concrete_modules: 80,
        total_modules: 100,
        git: Some(healthy_git()),
        unknown_disabled_rule_ids: Vec::new(),
        unknown_severity_override_ids: Vec::new(),
        unknown_suppression_rule_ids: Vec::new(),
    }
}

#[test]
fn a_healthy_run_produces_no_warnings_and_echoes_the_counts() {
    let d = build_diagnostics(healthy());
    assert!(d.warnings.is_empty());
    assert_eq!(d.files, 100);
    assert_eq!(d.git.as_ref().unwrap().fix_changes, 110);
}

#[test]
fn warns_when_there_are_files_but_zero_dependency_edges() {
    let d = build_diagnostics(DiagnosticsInput {
        dep_edges: 0,
        ..healthy()
    });
    assert!(d
        .warnings
        .iter()
        .any(|w| w.contains("0 internal dependency edges")));
}

#[test]
fn warns_when_dep_edges_are_pathologically_few_relative_to_files() {
    let d = build_diagnostics(DiagnosticsInput {
        files: 355,
        dep_edges: 1,
        ..healthy()
    });
    assert!(d
        .warnings
        .iter()
        .any(|w| w.contains("only 1 dependency edge(s)")));
}

#[test]
fn does_not_warn_on_a_healthy_edge_density() {
    let d = build_diagnostics(DiagnosticsInput {
        files: 100,
        dep_edges: 250,
        ..healthy()
    });
    assert!(!d.warnings.iter().any(|w| w.contains("dependency edge")));
}

#[test]
fn does_not_warn_about_0_dependency_edges_for_a_single_file_package() {
    let d = build_diagnostics(DiagnosticsInput {
        files: 1,
        dep_edges: 0,
        symbols: 1,
        total_modules: 1,
        concrete_modules: 1,
        ..healthy()
    });
    assert!(!d.warnings.iter().any(|w| w.contains("dependency edge")));
}

#[test]
fn warns_when_zero_symbols_were_extracted() {
    let d = build_diagnostics(DiagnosticsInput {
        symbols: 0,
        ..healthy()
    });
    assert!(d.warnings.iter().any(|w| w.contains("0 exported symbols")));
}

#[test]
fn zero_symbols_warning_uses_the_dual_possibility_phrasing() {
    let d = build_diagnostics(DiagnosticsInput {
        symbols: 0,
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|w| w.contains("0 exported symbols"))
        .expect("expected a 0-exported-symbols warning");
    assert!(w.contains("EITHER the tree genuinely exports nothing"));
    assert!(w.contains("OR export/symbol detection failed for this module system"));
}

#[test]
fn warns_when_every_module_is_classified_abstract() {
    let d = build_diagnostics(DiagnosticsInput {
        concrete_modules: 0,
        ..healthy()
    });
    assert!(d.warnings.iter().any(|w| w.contains("abstract")));
}

#[test]
fn warns_when_commits_exist_but_none_were_classified_by_type() {
    let d = build_diagnostics(DiagnosticsInput {
        git: Some(GitDiagnosticsInput {
            tagged_changes: 0,
            ..healthy_git()
        }),
        ..healthy()
    });
    assert!(d
        .warnings
        .iter()
        .any(|w| w.contains("classified by commit type")));
}

#[test]
fn with_commits_in_the_window_but_zero_changes_pathspec_submodule_message() {
    let d = build_diagnostics(DiagnosticsInput {
        git: Some(GitDiagnosticsInput {
            commits: 200,
            total_changes: 0,
            tagged_changes: 0,
            fix_changes: 0,
            ..healthy_git()
        }),
        ..healthy()
    });
    assert!(d
        .warnings
        .iter()
        .any(|w| w.contains("0 git changes") && w.contains("despite 200 commit")));
    assert!(!d
        .warnings
        .iter()
        .any(|w| w.contains("classified by commit type")));
}

#[test]
fn with_zero_commits_in_a_bounded_window_names_the_narrow_since_window() {
    let d = build_diagnostics(DiagnosticsInput {
        git: Some(GitDiagnosticsInput {
            commits: 0,
            total_changes: 0,
            tagged_changes: 0,
            fix_changes: 0,
            since: Some("1.year".to_string()),
        }),
        ..healthy()
    });
    assert!(d
        .warnings
        .iter()
        .any(|w| w.contains("0 commits in the analyzed window") && w.contains("git.since")));
    assert!(!d.warnings.iter().any(|w| w.contains("pathspec")));
}

#[test]
fn with_zero_commits_over_full_history_submodule_untracked() {
    let d = build_diagnostics(DiagnosticsInput {
        git: Some(GitDiagnosticsInput {
            commits: 0,
            total_changes: 0,
            tagged_changes: 0,
            fix_changes: 0,
            since: None,
        }),
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|x| x.contains("0 commits touch these files"));
    assert!(w.is_some());
    let w = w.unwrap();
    assert!(w.contains("submodule"));
    assert!(w.contains("will NOT help"));
}

#[test]
fn warns_when_the_window_has_at_most_1_commit_but_changes_exist() {
    let d = build_diagnostics(DiagnosticsInput {
        files: 355,
        git: Some(GitDiagnosticsInput {
            commits: 1,
            total_changes: 355,
            tagged_changes: 355,
            fix_changes: 0,
            ..healthy_git()
        }),
        ..healthy()
    });
    assert!(d
        .warnings
        .iter()
        .any(|w| w.contains("only 1 commit(s)") && w.contains("shallow clone")));
    // mutually exclusive with the "0 git changes" warning (that one needs total_changes == 0)
    assert!(!d.warnings.iter().any(|w| w.contains("0 git changes")));
}

#[test]
fn does_not_warn_about_shallow_history_on_a_healthy_multi_commit_window() {
    let d = build_diagnostics(healthy());
    assert!(!d.warnings.iter().any(|w| w.contains("shallow clone")));
}

#[test]
fn does_not_warn_about_empty_signals_on_an_empty_repo() {
    let d = build_diagnostics(DiagnosticsInput {
        files: 0,
        dep_edges: 0,
        symbols: 0,
        concrete_modules: 0,
        total_modules: 0,
        git: None,
        unknown_disabled_rule_ids: Vec::new(),
        unknown_severity_override_ids: Vec::new(),
        unknown_suppression_rule_ids: Vec::new(),
    });
    assert!(d.warnings.is_empty());
}

#[test]
fn git_none_suppresses_every_git_window_warning_even_with_pathological_file_counts() {
    // `git: None` means git was never attempted for this run — the module must not emit any
    // git-window warning no matter how the non-git counts look, since there is no honest count to
    // report zero of.
    let d = build_diagnostics(DiagnosticsInput {
        files: 355,
        dep_edges: 900,
        symbols: 400,
        concrete_modules: 80,
        total_modules: 100,
        git: None,
        unknown_disabled_rule_ids: Vec::new(),
        unknown_severity_override_ids: Vec::new(),
        unknown_suppression_rule_ids: Vec::new(),
    });
    assert!(!d
        .warnings
        .iter()
        .any(|w| w.contains("commit") || w.contains("git changes") || w.contains("submodule")));
}

#[test]
fn warns_when_a_disabled_rules_entry_matches_no_known_id() {
    let d = build_diagnostics(DiagnosticsInput {
        unknown_disabled_rule_ids: vec!["typescript/as-cast-typo".to_string()],
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|w| w.contains("matching no known rule id"))
        .expect("expected an unknown-disabled-rule-id warning");
    assert!(w.contains("typescript/as-cast-typo"));
    assert!(w.contains("did NOT disable anything"));
}

#[test]
fn does_not_warn_about_unknown_disabled_rules_when_the_list_is_empty() {
    let d = build_diagnostics(healthy());
    assert!(!d
        .warnings
        .iter()
        .any(|w| w.contains("matching no known rule id")));
}

#[test]
fn unknown_disabled_rule_ids_are_sorted_and_deduplicated_in_the_warning() {
    let d = build_diagnostics(DiagnosticsInput {
        unknown_disabled_rule_ids: vec![
            "z-pack/typo".to_string(),
            "a-pack/typo".to_string(),
            "a-pack/typo".to_string(),
        ],
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|w| w.contains("matching no known rule id"))
        .expect("expected an unknown-disabled-rule-id warning");
    assert!(w.contains("2 entry/entries"));
    assert!(w.contains("a-pack/typo, z-pack/typo"));
}

#[test]
fn warns_when_a_severity_override_entry_matches_no_known_id() {
    let d = build_diagnostics(DiagnosticsInput {
        unknown_severity_override_ids: vec!["n-plus-one".to_string()],
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|w| w.contains("matching no known rule id") && w.contains("severityOverrides"))
        .expect("expected an unknown-severity-override-id warning");
    assert!(w.contains("n-plus-one"));
    assert!(w.contains("did NOT remap"));
}

#[test]
fn does_not_warn_about_unknown_severity_overrides_when_the_list_is_empty() {
    let d = build_diagnostics(healthy());
    assert!(!d
        .warnings
        .iter()
        .any(|w| w.contains("severityOverrides") && w.contains("matching no known rule id")));
}

#[test]
fn unknown_severity_override_ids_are_sorted_and_deduplicated_in_the_warning() {
    let d = build_diagnostics(DiagnosticsInput {
        unknown_severity_override_ids: vec![
            "z-pack/typo".to_string(),
            "a-pack/typo".to_string(),
            "a-pack/typo".to_string(),
        ],
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|w| w.contains("severityOverrides") && w.contains("matching no known rule id"))
        .expect("expected an unknown-severity-override-id warning");
    assert!(w.contains("2 entry/entries"));
    assert!(w.contains("a-pack/typo, z-pack/typo"));
}

#[test]
fn warns_when_a_suppression_rule_id_matches_no_known_id() {
    let d = build_diagnostics(DiagnosticsInput {
        unknown_suppression_rule_ids: vec!["n-plus-one".to_string()],
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|w| w.contains("suppressions have") && w.contains("matches no known rule id"))
        .expect("expected an unknown-suppression-rule-id warning");
    assert!(w.contains("n-plus-one"));
    assert!(w.contains("did NOT suppress anything"));
}

#[test]
fn does_not_warn_about_unknown_suppression_rule_ids_when_the_list_is_empty() {
    let d = build_diagnostics(healthy());
    assert!(!d
        .warnings
        .iter()
        .any(|w| w.contains("suppressions have") && w.contains("matches no known rule id")));
}

#[test]
fn unknown_suppression_rule_ids_are_sorted_and_deduplicated_in_the_warning() {
    let d = build_diagnostics(DiagnosticsInput {
        unknown_suppression_rule_ids: vec![
            "z-pack/typo".to_string(),
            "a-pack/typo".to_string(),
            "a-pack/typo".to_string(),
        ],
        ..healthy()
    });
    let w = d
        .warnings
        .iter()
        .find(|w| w.contains("suppressions have") && w.contains("matches no known rule id"))
        .expect("expected an unknown-suppression-rule-id warning");
    assert!(w.contains("2 entry/entries"));
    assert!(w.contains("a-pack/typo, z-pack/typo"));
}
