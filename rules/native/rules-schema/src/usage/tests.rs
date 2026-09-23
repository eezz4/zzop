//! Unit tests for the usage-evidence collectors, the usage-aware cross-check, the churn rule, and
//! `analyze_schema_with_usage`'s composition of structural + usage signals.
use super::*;
use zzop_core::{Attribute, EntityRef, FieldAttr, SchemaField};

/// `cross_check_schema` under the BUILT-IN skip-field vocabulary — every case below is about the usage
/// cross-check's logic, not about which field names a project treats as boilerplate.
fn cross_check_default(
    models: &[SchemaModel],
    usage: &SchemaUsage,
    attrs: &AttributeStore,
) -> Vec<SchemaIssue> {
    cross_check_schema(models, usage, attrs, SKIP_FIELD_NAMES)
}

// --- fieldUsageTokens ---

#[test]
fn field_usage_tokens_collects_identifiers_from_one_file() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "export function getPostTitle(post: any) {\n  return post.title;\n}\n",
    );
    assert!(result.contains("title"));
    assert!(result.contains("post"));
}

#[test]
fn field_usage_tokens_dead_field_absent_when_never_referenced() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "export function f(post: any) { return post.title; }\n",
    );
    assert!(!result.contains("deadField"));
}

#[test]
fn field_usage_tokens_empty_for_a_d_ts_file() {
    let result = field_usage_tokens(
        "src/types/generated.d.ts",
        "export interface Generated { declarationOnlyFieldDEF: string; }\n",
    );
    assert!(result.is_empty());
}

#[test]
fn field_usage_tokens_empty_for_a_js_file() {
    let result = field_usage_tokens(
        "src/domains/post/routes/helper.js",
        "const jsOnlyFieldGHI = 1; module.exports = { jsOnlyFieldGHI };\n",
    );
    assert!(result.is_empty());
}

#[test]
fn field_usage_tokens_excludes_identifiers_inside_comments() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "// commentOnlyFieldJKL: this is a comment\n/* also commentOnlyFieldJKL */\nexport function f() { return 1; }\n",
    );
    assert!(!result.contains("commentOnlyFieldJKL"));
}

#[test]
fn field_usage_tokens_excludes_identifiers_inside_string_literals() {
    let result = field_usage_tokens(
        "src/domains/post/routes/createPostHandlers.ts",
        "export function f() {\n  const s = \"stringOnlyFieldMNO\";\n  const t = 'stringOnlyFieldMNO';\n  return s + t;\n}\n",
    );
    assert!(!result.contains("stringOnlyFieldMNO"));
}

#[test]
fn field_usage_tokens_tsx_file_also_scanned() {
    let result = field_usage_tokens(
        "src/domains/post/PostCard.tsx",
        "export function PostCard(post: any) { return post.title; }\n",
    );
    assert!(result.contains("title"));
}

// --- crossCheckSchema ---

fn field(name: &str) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        r#type: "String".to_string(),
        optional: false,
        list: false,
        attrs: vec![],
    }
}

fn model(name: &str, field_names: &[&str]) -> SchemaModel {
    SchemaModel {
        name: name.to_string(),
        fields: field_names.iter().map(|n| field(n)).collect(),
        ..Default::default()
    }
}

fn usage(identifiers: &[(&str, u32)]) -> SchemaUsage {
    SchemaUsage {
        identifier_counts: identifiers
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
    }
}

/// An `AttributeStore` asserting a truthy `BOUND_MODEL_ATTR` on each name-only model `Symbol` —
/// the injection replacement for the removed `SchemaUsage.bound_models` set.
fn bound_attrs(names: &[&str]) -> AttributeStore {
    AttributeStore::from_attrs(
        names
            .iter()
            .map(|n| Attribute {
                target: EntityRef::Symbol {
                    name: n.to_string(),
                    file: None,
                },
                key: BOUND_MODEL_ATTR.to_string(),
                value: serde_json::json!(true),
            })
            .collect(),
    )
}

/// An `AttributeStore` carrying `MODEL_CHURN_ATTR` counts per name-only model `Symbol` — the
/// injection replacement for the removed `SchemaUsage.model_churn` map.
fn churn_attrs(pairs: &[(&str, u32)]) -> AttributeStore {
    AttributeStore::from_attrs(
        pairs
            .iter()
            .map(|(n, count)| Attribute {
                target: EntityRef::Symbol {
                    name: n.to_string(),
                    file: None,
                },
                key: MODEL_CHURN_ATTR.to_string(),
                value: serde_json::json!(count),
            })
            .collect(),
    )
}

