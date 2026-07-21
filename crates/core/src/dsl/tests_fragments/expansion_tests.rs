//! Synthetic-pack unit coverage of the `${NAME}` fragment resolution/error contract (see the
//! `tests_fragments` module doc). Every pack is hand-built — `RulePackDef::expand_fragments` in isolation.

use super::super::def::{
    IoDirection, IoScan, LabeledPattern, LineScan, Matcher, MethodScan, RuleDef, RulePackDef,
    SymbolScan,
};
use super::super::fragments::{fragment_ref_name, FragmentError};
use crate::Severity;

fn line_scan_rule(id: &str, file_exclude_pattern: Option<&str>) -> RuleDef {
    RuleDef {
        id: id.to_string(),
        severity: Severity::Info,
        message: "m".to_string(),
        matcher: Matcher::LineScan(LineScan {
            file_pattern: "(?i)\\.ts$".to_string(),
            require_file: None,
            require_file_all: vec![],
            require_file_absent: vec![],
            skip_comment_lines: false,
            strip_string_literals: false,
            line_pattern: Some("TODO".to_string()),
            any: None,
            exclude_pattern: None,
            file_exclude_pattern: file_exclude_pattern.map(str::to_string),
            snippet_max: 160,
        }),
        suppress_marker: None,
    }
}

fn minimal_pack(
    fragments: std::collections::BTreeMap<String, String>,
    rules: Vec<RuleDef>,
) -> RulePackDef {
    RulePackDef {
        id: "p".to_string(),
        framework: "any".to_string(),
        schema_version: 1,
        fragments,
        rules,
    }
}

// --- `fragment_ref_name` ---

#[test]
fn fragment_ref_name_matches_only_a_whole_value_dollar_brace_name_brace() {
    assert_eq!(fragment_ref_name("${foo}"), Some("foo"));
    assert_eq!(fragment_ref_name("${test-paths}"), Some("test-paths"));
    assert_eq!(fragment_ref_name("foo ${bar} baz"), None); // substring, not whole-value
    assert_eq!(fragment_ref_name("${}"), None); // empty name
    assert_eq!(fragment_ref_name("(?i)\\.ts$"), None); // an ordinary regex
    assert_eq!(fragment_ref_name("${foo"), None); // unterminated
}

// --- `expand_fragments`: resolution ---

#[test]
fn expand_fragments_resolves_a_shared_bundled_fragment() {
    let mut pack = minimal_pack(
        Default::default(),
        vec![line_scan_rule("r1", Some("${test-paths}"))],
    );
    pack.expand_fragments()
        .expect("shared fragment must resolve");
    let Matcher::LineScan(m) = &pack.rules[0].matcher else {
        unreachable!()
    };
    assert_eq!(
        m.file_exclude_pattern.as_deref(),
        Some(
            "(?i)((^|/)(e2e|tests?|__tests?__|spec|fixtures?|testing)/|\\.(test|spec)\\.|[.-]spec\\.|\
             (^|/)(playwright|vitest|jest|cypress|vite)\\.config\\.)"
        )
    );
    assert!(
        pack.fragments.is_empty(),
        "fragments must be cleared after expansion"
    );
}

#[test]
fn expand_fragments_lets_a_per_pack_fragment_override_a_shared_name_of_the_same_name() {
    let mut fragments = std::collections::BTreeMap::new();
    fragments.insert("test-paths".to_string(), "(?i)custom-override".to_string());
    let mut pack = minimal_pack(fragments, vec![line_scan_rule("r1", Some("${test-paths}"))]);
    pack.expand_fragments().expect("override must resolve");
    let Matcher::LineScan(m) = &pack.rules[0].matcher else {
        unreachable!()
    };
    assert_eq!(
        m.file_exclude_pattern.as_deref(),
        Some("(?i)custom-override")
    );
}

#[test]
fn expand_fragments_resolves_a_pack_local_fragment_absent_from_the_shared_set() {
    let mut fragments = std::collections::BTreeMap::new();
    fragments.insert("only-mine".to_string(), "(?i)local-only".to_string());
    let mut pack = minimal_pack(fragments, vec![line_scan_rule("r1", Some("${only-mine}"))]);
    pack.expand_fragments()
        .expect("pack-local fragment must resolve");
    let Matcher::LineScan(m) = &pack.rules[0].matcher else {
        unreachable!()
    };
    assert_eq!(m.file_exclude_pattern.as_deref(), Some("(?i)local-only"));
}

