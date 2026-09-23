//! Duplicate rule ids inside one pack — a defect the pre-load validator used to call `valid`.
//!
//! ## The defect this exists for (2026-09-13, review ledger V179)
//!
//! `zzop validate-rule-pack` answered `{"valid":true,"issues":[]}` for a pack whose first rule was
//! present twice under the same id. The control matters: the same validator rejects a genuinely
//! malformed pack (`missing field 'id'`), so the surface works — it simply had no opinion about this.
//!
//! What the duplicate costs is not "the rule cannot fire". Measured on a synthetic tree, BOTH copies
//! fire. The cost is that neither copy is ADDRESSABLE:
//!
//! - `disabledRules` and `severityOverrides` name a rule by id, so a config entry reaches both copies
//!   and cannot reach one.
//! - The suppress marker derives from the rule id alone (`suppress_marker_for_id`), so one vetted
//!   `// zzop-<id>-ok` comment silences both.
//!
//! The engine already says the second half out loud at analysis time — `suppress_marker_collisions`
//! emitted "derived by 2 loaded rules: \"typescript/no-explicit-any\", \"typescript/no-explicit-any\""
//! on the reproduction. So the project had already decided this shape is worth a warning; the gap was
//! that the surface whose entire job is answering BEFORE a load did not know it. A validator that
//! passes what the engine will complain about is not offline parity, and offline parity is the thing
//! `validate-rule-pack` sells.
//!
//! ## Why this is its own census rather than a third arm of `rule_issues`
//!
//! That module's doc states its two classes share "the same consequence and the same remedy" — a rule
//! that can never fire, fixed by repairing the matcher. This class has neither: both rules fire, and
//! the remedy is renaming an id. Folding it in would make that sentence false, and it is the sentence
//! that keeps the census coherent.

use std::collections::BTreeMap;

use crate::dsl::RulePackDef;

/// One issue per id that appears more than once in `pack`, ordered by id so the output is
/// deterministic regardless of rule order.
///
/// Pack-qualified for the same reason [`super::pack_regex_issues`] is: these strings can reach a
/// multi-pack context, where a bare id names no offender.
pub fn pack_duplicate_id_issues(pack: &RulePackDef) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for rule in &pack.rules {
        *counts.entry(rule.id.as_str()).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(id, n)| {
            let pack_id = &pack.id;
            format!(
                "rule \"{pack_id}/{id}\": declared {n} times in this pack — both copies FIRE, but \
                 neither can be addressed on its own. `disabledRules`/`severityOverrides` name a rule \
                 by id, and the suppress marker `zzop-{id}-ok` is derived from the id alone, so a \
                 config entry or a vetted suppression comment reaches every copy. Rename one."
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_dsl_pack;

    const PACK: &str = r#"{
      "id": "p",
      "dslSchemaVersion": 1,
      "rules": [
        { "id": "a", "severity": "info", "message": "m",
          "matcher": { "type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "x" } },
        { "id": "b", "severity": "info", "message": "m",
          "matcher": { "type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "y" } }
      ]
    }"#;

    /// FLOOR, in both directions. A census that cannot report a duplicate is indistinguishable from a
    /// pack that has none — the failure this whole module answers, one level down.
    #[test]
    fn a_duplicate_id_is_reported_and_a_clean_pack_is_not() {
        let clean = parse_dsl_pack(PACK).expect("fixture must parse");
        assert!(
            pack_duplicate_id_issues(&clean).is_empty(),
            "a pack with distinct ids must report nothing"
        );

        let dup_json = PACK.replace("\"id\": \"b\"", "\"id\": \"a\"");
        assert_ne!(
            dup_json, PACK,
            "the fixture edit must actually change the text"
        );
        let dup =
            parse_dsl_pack(&dup_json).expect("a duplicate id must still PARSE — that is the point");
        let issues = pack_duplicate_id_issues(&dup);
        assert_eq!(
            issues.len(),
            1,
            "one id duplicated once = one issue: {issues:?}"
        );
        assert!(
            issues[0].contains("\"p/a\""),
            "must name the pack-qualified id: {issues:?}"
        );
        assert!(
            issues[0].contains("declared 2 times"),
            "must say how many: {issues:?}"
        );
    }

    /// Three copies is one issue naming three, not two issues — a reader fixing this needs the count,
    /// and a per-extra-copy line would read as several unrelated defects.
    #[test]
    fn three_copies_report_once_with_the_count() {
        let dup_json = PACK.replace(
            "\"id\": \"b\"",
            "\"id\": \"a\", \"severity\": \"info\", \"message\": \"m\",\n          \"matcher\": { \"type\": \"line-scan\", \"file_pattern\": \"\\\\.ts$\", \"line_pattern\": \"z\" } },\n        { \"id\": \"a\"",
        );
        let dup = parse_dsl_pack(&dup_json).expect("fixture must parse");
        let issues = pack_duplicate_id_issues(&dup);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("declared 3 times"), "{issues:?}");
    }
}