#[test]
fn cross_check_dead_model_no_store_binding_reported() {
    let issues = cross_check_default(
        &[model("Orphan", &["id", "payload"])],
        &usage(&[]),
        &AttributeStore::default(),
    );
    assert!(issues
        .iter()
        .any(|i| i.rule == "unreferenced-model-name" && i.model == "Orphan"));
}

#[test]
fn cross_check_dead_model_bound_model_not_reported() {
    let issues = cross_check_default(
        &[model("User", &["id", "nickname"])],
        &usage(&[("nickname", 5)]),
        &bound_attrs(&["User"]),
    );
    assert!(!issues.iter().any(|i| i.rule == "unreferenced-model-name"));
}

#[test]
fn cross_check_dead_field_zero_occurrences_reported() {
    let issues = cross_check_default(
        &[model("User", &["id", "nickname", "ghostField"])],
        &usage(&[("nickname", 3)]),
        &bound_attrs(&["User"]),
    );
    assert!(issues
        .iter()
        .any(|i| i.rule == "unreferenced-field-name" && i.field.as_deref() == Some("ghostField")));
    assert!(!issues
        .iter()
        .any(|i| i.rule == "unreferenced-field-name" && i.field.as_deref() == Some("nickname")));
}

#[test]
fn cross_check_dead_field_excludes_id_created_updated_at() {
    let issues = cross_check_default(
        &[model("X", &["id", "createdAt", "updatedAt", "name"])],
        &usage(&[]),
        &bound_attrs(&["X"]),
    );
    let dead_fields: Vec<&str> = issues
        .iter()
        .filter(|i| i.rule == "unreferenced-field-name")
        .map(|i| i.field.as_deref().unwrap())
        .collect();
    assert_eq!(dead_fields, vec!["name"]);
}

/// The INVERSE of the assertion that stood here until 2026-08-29
/// (`cross_check_dead_field_excludes_short_names`), kept rather than deleted so the removed
/// `MIN_FIELD_NAME_LEN = 3` floor is not re-added on intuition. It skipped one- and two-character names
/// on the theory that they "appear everywhere in BE source" — but whether a name appears is what
/// `identifier_counts` measures DIRECTLY, three lines further down, so the floor was a proxy for
/// evidence the rule already holds. It also never fired: on the corpus's only Prisma schema there are
/// zero candidate names shorter than three characters, and two builds one constant apart report the same
/// 27 findings. A short name that genuinely occurs nowhere is as strong a signal as a long one.
#[test]
fn cross_check_dead_field_has_no_name_length_floor() {
    let issues = cross_check_default(
        &[model("Y", &["id", "ab", "name"])],
        &usage(&[]),
        &bound_attrs(&["Y"]),
    );
    assert!(
        issues
            .iter()
            .any(|i| i.rule == "unreferenced-field-name" && i.field.as_deref() == Some("ab")),
        "a two-character name absent from all source must report: the length floor was removed"
    );
}

#[test]
fn cross_check_dead_field_not_reported_when_parent_is_dead_model() {
    let issues = cross_check_default(
        &[model("Q", &["id", "name", "payload"])],
        &usage(&[]),
        &AttributeStore::default(),
    );
    assert_eq!(
        issues
            .iter()
            .filter(|i| i.rule == "unreferenced-field-name")
            .count(),
        0
    );
}

// --- applyChurnRule ---

#[test]
fn churn_rule_at_least_5_reports() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &churn_attrs(&[("User", 5)]));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].severity, Severity::Info);
}

/// The second tier was removed on 2026-09-06 (see `usage.rs`'s churn-line doc): a threshold the code
/// itself calls unmeasurable must not hand out the loudest band. This pins the REPLACEMENT — a count
/// far past the old escalation line still reports in the one band — so a tier cannot come back unnoticed.
#[test]
fn churn_rule_well_past_the_old_escalation_line_stays_one_band() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &churn_attrs(&[("User", 12)]));
    assert_eq!(issues[0].severity, Severity::Info);
    assert_eq!(issues[0].params.as_ref().unwrap()["count"], 12);
}

#[test]
fn churn_rule_at_most_4_no_hit() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &churn_attrs(&[("User", 4)]));
    assert_eq!(issues.len(), 0);
}