#[test]
fn expand_fragments_leaves_an_ordinary_pattern_untouched() {
    let mut pack = minimal_pack(Default::default(), vec![line_scan_rule("r1", None)]);
    let before = format!("{pack:?}");
    pack.expand_fragments().expect("nothing to resolve");
    assert_eq!(format!("{pack:?}"), before);
}

#[test]
fn expand_fragments_errs_on_an_unknown_fragment_name() {
    let mut pack = minimal_pack(
        Default::default(),
        vec![line_scan_rule("r1", Some("${does-not-exist}"))],
    );
    let err = pack.expand_fragments().unwrap_err();
    assert_eq!(
        err,
        FragmentError::Unknown {
            rule: "r1".to_string(),
            field: "file_exclude_pattern".to_string(),
            name: "does-not-exist".to_string(),
        }
    );
    assert!(err.to_string().contains("unknown fragment"));
    assert!(err.to_string().contains("does-not-exist"));
}

#[test]
fn expand_fragments_errs_on_a_nested_fragment_reference_rather_than_chaining() {
    let mut fragments = std::collections::BTreeMap::new();
    fragments.insert("inner".to_string(), "${test-paths}".to_string()); // itself a whole-value ref
    fragments.insert("outer".to_string(), "${inner}".to_string());
    let mut pack = minimal_pack(fragments, vec![line_scan_rule("r1", Some("${outer}"))]);
    let err = pack.expand_fragments().unwrap_err();
    assert_eq!(
        err,
        FragmentError::Nested {
            rule: "r1".to_string(),
            field: "file_exclude_pattern".to_string(),
            name: "outer".to_string(),
        }
    );
    assert!(err.to_string().contains("nested"));
}

#[test]
fn expand_fragments_is_idempotent_on_an_already_expanded_pack() {
    let mut pack = minimal_pack(
        Default::default(),
        vec![line_scan_rule("r1", Some("${test-paths}"))],
    );
    pack.expand_fragments().unwrap();
    let once = format!("{pack:?}");
    pack.expand_fragments().unwrap(); // fragments already empty, values already literal — must be a no-op
    assert_eq!(format!("{pack:?}"), once);
}

