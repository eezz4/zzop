use super::{unmatched_global_exclude_warnings, unmatched_suppression_warnings};
use crate::EngineConfig;
use zzop_core::{GlobalExclude, RuleConfig, Suppression};

fn config_with(suppressions: Vec<Suppression>) -> EngineConfig {
    EngineConfig {
        rule_config: RuleConfig {
            suppressions,
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    }
}

fn config_with_global_excludes(global_excludes: Vec<GlobalExclude>) -> EngineConfig {
    EngineConfig {
        rule_config: RuleConfig {
            global_excludes,
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    }
}

#[test]
fn segment_bound_glob_matching_nothing_warns_with_double_star_hint() {
    let config = config_with(vec![Suppression {
        rule: "browser/no-system-dialogs".to_string(),
        path: None,
        glob: Some("*.stories.tsx".to_string()),
    }]);
    let rels = ["src/a/x.stories.tsx"];
    let warnings = unmatched_suppression_warnings(&config, &rels);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("matched no files"));
    assert!(warnings[0].contains("**/*.stories.tsx"));
}

#[test]
fn segment_bound_glob_matching_a_root_file_warns_nothing() {
    let config = config_with(vec![Suppression {
        rule: "browser/no-system-dialogs".to_string(),
        path: None,
        glob: Some("*.stories.tsx".to_string()),
    }]);
    let rels = ["x.stories.tsx"];
    let warnings = unmatched_suppression_warnings(&config, &rels);
    assert!(warnings.is_empty());
}

#[test]
fn whole_rule_suppression_with_no_path_or_glob_is_never_flagged() {
    let config = config_with(vec![Suppression {
        rule: "r".to_string(),
        path: None,
        glob: None,
    }]);
    let rels = ["anything.ts"];
    let warnings = unmatched_suppression_warnings(&config, &rels);
    assert!(warnings.is_empty());
}

#[test]
fn plain_path_substring_matching_nothing_warns_without_double_star_hint() {
    let config = config_with(vec![Suppression {
        rule: "r".to_string(),
        path: Some("legacy/".to_string()),
        glob: None,
    }]);
    let rels = ["src/fresh/new.ts"];
    let warnings = unmatched_suppression_warnings(&config, &rels);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("matched no files"));
    assert!(!warnings[0].contains("**/"));
}

#[test]
fn global_exclude_segment_bound_glob_matching_nothing_warns_with_double_star_hint() {
    let config = config_with_global_excludes(vec![GlobalExclude {
        path: None,
        glob: Some("*.stories.tsx".to_string()),
    }]);
    let rels = ["src/a/x.stories.tsx"];
    let warnings = unmatched_global_exclude_warnings(&config, &rels);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("matched no files"));
    assert!(warnings[0].contains("**/*.stories.tsx"));
    // Worded as a top-level exclude, not tied to any rule id.
    assert!(!warnings[0].contains("rule"));
}

#[test]
fn global_exclude_segment_bound_glob_matching_a_root_file_warns_nothing() {
    let config = config_with_global_excludes(vec![GlobalExclude {
        path: None,
        glob: Some("*.stories.tsx".to_string()),
    }]);
    let rels = ["x.stories.tsx"];
    let warnings = unmatched_global_exclude_warnings(&config, &rels);
    assert!(warnings.is_empty());
}

#[test]
fn global_exclude_plain_path_substring_matching_nothing_warns_without_double_star_hint() {
    let config = config_with_global_excludes(vec![GlobalExclude {
        path: Some("legacy/".to_string()),
        glob: None,
    }]);
    let rels = ["src/fresh/new.ts"];
    let warnings = unmatched_global_exclude_warnings(&config, &rels);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("matched no files"));
    assert!(!warnings[0].contains("**/"));
}

#[test]
fn empty_global_excludes_warns_nothing() {
    let config = config_with_global_excludes(Vec::new());
    let rels = ["anything.ts"];
    let warnings = unmatched_global_exclude_warnings(&config, &rels);
    assert!(warnings.is_empty());
}