#[test]
fn churn_rule_model_absent_from_churn_treated_as_zero() {
    let issues = apply_churn_rule(
        &[model("User", &["id"]), model("Item", &["id"])],
        &churn_attrs(&[("User", 6)]),
    );
    assert_eq!(
        issues.iter().map(|i| i.model.as_str()).collect::<Vec<_>>(),
        vec!["User"]
    );
}

#[test]
fn churn_rule_empty_churn_map_no_issues() {
    let issues = apply_churn_rule(&[model("User", &["id"])], &AttributeStore::default());
    assert_eq!(issues.len(), 0);
}

// --- analyzeSchema (usage branch) ---

fn risk_field(name: &str, optional: bool) -> SchemaField {
    // `createdAt`/`updatedAt` are spelled as a real schema spells them — `DateTime`, with `@updatedAt` on
    // the second — so that a fixture asking for a model with NO issues can actually have none. Since
    // `missing-timestamps` lost its field-count floor (2026-08-29) every model without timestamps
    // reports, and a `String` "createdAt" would trade that finding for a `temporal-as-string` one.
    let temporal = matches!(name, "createdAt" | "updatedAt");
    SchemaField {
        name: name.to_string(),
        r#type: if temporal { "DateTime" } else { "String" }.to_string(),
        optional,
        list: false,
        attrs: match name {
            "id" => vec![FieldAttr {
                name: "id".to_string(),
                args: None,
            }],
            "updatedAt" => vec![FieldAttr {
                name: "updatedAt".to_string(),
                args: None,
            }],
            _ => vec![],
        },
    }
}

fn risk_model(name: &str, field_names: &[&str]) -> SchemaModel {
    SchemaModel {
        name: name.to_string(),
        fields: field_names.iter().map(|n| risk_field(n, false)).collect(),
        ..Default::default()
    }
}

#[test]
fn analyze_with_usage_structural_only_model_risk_matches_summed_points() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model("P", &["id", "userId", "content"])],
        None,
        &AttributeStore::default(),
    );
    assert!(analysis.model_risk["P"] > 0);
    let expected: i64 = analysis
        .issues
        .iter()
        .filter(|i| i.model == "P")
        .map(|i| severity_points(i.severity))
        .sum();
    assert_eq!(analysis.model_risk["P"], expected);
}

#[test]
fn analyze_with_usage_every_model_gets_model_risk_entry_even_zero_issues() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model(
            "Lookup",
            &["id", "code", "createdAt", "updatedAt"],
        )],
        None,
        &AttributeStore::default(),
    );
    assert!(
        analysis.issues.iter().all(|i| i.model != "Lookup"),
        "fixture must genuinely raise no issue, or this test stops being about model_risk: {:?}",
        analysis.issues
    );
    assert_eq!(analysis.model_risk["Lookup"], 0);
}

#[test]
fn analyze_with_usage_signals_add_dead_model_field_and_churn_issues() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model("Ghost", &["id", "secretField"])],
        Some(SchemaUsage::default()),
        &churn_attrs(&[("Ghost", 12)]),
    );
    // Ghost is unbound -> unreferenced-model-name; churn 12 -> model-churn (one band). unreferenced-field-name is skipped under unreferenced-model-name.
    assert!(analysis
        .issues
        .iter()
        .any(|i| i.rule == "unreferenced-model-name"));
    assert!(analysis
        .issues
        .iter()
        .any(|i| i.rule == "model-churn" && i.severity == Severity::Info));
}

#[test]
fn analyze_with_usage_no_usage_runs_only_structural_rules() {
    let analysis = analyze_schema_with_usage(
        vec![risk_model("Orphan", &["id", "payload"])],
        None,
        &AttributeStore::default(),
    );
    assert!(!analysis
        .issues
        .iter()
        .any(|i| i.rule == "unreferenced-model-name"));
}