/// Every pattern-bearing field the task names — `file_pattern`, `require_file`, `require_file_all`,
/// `require_file_absent`, `line_pattern`, `any[].pattern`, `exclude_pattern`, `file_exclude_pattern`
/// (line-scan); `patterns[].pattern`/`absent[].pattern`/`file_exclude_pattern` (method-scan);
/// `name_pattern` (symbol-scan); `key_pattern` (io-scan) — resolves a `${NAME}` ref. One pack exercising
/// every field at once, each pointed at its own fragment name, so a future field added to a matcher
/// without wiring it into `expand_fragments` shows up here as an unresolved `${...}` left in place
/// (caught by the sentinel-collision guard below on ANY pack, not just shipped ones, if this test's own
/// pack were scanned by it — it deliberately is not, since it's a synthetic fixture, not a shipped pack).
#[test]
fn expand_fragments_covers_every_pattern_bearing_field_on_every_matcher_kind() {
    let mut fragments = std::collections::BTreeMap::new();
    for name in [
        "file-pattern",
        "require-file",
        "require-file-all",
        "require-file-absent",
        "line-pattern",
        "any-pattern",
        "exclude-pattern",
        "file-exclude-pattern",
        "patterns-pattern",
        "absent-pattern",
        "name-pattern",
        "key-pattern",
    ] {
        fragments.insert(name.to_string(), format!("(?i){name}-resolved"));
    }

    let line_scan_rule = RuleDef {
        id: "ls".to_string(),
        severity: Severity::Info,
        message: "m".to_string(),
        matcher: Matcher::LineScan(LineScan {
            file_pattern: "${file-pattern}".to_string(),
            require_file: Some("${require-file}".to_string()),
            require_file_all: vec!["${require-file-all}".to_string()],
            require_file_absent: vec!["${require-file-absent}".to_string()],
            skip_comment_lines: false,
            strip_string_literals: false,
            line_pattern: Some("${line-pattern}".to_string()),
            any: Some(vec![LabeledPattern {
                pattern: "${any-pattern}".to_string(),
                label: "l".to_string(),
            }]),
            exclude_pattern: Some("${exclude-pattern}".to_string()),
            file_exclude_pattern: Some("${file-exclude-pattern}".to_string()),
            snippet_max: 160,
        }),
        suppress_marker: None,
    };
    let method_scan_rule = RuleDef {
        id: "ms".to_string(),
        severity: Severity::Info,
        message: "m".to_string(),
        matcher: Matcher::MethodScan(MethodScan {
            file_pattern: "${file-pattern}".to_string(),
            require_file: Some("${require-file}".to_string()),
            require_file_all: vec!["${require-file-all}".to_string()],
            require_file_absent: vec!["${require-file-absent}".to_string()],
            skip_comment_lines: false,
            strip_string_literals: false,
            patterns: vec![LabeledPattern {
                pattern: "${patterns-pattern}".to_string(),
                label: "t".to_string(),
            }],
            trigger: "t".to_string(),
            trigger_in_loop: false,
            absent: vec![LabeledPattern {
                pattern: "${absent-pattern}".to_string(),
                label: "a".to_string(),
            }],
            file_exclude_pattern: Some("${file-exclude-pattern}".to_string()),
            snippet_max: 160,
        }),
        suppress_marker: None,
    };
    let symbol_scan_rule = RuleDef {
        id: "ss".to_string(),
        severity: Severity::Info,
        message: "m".to_string(),
        matcher: Matcher::SymbolScan(SymbolScan {
            file_pattern: "${file-pattern}".to_string(),
            kind: None,
            name_pattern: Some("${name-pattern}".to_string()),
            exported: None,
            negate: false,
        }),
        suppress_marker: None,
    };
    let io_scan_rule = RuleDef {
        id: "is".to_string(),
        severity: Severity::Info,
        message: "m".to_string(),
        matcher: Matcher::IoScan(IoScan {
            file_pattern: "${file-pattern}".to_string(),
            direction: IoDirection::Any,
            kind: None,
            key_pattern: Some("${key-pattern}".to_string()),
            negate: false,
        }),
        suppress_marker: None,
    };

    let mut pack = minimal_pack(
        fragments,
        vec![
            line_scan_rule,
            method_scan_rule,
            symbol_scan_rule,
            io_scan_rule,
        ],
    );
    pack.expand_fragments().expect("every field must resolve");

    let Matcher::LineScan(ls) = &pack.rules[0].matcher else {
        unreachable!()
    };
    assert_eq!(ls.file_pattern, "(?i)file-pattern-resolved");
    assert_eq!(
        ls.require_file.as_deref(),
        Some("(?i)require-file-resolved")
    );
    assert_eq!(
        ls.require_file_all,
        vec!["(?i)require-file-all-resolved".to_string()]
    );
    assert_eq!(
        ls.require_file_absent,
        vec!["(?i)require-file-absent-resolved".to_string()]
    );
    assert_eq!(
        ls.line_pattern.as_deref(),
        Some("(?i)line-pattern-resolved")
    );
    assert_eq!(
        ls.any.as_ref().unwrap()[0].pattern,
        "(?i)any-pattern-resolved"
    );
    assert_eq!(
        ls.exclude_pattern.as_deref(),
        Some("(?i)exclude-pattern-resolved")
    );
    assert_eq!(
        ls.file_exclude_pattern.as_deref(),
        Some("(?i)file-exclude-pattern-resolved")
    );

    let Matcher::MethodScan(ms) = &pack.rules[1].matcher else {
        unreachable!()
    };
    assert_eq!(ms.patterns[0].pattern, "(?i)patterns-pattern-resolved");
    assert_eq!(ms.absent[0].pattern, "(?i)absent-pattern-resolved");

    let Matcher::SymbolScan(ss) = &pack.rules[2].matcher else {
        unreachable!()
    };
    assert_eq!(
        ss.name_pattern.as_deref(),
        Some("(?i)name-pattern-resolved")
    );

    let Matcher::IoScan(io) = &pack.rules[3].matcher else {
        unreachable!()
    };
    assert_eq!(io.key_pattern.as_deref(), Some("(?i)key-pattern-resolved"));
}
