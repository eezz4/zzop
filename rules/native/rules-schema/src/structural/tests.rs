//! Unit tests for the structural schema rules — moved verbatim from `structural.rs`.
use super::*;
use zzop_core::{FieldAttr, SchemaField};

/// `apply_schema_rules` under the BUILT-IN money vocabulary — every case below is about rule logic, not
/// about which tokens mean money, so they all go through this one wrapper rather than repeating the
/// default at 20 call sites. The declared-vocabulary path is sealed separately (engine e2e).
fn apply_schema_rules_default(models: &[SchemaModel]) -> Vec<SchemaIssue> {
    apply_schema_rules(models, MONEY_TOKENS)
}

fn field(
    name: &str,
    ty: &str,
    optional: bool,
    list: bool,
    attrs: &[(&str, Option<&str>)],
) -> SchemaField {
    SchemaField {
        name: name.into(),
        r#type: ty.into(),
        optional,
        list,
        attrs: attrs
            .iter()
            .map(|(n, a)| FieldAttr {
                name: n.to_string(),
                args: a.map(str::to_string),
            })
            .collect(),
    }
}
fn f(name: &str) -> SchemaField {
    field(name, "String", false, false, &[])
}
fn id() -> SchemaField {
    field("id", "String", false, false, &[("id", None)])
}
fn model(
    name: &str,
    fields: Vec<SchemaField>,
    uniques: Vec<Vec<String>>,
    indexes: Vec<Vec<String>>,
) -> SchemaModel {
    SchemaModel {
        name: name.into(),
        fields,
        uniques,
        indexes,
        ..Default::default()
    }
}
fn cols(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}
fn has(issues: &[SchemaIssue], rule: &str, field: &str) -> bool {
    issues
        .iter()
        .any(|i| i.rule == rule && i.field.as_deref() == Some(field))
}

#[test]
fn fk_no_index_hit() {
    let issues = apply_schema_rules_default(&[model(
        "Post",
        vec![id(), f("ownerId"), f("title")],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "fk-no-index", "ownerId"));
}

#[test]
fn fk_no_index_covered_by_unique() {
    let issues = apply_schema_rules_default(&[model(
        "Share",
        vec![id(), f("itemId")],
        vec![cols(&["itemId"])],
        vec![],
    )]);
    assert!(!has(&issues, "fk-no-index", "itemId"));
}

#[test]
fn fk_no_index_leading_composite_member_fully_covered() {
    let issues = apply_schema_rules_default(&[model(
        "ItemUser",
        vec![id(), f("itemId"), f("userId")],
        vec![cols(&["itemId", "userId"])],
        vec![],
    )]);
    assert!(!has(&issues, "fk-no-index", "itemId"));
}

#[test]
fn fk_no_index_non_leading_composite_member_gets_info_variant() {
    let issues = apply_schema_rules_default(&[model(
        "Member",
        vec![id(), f("a"), f("guildId")],
        vec![cols(&["a", "guildId"])],
        vec![],
    )]);
    let matches: Vec<_> = issues
        .iter()
        .filter(|i| i.rule == "fk-no-index" && i.field.as_deref() == Some("guildId"))
        .collect();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].severity, Severity::Info);
    let params = matches[0].params.as_ref().unwrap();
    assert_eq!(params["coverage"], "non-leading");
    let composite_cols = params["compositeCols"].as_array().unwrap();
    assert_eq!(
        composite_cols,
        &vec!["a".to_string(), "guildId".to_string()]
    );
}

#[test]
fn missing_timestamps_domain_named_creation_field_with_default_now_satisfies_created_at() {
    let issues = apply_schema_rules_default(&[model(
        "Event",
        vec![
            id(),
            f("userId"),
            f("payload"),
            field(
                "receivedAt",
                "DateTime",
                false,
                false,
                &[("default", Some("now()"))],
            ),
        ],
        vec![],
        vec![],
    )]);
    let t = issues
        .iter()
        .find(|i| i.rule == "missing-timestamps")
        .expect("expected a missing-timestamps issue for missing updatedAt");
    let missing = t.params.as_ref().unwrap()["missing"].as_array().unwrap();
    assert!(missing.iter().any(|v| v == "updatedAt"));
    assert!(!missing.iter().any(|v| v == "createdAt"));
}

