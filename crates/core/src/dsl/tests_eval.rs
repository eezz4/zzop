//! Pack-evaluation tests: the RegexSet pre-filter differential (pure optimization — must be
//! observationally identical) and the `eval_pack_profiled` rule-profiling substrate.

use super::eval::eval_pack_no_prefilter;
use super::test_support::{findings_as_json, method, pack};
use super::{eval_pack, eval_pack_profiled, RuleContext, RulePackDef, SourceFile};

// --- RegexSet multi-pattern pre-filter ---

#[test]
fn prefilter_matches_unoptimized_findings_across_the_moved_java_rules() {
    let files = vec![
        SourceFile {
            loop_spans: Vec::new(),
            rel: "C.java".into(),
            text: r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = '" + login + "'");"#
                .into(),
            symbols: vec![],
            io: None,
        },
        SourceFile {
            loop_spans: Vec::new(),
            rel: "D.java".into(),
            text: "MessageDigest md = MessageDigest.getInstance(\"MD5\");\nCipher.getInstance(\"DES/CBC/PKCS5Padding\");\n// legacy DigestUtils.md5DigestAsHex\n".into(),
            symbols: vec![],
            io: None,
        },
        SourceFile {
            loop_spans: Vec::new(),
            rel: "E.java".into(),
            text: "public class E { void noop() { System.out.println(\"nothing interesting\"); } }".into(),
            symbols: vec![],
            io: None,
        },
        SourceFile {
            loop_spans: Vec::new(),
            rel: "F.java".into(),
            text: "public class F {\n  void run() {\n    String[] cmd = { \"sh\", \"-c\", \"ping \" + host };\n    Runtime.getRuntime().exec(cmd);\n  }\n}".into(),
            symbols: vec![method("run", 2, 5)],
            io: None,
        },
    ];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let pack = pack();
    let optimized = eval_pack(&pack, &ctx);
    let unoptimized = eval_pack_no_prefilter(&pack, &ctx);
    assert!(!optimized.is_empty());
    assert_eq!(findings_as_json(&optimized), findings_as_json(&unoptimized));
}

#[test]
fn prefilter_respects_require_file_cheap_skip_semantics_unchanged() {
    let pack: RulePackDef = serde_json::from_str(
        r#"{
            "id": "t",
            "framework": "any",
            "rules": [
                {
                    "id": "r1",
                    "severity": "info",
                    "message": "m",
                    "matcher": {
                        "type": "line-scan",
                        "file_pattern": ".*",
                        "require_file": "NEEDLE",
                        "line_pattern": "foo"
                    }
                }
            ]
        }"#,
    )
    .unwrap();
    let files = vec![
        // RegexSet candidate (contains "foo") but require_file ("NEEDLE") is absent -> must stay skipped.
        SourceFile {
            loop_spans: Vec::new(),
            rel: "a.txt".into(),
            text: "foo bar".into(),
            symbols: vec![],
            io: None,
        },
        // RegexSet candidate AND require_file present -> must be flagged.
        SourceFile {
            loop_spans: Vec::new(),
            rel: "b.txt".into(),
            text: "foo NEEDLE".into(),
            symbols: vec![],
            io: None,
        },
    ];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let optimized = eval_pack(&pack, &ctx);
    let unoptimized = eval_pack_no_prefilter(&pack, &ctx);
    assert_eq!(findings_as_json(&optimized), findings_as_json(&unoptimized));
    assert_eq!(optimized.len(), 1);
    assert_eq!(optimized[0].file, "b.txt");
}

// --- eval_pack_profiled (rule profiling substrate) ---

#[test]
fn eval_pack_profiled_findings_match_eval_pack_exactly() {
    let files = vec![
        SourceFile {
            loop_spans: Vec::new(),
            rel: "C.java".into(),
            text: r#"Query q = em.createQuery("SELECT u FROM User u WHERE u.login = '" + login + "'");"#
                .into(),
            symbols: vec![],
            io: None,
        },
        SourceFile {
            loop_spans: Vec::new(),
            rel: "D.java".into(),
            text: "MessageDigest.getInstance(\"MD5\");\n".into(),
            symbols: vec![],
            io: None,
        },
    ];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let pack = pack();
    let plain = eval_pack(&pack, &ctx);
    let (profiled, timings) = eval_pack_profiled(&pack, &ctx);
    assert_eq!(findings_as_json(&plain), findings_as_json(&profiled));
    assert!(!plain.is_empty());

    assert_eq!(timings.len(), pack.rules.len());
    let ids: std::collections::HashSet<&str> = timings.iter().map(|t| t.rule_id.as_str()).collect();
    assert_eq!(ids.len(), timings.len(), "duplicate rule_id in timings");
    for t in &timings {
        assert!(t.rule_id.starts_with("be-security/"));
    }
    let total_findings: usize = timings.iter().map(|t| t.findings).sum();
    assert_eq!(total_findings, plain.len());
}

#[test]
fn eval_pack_profiled_on_empty_pack_yields_no_timings() {
    let pack = RulePackDef {
        id: "empty".into(),
        framework: "any".into(),
        schema_version: 1,
        fragments: Default::default(),
        rules: vec![],
    };
    let files = vec![];
    let ctx = RuleContext {
        files: &files,
        ir: None,
    };
    let (findings, timings) = eval_pack_profiled(&pack, &ctx);
    assert!(findings.is_empty());
    assert!(timings.is_empty());
}
