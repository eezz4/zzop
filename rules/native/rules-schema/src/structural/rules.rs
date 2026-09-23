//! Rule bodies and index-coverage helpers for the structural schema rules — moved verbatim from
//! `structural.rs` (the thin root keeps the census-pinned consts, the
//! IR-facing types, and the `analyze_schema`/`apply_schema_rules` orchestrators).

use zzop_core::{SchemaField, SchemaModel, Severity};

use super::{SchemaIssue, GOD_THRESHOLD};

mod coverage;
mod relation;

use coverage::{index_coverage, Coverage};
pub(super) use relation::{rule_implicit_fk, rule_nullable_fk};

fn issue(rule: &str, severity: Severity, model: &str, field: Option<&str>) -> SchemaIssue {
    SchemaIssue {
        rule: rule.to_string(),
        severity,
        model: model.to_string(),
        field: field.map(str::to_string),
        params: None,
    }
}

pub(super) fn rule_god_model(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
    if model.fields.len() < GOD_THRESHOLD {
        return;
    }
    // Ships `info` (2026-09-06). The message already says the 15-field line is "a convention, not a
    // measurement" and that one field moves the count by 4-5, which is a rule declaring that it reports
    // a shape rather than judging one. Nothing is dropped; it sits behind the defect claims.
    let mut i = issue("god-model", Severity::Info, &model.name, None);
    i.params = Some(serde_json::json!({ "fieldCount": model.fields.len() }));
    out.push(i);
}

/// NO FIELD-COUNT FLOOR — removed 2026-08-29, and the removal is the point rather than a cleanup.
///
/// A `LOOKUP_FIELD_MAX = 3` floor used to skip models with at most three fields, documented as
/// "assumed lookup tables". Measured against the only Prisma schema in the 9-tree corpus
/// (calcom/cal.com, 100 models; two clean release builds one constant apart), the floor exempted
/// **exactly one model**, and the count barely noticed it: 53 findings at 3, 54 at 2, 52 at 4, 54
/// with no floor at all. A gate that moves one finding in 54 is not splitting the judgment.
///
/// Worse, the one model it exempted was `UserPassword` (`hash`, `userId`, `user`) — a credential
/// store, not a lookup table, and close to the model where "when was this row last written" is worth
/// the most. The floor's stated class and the floor's actual effect were disjoint on the only
/// evidence anyone has.
///
/// What it costs to remove, stated rather than left to be discovered: a genuinely tiny join model now
/// draws an `info` finding it used to be spared. That class is empty in every measured tree (no model
/// under three fields exists in the corpus), so the price is unmeasured rather than zero — but the
/// exemption was NOT free either, and `UserPassword` is what it bought.
pub(super) fn rule_missing_timestamps(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
    let names: std::collections::HashSet<&str> =
        model.fields.iter().map(|f| f.name.as_str()).collect();
    // A creation timestamp is satisfied by a field named `createdAt`, or by any `DateTime
    // @default(now())` field (e.g. `receivedAt` on an append-only event model) — both are equally valid.
    let has_creation_ts = names.contains("createdAt") || model.fields.iter().any(has_default_now);
    let mut missing = Vec::new();
    if !has_creation_ts {
        missing.push("createdAt");
    }
    if !names.contains("updatedAt") {
        missing.push("updatedAt");
    }
    if missing.is_empty() {
        return;
    }
    let mut i = issue("missing-timestamps", Severity::Info, &model.name, None);
    i.params = Some(serde_json::json!({ "missing": missing }));
    out.push(i);
}

fn has_default_now(field: &SchemaField) -> bool {
    field.r#type.eq_ignore_ascii_case("DateTime")
        && field
            .attrs
            .iter()
            .any(|a| a.name == "default" && a.args.as_deref().unwrap_or("").contains("now()"))
}

pub(super) fn rule_redundant_index(model: &SchemaModel, out: &mut Vec<SchemaIssue>) {
    let single_unique: std::collections::HashSet<&str> = model
        .uniques
        .iter()
        .filter(|g| g.len() == 1)
        .map(|g| g[0].as_str())
        .collect();
    let already_indexed = |col: &str| -> bool {
        if single_unique.contains(col) {
            return true;
        }
        model
            .fields
            .iter()
            .find(|x| x.name == col)
            .is_some_and(|f| has_attr(f, "id") || has_attr(f, "unique"))
    };
    for group in &model.indexes {
        if group.len() != 1 {
            continue;
        }
        let col = group[0].as_str();
        if already_indexed(col) {
            out.push(issue(
                "redundant-index",
                Severity::Info,
                &model.name,
                Some(col),
            ));
        }
    }
}

