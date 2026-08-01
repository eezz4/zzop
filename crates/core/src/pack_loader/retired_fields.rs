//! The retired-field census: pack fields that a serde-lenient parse DROPS ON THE FLOOR.
//!
//! `RuleDef` carries no `#[serde(deny_unknown_fields)]`, and that leniency IS the contract — a pack
//! written against a newer zzop must still load on an older one rather than hard-failing on a field it
//! has not learned yet. The cost of that contract is this defect class: a field the author believed in,
//! silently ignored, with a `valid: true` verdict on top of it. That is the same shape
//! `zzop_config`'s `RETIRED_KEYS` was written to abolish on the config side — "it was accepted, and
//! setting it did nothing" — and this module is its pack-side twin.
//!
//! Scope is deliberately NARROW: only fields zzop ITSELF once accepted and later retired are reported.
//! An unknown field zzop never had is NOT an issue here — that is the forward-compatibility the
//! leniency exists for, and flagging it would convert the contract into its opposite.
//!
//! Detection reads the RAW JSON, not the parsed `RulePackDef`, for the obvious reason: by the time
//! serde is done the evidence is gone. That is also why this is a separate pass from
//! [`super::rule_issues::pack_regex_issues`], which needs the typed pack.

use serde_json::Value;

/// Fields zzop once accepted on a rule and no longer reads, each with WHY it went and what to do — the
/// same two-part shape (`key`, `why`) `zzop_config::mapper::warnings::RETIRED_KEYS` uses, and for the
/// same reason: a retirement notice that does not say what replaced the field just moves the confusion.
///
/// Message style note: no backticks. These strings reach the same `issues` array as the loader's own
/// verdicts, which quote with `"` so the reference-validation contract's backtick scan does not read a
/// retired name as a claim that the knob still exists.
const RETIRED_RULE_FIELDS: &[(&str, &str)] = &[(
    "suppress_marker",
    "it was accepted as a per-rule inline ok-marker string, but the marker is now DERIVED from the \
     rule id as \"zzop-<id>-ok\" and nothing reads a stored one, so setting it changed no \
     suppression. Delete it — behaviour is unchanged — and write the derived marker in the source \
     comment instead. The CLI prints the derived spelling with \"zzop explain <rule-id>\"; an MCP \
     host has no explain tool, so read it off the rule id you already hold — the marker is \
     \"zzop-\" followed by that id followed by \"-ok\".",
)];

/// Every retired field present anywhere in `pack_json`'s rules, as one issue string each
/// (deterministic: rule order, then the order of [`RETIRED_RULE_FIELDS`]).
///
/// Takes the raw text rather than a parsed pack because the parse is what destroys the evidence. A
/// text that is not valid JSON yields no issues at all — the caller already reports the parse failure,
/// and a second complaint about fields inside unparseable JSON would be noise.
pub fn pack_retired_field_issues(pack_json: &str) -> Vec<String> {
    let Ok(root) = serde_json::from_str::<Value>(pack_json) else {
        return Vec::new();
    };
    let pack_id = root.get("id").and_then(Value::as_str).unwrap_or("<pack>");
    let Some(rules) = root.get("rules").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut issues = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        let Some(obj) = rule.as_object() else {
            continue;
        };
        // Pack-qualified for the same reason `pack_regex_issues` qualifies: two packs may each carry a
        // rule of the same id, and a bare id leaves no way to name the offender. A rule with no `id` is
        // located by index instead of skipped — it still has a retired field, and the caller's OTHER
        // issue (serde's "missing field `id`") does not name which rule.
        let rule_id = match obj.get("id").and_then(Value::as_str) {
            Some(id) => format!("{pack_id}/{id}"),
            None => format!("{pack_id}/<rule #{index}>"),
        };
        for (field, why) in RETIRED_RULE_FIELDS {
            if obj.contains_key(*field) {
                issues.push(format!(
                    "{rule_id}: \"{field}\" is a RETIRED field and is silently ignored — {why}"
                ));
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_with(rule_body: &str) -> String {
        format!(
            r#"{{"id":"p","schemaVersion":1,"rules":[{{"id":"r","severity":"warn",
               "message":"m","matcher":{{"kind":"LineScan","linePattern":"x"}}{rule_body}}}]}}"#
        )
    }

    #[test]
    fn a_retired_field_is_reported_with_its_rule_and_its_remedy() {
        let issues = pack_retired_field_issues(&pack_with(r#","suppress_marker":"my-marker""#));
        assert_eq!(issues.len(), 1, "{issues:?}");
        let one = &issues[0];
        assert!(one.starts_with("p/r: "), "pack-qualified rule id: {one}");
        assert!(one.contains("RETIRED"), "{one}");
        // The remedy must name the DERIVATION, not just say "removed" — a notice that only says the
        // field is gone leaves the author with no way to get the behaviour they wanted.
        assert!(
            one.contains("zzop-<id>-ok"),
            "names the derived form: {one}"
        );
        assert!(one.contains("zzop explain"), "names how to get it: {one}");
    }

    /// The leniency contract, pinned: an unknown field zzop NEVER had is forward compatibility, not a
    /// defect. Reporting it would convert `RuleDef`'s deliberate lack of `deny_unknown_fields` into an
    /// effective `deny_unknown_fields`, which is the opposite of the decision.
    #[test]
    fn an_unknown_field_that_was_never_ours_is_not_an_issue() {
        let issues = pack_retired_field_issues(&pack_with(r#","somethingFromANewerZzop":true"#));
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn a_clean_pack_reports_nothing() {
        assert!(pack_retired_field_issues(&pack_with("")).is_empty());
    }

    /// Unparseable input yields nothing rather than a guess — the caller already reports the parse
    /// failure, and two complaints about one cause read as two problems.
    #[test]
    fn unparseable_json_is_the_callers_problem_not_ours() {
        assert!(pack_retired_field_issues("{not json").is_empty());
        assert!(pack_retired_field_issues("[]").is_empty());
    }

    /// A rule missing its own `id` is still LOCATED. Serde's companion error says a field is missing
    /// but not which rule; without the index a pack author with 40 rules gets neither.
    #[test]
    fn a_rule_without_an_id_is_located_by_index() {
        let json = r#"{"id":"p","rules":[{"severity":"warn","suppress_marker":"x"}]}"#;
        let issues = pack_retired_field_issues(json);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].starts_with("p/<rule #0>: "), "{}", issues[0]);
    }

    /// Every entry must carry a remedy, not just a name. An entry that only says "removed" recreates
    /// the confusion this census exists to end.
    #[test]
    fn every_retired_entry_says_what_to_do_instead() {
        for (field, why) in RETIRED_RULE_FIELDS {
            assert!(
                why.contains("Delete it"),
                "{field}: the notice must tell the author what to DO: {why}"
            );
            assert!(
                !why.contains('`'),
                "{field}: no backticks — see this module's message style note"
            );
        }
    }
}