/// **The generated client never spells the model name.** Prisma lowercases the first character to build
/// its delegate, so `model UserPassword` is reached as `prisma.userPassword` and the PascalCase name
/// appears nowhere in correct, heavily-used code. Measured on calcom/cal.com `176037d`: 7 of 7
/// `unreferenced-model-name` findings examined were this, every one a model in daily use.
///
/// Both spellings and a genuinely-unused model ride in one call — "camelCase counts" and "nothing is
/// reported any more" are the same assertion without the control.
#[test]
fn a_model_reached_through_its_camel_case_delegate_is_referenced() {
    let models = [
        model("UserPassword", &["hashedValue"]),
        model("Team", &["displayName"]),
        model("NobodyUsesMe", &["someLabel"]),
    ];
    // `userPassword` is the delegate spelling; `Team` is named directly; nothing names NobodyUsesMe.
    let u = usage(&[
        ("userPassword", 3),
        ("hashedValue", 1),
        ("Team", 2),
        ("displayName", 1),
    ]);
    let issues = cross_check_default(&models, &u, &AttributeStore::default());
    let unreferenced: Vec<&str> = issues
        .iter()
        .filter(|i| i.rule == "unreferenced-model-name")
        .map(|i| i.model.as_str())
        .collect();
    assert_eq!(
        unreferenced,
        vec!["NobodyUsesMe"],
        "only the model nothing names may report: {issues:?}"
    );
}

/// **A relation navigator is not a deletable field.** It is the required opposite side of a `@relation`
/// declared on the other model; removing it makes `prisma validate` fail, so you cannot generate a
/// client, let alone migrate. Prisma also never requires code to name the back side — you traverse it
/// through `include` — so "no identifier hit" is the EXPECTED reading for a correct schema.
/// Measured on calcom/cal.com `176037d`, where the advice was literally "remove the field" for
/// `Team.orgUsers`, `Team.inviteTokens`, `Team.accessCodes` and more, out of 65 findings.
///
/// The test is structural — a field whose declared TYPE names another model in this schema — so a
/// genuinely dead scalar column in the SAME model still reports. That control is the point.
#[test]
fn a_relation_navigator_is_never_an_unreferenced_field_but_a_dead_scalar_still_is() {
    let mut team = model("Team", &["displayName", "hideBookATeamMember"]);
    team.fields.push(SchemaField {
        name: "orgUsers".to_string(),
        r#type: "User".to_string(), // the other model -> a navigator
        optional: false,
        list: true,
        attrs: vec![],
    });
    let models = [team, model("User", &["emailAddress"])];
    let u = usage(&[
        ("Team", 1),
        ("User", 1),
        ("displayName", 2),
        ("emailAddress", 2),
    ]);
    let issues = cross_check_default(&models, &u, &AttributeStore::default());
    let dead: Vec<&str> = issues
        .iter()
        .filter(|i| i.rule == "unreferenced-field-name")
        .filter_map(|i| i.field.as_deref())
        .collect();
    assert_eq!(
        dead,
        vec!["hideBookATeamMember"],
        "the navigator must be silent and the dead scalar must not be: {issues:?}"
    );
}

/// **The delegate spelling is only accepted for a MULTI-WORD model**, because `identifier_counts` is an
/// unqualified whole-tree token bag: it records that the token `user` appeared, never that `prisma.user`
/// did. Accepting a single-word delegate made the rule vacuous rather than merely loose — and worse than
/// silent, because the model-level short-circuit stopped firing and the FIELD loop ran, so two correct
/// findings were replaced by two asserting the opposite.
///
/// The canary is the review's own: two single-word models and a file whose only content is two ordinary
/// local bindings. A multi-word model rides along, since "single-word is rejected" and "the delegate is
/// never accepted" are the same assertion without it.
#[test]
fn a_single_word_delegate_is_too_common_a_token_to_count_as_a_reference() {
    let models = [
        model("User", &["fullName"]),
        model("Team", &["labelText"]),
        model("UserPassword", &["hashedValue"]),
    ];
    // No Prisma call anywhere — just two ordinary locals, plus the multi-word delegate.
    let u = usage(&[("user", 1), ("team", 1), ("userPassword", 2)]);
    let issues = cross_check_default(&models, &u, &AttributeStore::default());
    let unreferenced: Vec<&str> = issues
        .iter()
        .filter(|i| i.rule == "unreferenced-model-name")
        .map(|i| i.model.as_str())
        .collect();
    assert_eq!(
        unreferenced,
        vec!["User", "Team"],
        "single-word delegates must not count; the multi-word one must: {issues:?}"
    );
    // The other half of the same defect: a model wrongly judged "used" then emits FIELD findings that
    // assert the opposite. Neither single-word model may reach the field loop at all.
    assert!(
        !issues.iter().any(
            |i| i.rule == "unreferenced-field-name" && (i.model == "User" || i.model == "Team")
        ),
        "a model reported unreferenced must not also report its fields: {issues:?}"
    );
}