#[test]
fn missing_timestamps_domain_named_creation_field_and_updated_at_present_has_no_issue() {
    let issues = apply_schema_rules_default(&[model(
        "Event",
        vec![
            id(),
            f("userId"),
            f("payload"),
            field(
                "receivedAt",
                "DateTime",
                false,
                false,
                &[("default", Some("now()"))],
            ),
            field("updatedAt", "DateTime", false, false, &[]),
        ],
        vec![],
        vec![],
    )]);
    assert!(!issues.iter().any(|i| i.rule == "missing-timestamps"));
}

#[test]
fn missing_timestamps_reports_updated_at() {
    let issues = apply_schema_rules_default(&[model(
        "Log",
        vec![
            id(),
            f("userId"),
            f("msg"),
            field("createdAt", "DateTime", false, false, &[]),
        ],
        vec![],
        vec![],
    )]);
    let t = issues
        .iter()
        .find(|i| i.rule == "missing-timestamps")
        .unwrap();
    let missing = t.params.as_ref().unwrap()["missing"].as_array().unwrap();
    assert!(missing.iter().any(|v| v == "updatedAt"));
}

/// The INVERSE of the assertion that stood here until 2026-08-29 (`missing_timestamps_excludes_lookup`),
/// and it is kept as a test rather than deleted because the removed `LOOKUP_FIELD_MAX = 3` floor is the
/// kind of thing someone re-adds on intuition. It exempted models of at most three fields as "assumed
/// lookup tables"; measured on the corpus's only Prisma schema (cal.com, 100 models) it exempted exactly
/// ONE model — `UserPassword`, a credential store rather than a lookup table — and moved one finding in
/// 54 (53 at 3, 54 at 2, 52 at 4, 54 with no floor). A two-field model now reports.
#[test]
fn missing_timestamps_has_no_field_count_floor() {
    let issues =
        apply_schema_rules_default(&[model("Lookup", vec![id(), f("code")], vec![], vec![])]);
    assert!(
        issues.iter().any(|i| i.rule == "missing-timestamps"),
        "a two-field model must report: the field-count floor was removed, not narrowed"
    );
}

#[test]
fn god_model_hit() {
    let mut fields = vec![id()];
    for i in 0..16 {
        fields.push(f(&format!("f{i}")));
    }
    let issues = apply_schema_rules_default(&[model("Big", fields, vec![], vec![])]);
    assert!(issues.iter().any(|i| i.rule == "god-model"));
}

/// The relation FIELD that declares `<scalar>` to be a foreign key — Prisma writes `@relation` here and
/// never on the scalar column, so every `nullable-fk` fixture needs one of these beside the column.
fn rel(name: &str, ty: &str, args: &str) -> SchemaField {
    field(name, ty, true, false, &[("relation", Some(args))])
}