pub(super) fn rule_float_money(
    model: &SchemaModel,
    field: &SchemaField,
    money_tokens: &[&str],
    out: &mut Vec<SchemaIssue>,
) {
    let t = field.r#type.to_ascii_lowercase();
    if t != "float" && t != "double" && t != "real" {
        return;
    }
    // Shared with the config front end's "can this entry ever match?" check — see
    // `zzop_core::vocab_norm`'s module doc.
    let lower = zzop_core::vocab_norm::ascii_lowercase(&field.name);
    if !money_tokens.iter().any(|tok| lower.contains(tok)) {
        return;
    }
    let mut i = issue(
        "float-money",
        Severity::Warning,
        &model.name,
        Some(&field.name),
    );
    i.params = Some(serde_json::json!({ "type": field.r#type }));
    out.push(i);
}

pub(super) fn rule_stale_updated_at(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
    if field.name != "updatedAt" {
        return;
    }
    let t = field.r#type.to_ascii_lowercase();
    if t != "datetime" && t != "timestamp" && t != "date" {
        return;
    }
    if has_attr(field, "updatedAt") {
        return;
    }
    out.push(issue(
        "stale-updated-at",
        Severity::Warning,
        &model.name,
        Some(&field.name),
    ));
}

pub(super) fn rule_temporal_as_string(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
    if field.r#type != "String" {
        return; // Int/BigInt epoch is legitimate; only flag text-stored dates.
    }
    let n = &field.name;
    let suffix =
        n.ends_with("At") || n.ends_with("Date") || n.ends_with("Time") || n.ends_with("Timestamp");
    let exact = matches!(
        n.to_ascii_lowercase().as_str(),
        "date" | "time" | "timestamp" | "datetime"
    );
    if suffix || exact {
        out.push(issue(
            "temporal-as-string",
            Severity::Warning,
            &model.name,
            Some(n),
        ));
    }
}

pub(super) fn rule_fk_no_index(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
    if has_attr(field, "unique") || has_attr(field, "id") {
        return;
    }
    match index_coverage(&field.name, &model.uniques, &model.indexes) {
        Coverage::Leading => {}
        Coverage::NonLeading { cols, kind } => {
            let mut i = issue(
                "fk-no-index",
                Severity::Info,
                &model.name,
                Some(&field.name),
            );
            i.params = Some(serde_json::json!({
                "coverage": "non-leading",
                "compositeCols": cols,
                "compositeKind": kind,
            }));
            out.push(i);
        }
        Coverage::None => {
            out.push(issue(
                "fk-no-index",
                Severity::Warning,
                &model.name,
                Some(&field.name),
            ));
        }
    }
}

pub(super) fn is_fk_candidate(field: &SchemaField) -> bool {
    if field.name == "id" || field.name == "_id" {
        return false;
    }
    if !field.name.ends_with("Id") && !field.name.to_ascii_lowercase().ends_with("_id") {
        return false;
    }
    matches!(field.r#type.as_str(), "String" | "Int" | "BigInt")
}

fn has_attr(field: &SchemaField, name: &str) -> bool {
    field.attrs.iter().any(|a| a.name == name)
}

/// `@default(<literal>)` on a foreign-key-shaped column — the author PINNED a value the database stores
/// when nobody supplies one, and a value the schema chose for itself is not a parent key. cal.com's
/// `Avatar.teamId Int @default(0)` is the shape (`// e.g. NULL(0), organization ID or team logo`): every
/// user avatar row carries `0`, so the `@relation` this rule would otherwise prescribe generates a
/// FOREIGN KEY that Postgres rejects against the existing rows.
///
/// This is a SUPPRESSION, so its evidence has to be a declaration in the scanned source rather than a
/// guess (rule-quality.md §24) — the `@default` attribute is exactly that, and it arrives on the same
/// `field.attrs` channel `has_attr` already reads for `@relation`/`@unique`, so no new evidence kind and
/// no new vocabulary enter with it (§26 ①).
///
/// LITERAL, not any default: `@default(uuid())`/`@default(autoincrement())`/`@default(now())` mint a
/// fresh value per row and therefore pin nothing, so they say nothing about referential intent and do
/// not silence the rule. `is_fk_candidate` admits only `String`/`Int`/`BigInt`, which leaves exactly two
/// literal spellings reachable here — a number and a quoted string — and both are tested. Widening to
/// "has any `@default`" would harvest 0 further findings on the 9-tree corpus (measured: the only
/// FK-shaped columns carrying any `@default` at all are the two `Avatar` columns above), and §26 ③
/// rejects a direction with no harvest.
fn has_pinned_literal_default(field: &SchemaField) -> bool {
    field.attrs.iter().any(|a| {
        a.name == "default"
            && a.args
                .as_deref()
                .map(str::trim)
                .is_some_and(is_pinned_literal)
    })
}

/// Positive test, never "does not look like a call": a literal is a number or a quoted string, and every
/// other spelling — a generator call, a `dbgenerated(...)`, a list, a bare identifier — falls through to
/// false. Asking the question the other way round would make an unrecognized spelling SUPPRESS, which is
/// the wrong default for a veto.
fn is_pinned_literal(arg: &str) -> bool {
    let mut chars = arg.chars();
    match chars.next() {
        Some('"') => true,
        Some(c) if c.is_ascii_digit() => true,
        Some('-') | Some('+') => chars.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    }
}
