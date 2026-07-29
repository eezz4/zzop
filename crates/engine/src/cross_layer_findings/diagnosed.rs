//! Which consumes another cross-layer rule already explained — the input that keeps
//! `cross-layer/all-consumes-unjoined` from asserting "one unresolved base path" over a tree whose calls
//! each break in their own way. Split out of `mod.rs` to keep that file under the line-count ratchet.

use std::collections::BTreeSet;

use zzop_core::Finding;

/// Rules whose firing PROVES the join reached real provides for that consume: each compared the key
/// against the run's provide universe and reported how close it came. A consume one of these explained is
/// therefore not evidence of a dark join, and `cross-layer/all-consumes-unjoined` must not count it — see
/// that rule's `diagnosed` parameter for the benchmark failure that established this.
///
/// Deliberately excludes the two rules the fold REPLACES (`ambiguous-consume`,
/// `unprovided-mutation-call`): neither compares against anything, so counting them would make every tree
/// self-diagnosing and the fold could never fire. Also excludes the `unconsumed-*` family, which is
/// anchored on the PROVIDE side.
const DIAGNOSING_RULE_IDS: &[&str] = &[
    "cross-layer/method-mismatch",
    "cross-layer/version-skew",
    "cross-layer/path-near-miss",
    "cross-layer/route-near-miss",
    "cross-layer/prefix-drift",
];

/// The `(consumeSource, file, line)` anchor of every consume a [`DIAGNOSING_RULE_IDS`] rule already
/// explained, read back off the findings themselves rather than re-derived. Reading the OUTPUT is what
/// keeps this honest under gating: a rule the user disabled emits nothing, so its consumes correctly
/// become undiagnosed again and the fold covers them — exactly the `unconsumed_family` contract, where
/// disabling the specialization hands its subjects back to the general rule.
pub(super) fn consume_anchors(sources: &[Vec<Finding>]) -> BTreeSet<(String, String, u32)> {
    sources
        .iter()
        .flatten()
        .filter(|f| DIAGNOSING_RULE_IDS.contains(&f.rule_id.as_str()))
        .filter_map(|f| {
            let source = f
                .data
                .as_ref()
                .and_then(|d| d.get("consumeSource"))
                .and_then(|v| v.as_str())?;
            Some((source.to_string(), f.file.clone(), f.line))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::consume_anchors;
    use zzop_core::{Finding, Severity};

    fn finding(rule_id: &str, source: Option<&str>, file: &str, line: u32) -> Finding {
        Finding {
            rule_id: rule_id.to_string(),
            severity: Severity::Warning,
            file: file.to_string(),
            line,
            message: String::new(),
            evidence_paths: Vec::new(),
            data: source.map(|s| serde_json::json!({ "consumeSource": s })),
        }
    }

    #[test]
    fn only_diagnosing_rules_contribute_and_the_anchor_carries_its_tree() {
        let anchors = consume_anchors(&[
            vec![finding(
                "cross-layer/method-mismatch",
                Some("xfe"),
                "c.ts",
                5,
            )],
            vec![
                // Replaced by the fold, so it must never make its own tree look diagnosed.
                finding("cross-layer/ambiguous-consume", Some("xfe"), "c.ts", 11),
                // Provide-side family — anchored on the wrong side of the join entirely.
                finding("cross-layer/unconsumed-endpoint", Some("xbe"), "p.ts", 3),
            ],
            vec![finding("cross-layer/prefix-drift", Some("fe"), "a.ts", 18)],
        ]);
        assert_eq!(anchors.len(), 2);
        assert!(anchors.contains(&("xfe".to_string(), "c.ts".to_string(), 5)));
        assert!(anchors.contains(&("fe".to_string(), "a.ts".to_string(), 18)));
    }

    /// A diagnosing finding that carries no `consumeSource` cannot be attributed to a tree, and guessing
    /// one would discount an unrelated tree's call at the same relative path.
    #[test]
    fn a_diagnosing_finding_without_a_consume_source_contributes_nothing() {
        assert!(consume_anchors(&[vec![finding(
            "cross-layer/method-mismatch",
            None,
            "c.ts",
            5
        )]])
        .is_empty());
    }
}