#[test]
fn nullable_fk_hit() {
    let issues = apply_schema_rules_default(&[model(
        "Item",
        vec![
            id(),
            field("ownerId", "String", true, false, &[]),
            rel("owner", "User", "fields: [ownerId], references: [id]"),
            f("name"),
        ],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "nullable-fk", "ownerId"));
}

/// GATE 1 (rule-quality.md §24/§26) — the rule's noun is "foreign key", and without a declared
/// `@relation` naming the column that noun is a guess off the name alone. cal.com's `User.calcomUserId
/// Int? @unique` is the shape: an id another system mints, which an outside auditor read and judged
/// IGNORE for exactly this reason. Measured harvest: 39 of 136 cal.com firings; 35 of those columns are already
/// reported by `implicit-fk` (whose count this gate leaves unchanged) — the rule whose claim about them
/// is true, and the 4 that nothing reports afterwards are all `@unique` external-system ids.
#[test]
fn nullable_fk_needs_a_declared_relation() {
    let issues = apply_schema_rules_default(&[model(
        "User",
        vec![
            id(),
            field("calcomUserId", "Int", true, false, &[("unique", None)]),
        ],
        vec![],
        vec![],
    )]);
    assert!(
        !has(&issues, "nullable-fk", "calcomUserId"),
        "a name-only *Id guess is a false CLAIM, not noise: {issues:?}"
    );
}

/// GATE 2 (rule-quality.md §24/§26) — Prisma refuses `onDelete: SetNull` unless the relation's scalar
/// fields are optional, so the very line the rule parsed already answers the question it asks. Measured
/// harvest: 20 of the 97 that survive gate 1.
#[test]
fn nullable_fk_declines_when_the_relation_sets_null_on_delete() {
    let issues = apply_schema_rules_default(&[model(
        "Item",
        vec![
            id(),
            field("ownerId", "String", true, false, &[]),
            rel(
                "owner",
                "User",
                "fields: [ownerId], references: [id], onDelete : SetNull",
            ),
        ],
        vec![],
        vec![],
    )]);
    assert!(
        !has(&issues, "nullable-fk", "ownerId"),
        "the declared action REQUIRES the `?` -- there is nothing left to confirm: {issues:?}"
    );
}

/// §26 ③ — the directions that were measured and REFUSED, pinned so a later widening has to argue with a
/// red test rather than with silence. `onDelete: Cascade` is the majority spelling among the findings that
/// remain (56 of 77 on cal.com) and says nothing about optionality; `@default` on the column harvests 0
/// here; `@unique` (a 1:1 relation) and a self-referential relation harvest 7 and 3 respectively and are
/// refused on evidence GRADE — a 1:1 relation and a parent pointer are both expressible as required
/// columns, so neither declaration REQUIRES the `?`.
#[test]
fn nullable_fk_still_fires_on_the_refused_directions() {
    let issues = apply_schema_rules_default(&[
        model(
            "Cascading",
            vec![
                id(),
                field("ownerId", "String", true, false, &[]),
                rel(
                    "owner",
                    "User",
                    "fields: [ownerId], references: [id], onDelete: Cascade",
                ),
            ],
            vec![],
            vec![],
        ),
        model(
            "OneToOne",
            vec![
                id(),
                field("profileId", "String", true, false, &[("unique", None)]),
                rel(
                    "profile",
                    "Profile",
                    "fields: [profileId], references: [id]",
                ),
            ],
            vec![],
            vec![],
        ),
        model(
            "Node",
            vec![
                id(),
                field(
                    "parentId",
                    "String",
                    true,
                    false,
                    &[("default", Some("\"0\""))],
                ),
                rel("parent", "Node", "fields: [parentId], references: [id]"),
            ],
            vec![],
            vec![],
        ),
    ]);
    for (model_field, why) in [
        ("ownerId", "onDelete: Cascade does not require optionality"),
        (
            "profileId",
            "@unique is a 1:1 relation, not a NULL requirement",
        ),
        (
            "parentId",
            "a self-relation with a pinned default is still a choice",
        ),
    ] {
        assert!(
            has(&issues, "nullable-fk", model_field),
            "{why}: {issues:?}"
        );
    }
}

#[test]
fn implicit_fk_hit() {
    let issues = apply_schema_rules_default(&[model(
        "Ref",
        vec![id(), f("userId"), f("name")],
        vec![],
        vec![cols(&["userId"])],
    )]);
    assert!(has(&issues, "implicit-fk", "userId"));
}

#[test]
fn implicit_fk_with_relation_no_hit() {
    let issues = apply_schema_rules_default(&[model(
        "X",
        vec![
            id(),
            f("targetId"),
            field(
                "target",
                "Target",
                false,
                false,
                &[("relation", Some("fields: [targetId], references: [id]"))],
            ),
        ],
        vec![],
        vec![],
    )]);
    assert!(!issues.iter().any(|i| i.rule == "implicit-fk"));
}

/// The Avatar shape from cal.com (`packages/prisma/schema.prisma:1636`): `teamId Int @default(0)` and
/// `userId Int @default(0)`, where `0` is the schema's own way of writing "no team"/"no user". A pinned
/// literal default is a DECLARATION in the scanned source (rule-quality.md §24) that the column carries a
/// value of the author's choosing when nobody supplies one, and a value chosen by the schema is not a
/// parent key — so this rule's prescription (`@relation`) would ask the reader to add a constraint that
/// cannot hold. Same evidence channel the rule already reads (`field.attrs`), so §26 ① holds: no new
/// vocabulary, no new channel.
#[test]
fn implicit_fk_literal_default_is_a_sentinel_declaration_and_is_not_reported() {
    let issues = apply_schema_rules_default(&[model(
        "Avatar",
        vec![
            field("teamId", "Int", false, false, &[("default", Some("0"))]),
            field("userId", "Int", false, false, &[("default", Some("0"))]),
            f("data"),
            field("objectKey", "String", false, false, &[("unique", None)]),
        ],
        vec![cols(&["teamId", "userId", "isBanner"])],
        vec![],
    )]);
    assert!(!has(&issues, "implicit-fk", "teamId"), "{issues:?}");
    assert!(!has(&issues, "implicit-fk", "userId"), "{issues:?}");
}

/// A quoted-string literal is the other spelling reachable on this rule's population (`is_fk_candidate`
/// admits only `String`/`Int`/`BigInt`, so an enum member or a boolean can never appear here).
#[test]
fn implicit_fk_string_literal_default_is_also_a_sentinel() {
    let issues = apply_schema_rules_default(&[model(
        "Row",
        vec![
            id(),
            field(
                "tenantId",
                "String",
                false,
                false,
                &[("default", Some("\"\""))],
            ),
        ],
        vec![],
        vec![],
    )]);
    assert!(!has(&issues, "implicit-fk", "tenantId"), "{issues:?}");
}

/// The canary for the gate above, in the other direction. A FUNCTION default generates a fresh value per
/// row — it pins nothing, so it declares nothing about referential intent and must NOT silence the rule.
/// Without this the gate could widen to "has any `@default`" and nothing would go red.
#[test]
fn implicit_fk_function_default_is_not_a_sentinel_and_still_reports() {
    let issues = apply_schema_rules_default(&[model(
        "Ref",
        vec![
            id(),
            field(
                "traceId",
                "String",
                false,
                false,
                &[("default", Some("uuid()"))],
            ),
            field(
                "seqId",
                "Int",
                false,
                false,
                &[("default", Some("autoincrement()"))],
            ),
        ],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "implicit-fk", "traceId"), "{issues:?}");
    assert!(has(&issues, "implicit-fk", "seqId"), "{issues:?}");
}

#[test]
fn float_money_hit() {
    let issues = apply_schema_rules_default(&[model(
        "Order",
        vec![id(), field("totalAmount", "Float", false, false, &[])],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "float-money", "totalAmount"));
}

#[test]
fn temporal_as_string_hit() {
    let issues = apply_schema_rules_default(&[model(
        "Ev",
        vec![id(), field("startTime", "String", false, false, &[])],
        vec![],
        vec![],
    )]);
    assert!(has(&issues, "temporal-as-string", "startTime"));
}

#[test]
fn analyze_schema_sums_model_risk() {
    // one warning (nullable-fk) = 2 points on model "Item".
    let a = analyze_schema(vec![model(
        "Item",
        vec![
            id(),
            field("ownerId", "String", true, false, &[]),
            rel("owner", "User", "fields: [ownerId], references: [id]"),
        ],
        vec![cols(&["ownerId"])], // covered -> no fk-no-index; nullable-fk still fires
        vec![],
    )]);
    assert!(a.model_risk["Item"] >= 2);
}
