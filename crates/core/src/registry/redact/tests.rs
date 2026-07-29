//! Coverage for evidence-path redaction — the "keep the finding, remove the path" half of `exclude`.

use super::{redact_excluded_evidence, REDACTED};
use crate::finding::Finding;
use crate::registry::{GlobalExclude, RuleConfig, Suppression};

fn finding(message: &str, evidence: &[&str], data: serde_json::Value) -> Finding {
    Finding {
        rule_id: "cross-layer/method-mismatch".to_string(),
        severity: crate::Severity::Warning,
        file: "fe/api.ts".to_string(),
        line: 12,
        message: message.to_string(),
        evidence_paths: evidence.iter().map(|p| p.to_string()).collect(),
        data: Some(data),
    }
}

fn excluding(path: &str) -> RuleConfig {
    RuleConfig {
        global_excludes: vec![GlobalExclude {
            path: Some(path.to_string()),
            glob: None,
        }],
        ..Default::default()
    }
}

/// The shape the field exists for: a finding anchored on the consume side that PRINTS a provide-side path
/// the reader excluded. Before this, that path sailed through and got printed anyway.
#[test]
fn an_excluded_evidence_path_is_replaced_everywhere_it_appears() {
    let mut f = finding(
        "GET /users is served by be/routes.ts:8 as PUT",
        &["be/routes.ts"],
        serde_json::json!({ "exampleProvide": { "file": "be/routes.ts", "line": 8 } }),
    );
    redact_excluded_evidence(&excluding("be/"), &mut f);
    assert_eq!(
        f.message,
        format!("GET /users is served by {REDACTED}:8 as PUT")
    );
    assert_eq!(f.data.as_ref().unwrap()["exampleProvide"]["file"], REDACTED);
    assert!(f.evidence_paths.is_empty());
}

/// The finding SURVIVES — that is the whole difference from the anchor role. A reader who excluded the
/// backend still learns their own call is wrong; they just do not get told where.
#[test]
fn the_finding_itself_is_untouched_apart_from_the_path() {
    let mut f = finding(
        "mismatch against be/routes.ts",
        &["be/routes.ts"],
        serde_json::json!({ "consumeKey": "GET /users" }),
    );
    redact_excluded_evidence(&excluding("be/"), &mut f);
    assert_eq!(f.file, "fe/api.ts");
    assert_eq!(f.line, 12);
    assert_eq!(f.rule_id, "cross-layer/method-mismatch");
    assert_eq!(f.data.as_ref().unwrap()["consumeKey"], "GET /users");
}

/// A composite string (`"source:file:line"`, the shape the N-source rules put in `sites`) must redact its
/// path PART. Whole-value matching would leave the excluded path sitting in plain sight.
#[test]
fn a_path_embedded_in_a_longer_string_is_redacted_in_place() {
    let mut f = finding(
        "all sites: be1:be/routes.ts:6, be2:legacy/old.ts:2",
        &["be/routes.ts", "legacy/old.ts"],
        serde_json::json!({ "sites": ["be1:be/routes.ts:6", "be2:legacy/old.ts:2"] }),
    );
    redact_excluded_evidence(&excluding("legacy/"), &mut f);
    assert_eq!(
        f.data.as_ref().unwrap()["sites"],
        serde_json::json!([
            "be1:be/routes.ts:6",
            format!("be2:{REDACTED}:2").as_str().to_string()
        ])
    );
    assert_eq!(f.evidence_paths, vec!["be/routes.ts".to_string()]);
}

/// Only the excluded path goes. A rule that names three places and one exclusion must still name two.
#[test]
fn unexcluded_evidence_paths_are_left_alone() {
    let mut f = finding(
        "a/x.ts, b/y.ts and c/z.ts all provide it",
        &["a/x.ts", "b/y.ts", "c/z.ts"],
        serde_json::json!({}),
    );
    redact_excluded_evidence(&excluding("b/"), &mut f);
    assert_eq!(
        f.message,
        format!("a/x.ts, {REDACTED} and c/z.ts all provide it")
    );
    assert_eq!(
        f.evidence_paths,
        vec!["a/x.ts".to_string(), "c/z.ts".to_string()]
    );
}

/// A `suppressions` entry is scoped to its rule, exactly as it is for the anchor — so one config key
/// means one thing whichever role the path plays.
#[test]
fn a_rule_scoped_suppression_redacts_only_its_own_rule() {
    let config = RuleConfig {
        suppressions: vec![Suppression {
            rule: "cross-layer/version-skew".to_string(),
            path: Some("be/".to_string()),
            glob: None,
        }],
        ..Default::default()
    };
    let mut f = finding(
        "names be/routes.ts",
        &["be/routes.ts"],
        serde_json::json!({}),
    );
    redact_excluded_evidence(&config, &mut f); // rule is method-mismatch, not version-skew
    assert_eq!(f.message, "names be/routes.ts");
    assert_eq!(f.evidence_paths, vec!["be/routes.ts".to_string()]);

    let mut g = Finding {
        rule_id: "cross-layer/version-skew".to_string(),
        ..finding(
            "names be/routes.ts",
            &["be/routes.ts"],
            serde_json::json!({}),
        )
    };
    redact_excluded_evidence(&config, &mut g);
    assert_eq!(g.message, format!("names {REDACTED}"));
}

/// A glob exclude must reach evidence too — the two filter spellings cannot mean different things in the
/// two roles.
#[test]
fn a_glob_exclude_redacts_evidence_the_same_way_a_substring_does() {
    let config = RuleConfig {
        global_excludes: vec![GlobalExclude {
            path: None,
            glob: Some("**/*.gen.ts".to_string()),
        }],
        ..Default::default()
    };
    let mut f = finding(
        "provided by sdk/client.gen.ts",
        &["sdk/client.gen.ts"],
        serde_json::json!({}),
    );
    redact_excluded_evidence(&config, &mut f);
    assert_eq!(f.message, format!("provided by {REDACTED}"));
}

/// The overwhelmingly common case — one place, nothing to redact — must not pay for the feature or change
/// shape because of it.
#[test]
fn a_finding_with_no_evidence_paths_is_untouched() {
    let mut f = Finding {
        evidence_paths: Vec::new(),
        ..finding("plain", &[], serde_json::json!({ "k": "fe/api.ts" }))
    };
    redact_excluded_evidence(&excluding("fe/"), &mut f);
    assert_eq!(f.message, "plain");
    // Not redacted despite matching: `fe/api.ts` is this finding's OWN anchor, and an anchor exclusion is
    // the merge layer's drop, never a redaction of the finding about itself.
    assert_eq!(f.data.as_ref().unwrap()["k"], "fe/api.ts");
}

/// Object keys are schema, not content. Rewriting one would produce a `data` shape no consumer can read —
/// worse than the leak it would be fixing.
#[test]
fn object_keys_are_never_redacted() {
    let mut f = finding(
        "x",
        &["be/routes.ts"],
        serde_json::json!({ "be/routes.ts": 3 }),
    );
    redact_excluded_evidence(&excluding("be/"), &mut f);
    assert_eq!(f.data.as_ref().unwrap()["be/routes.ts"], 3);
}
