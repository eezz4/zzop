//! `nullable-fk` and `implicit-fk` — the two structural rules that judge a **declared** `@relation`, from
//! opposite sides of one predicate. Split out of `rules.rs` when the `nullable-fk` gates pushed that file
//! past the 300-line guard, and kept together because [`declared_relation`] is the single spelling both
//! read: `implicit-fk` fires only where it answers `None` and `nullable-fk` only where it answers `Some`,
//! so the two partition a model's foreign-key-shaped columns between them and neither can drift from the
//! other. That partition is why the gate below is available to `nullable-fk` at all and NOT to its
//! sibling — requiring a declared `@relation` in `implicit-fk` would make its firing condition and its
//! suppression condition each other's negation, which is a rule that never fires.

use zzop_core::{FieldAttr, SchemaField, SchemaModel, Severity};

use super::{has_attr, has_pinned_literal_default, issue, SchemaIssue};

/// The `@relation(fields: [<field>], ...)` attribute that DECLARES `field` to be a foreign key, if this
/// model carries one.
///
/// Prisma writes the attribute on the relation FIELD (`user User? @relation(fields: [userId], references:
/// [id])`), never on the scalar column it names, so the question is only answerable against the whole
/// model — which is what `rule_implicit_fk` already did inline before this became shared.
///
/// Substring rather than a parsed `fields:` list on purpose: it is the spelling `implicit-fk` shipped
/// with, and re-spelling it as an exact list-member test harvests 0 differences on the corpus (measured on
/// cal.com, the only tree of the nine holding a `.prisma` file: 170 foreign-key-shaped columns, 170
/// matched identically by both spellings, 0 matched by substring alone). §26 ③ rejects a direction with no
/// harvest, and this one would additionally cost the two rules the shared predicate above.
fn declared_relation<'a>(model: &'a SchemaModel, field: &SchemaField) -> Option<&'a FieldAttr> {
    model
        .fields
        .iter()
        .flat_map(|f| f.attrs.iter())
        .find(|a| a.name == "relation" && a.args.as_deref().unwrap_or("").contains(&field.name))
}

/// `onDelete: SetNull` on the declaring `@relation` — the one referential action Prisma REFUSES to accept
/// unless the relation's scalar fields are optional. The line this rule parsed therefore already answers
/// the question this rule asks: the `?` is not an oversight, it is what the declared action requires, and
/// a reader told to "confirm the optional relation is intentional" has nothing left to confirm.
///
/// A suppression, so §24 demands a declaration in the scanned source rather than an inference — this is
/// one, hand-written by the schema's author, and it arrives on the same `attrs` channel `has_attr` and
/// [`declared_relation`] already read, so no new evidence kind and no new vocabulary enter with it
/// (§26 ①). Harvest, measured (§26 ②): **20** findings, all on cal.com.
///
/// Whitespace-insensitive by normalization rather than by regex: `parse_attrs` hands over the raw text
/// between the parentheses and `onDelete : SetNull` is legal Prisma. `SetDefault` is deliberately NOT
/// treated the same way — it constrains `@default`, not optionality, and harvests 0 here anyway.
fn sets_null_on_delete(rel: &FieldAttr) -> bool {
    let args: String = rel
        .args
        .as_deref()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    args.contains("onDelete:SetNull")
}

/// An optional column that a declared `@relation` names as a foreign key, where the schema has not already
/// said the column must be nullable.
///
/// Two gates stand in front of the `field.optional` test this rule shipped as, and each removes a
/// population the rule was wrong about rather than one it was merely noisy on:
///
/// 1. **A declared `@relation` is required.** Without one the rule's own noun is a guess off the column
///    NAME (`is_fk_candidate` admits any `String`/`Int`/`BigInt` ending in `Id`), and a finding that opens
///    "field X is a nullable foreign key" about `User.calcomUserId Int? @unique` — an id minted by another
///    system, with no relation in this schema — is not noise, it is a false claim. §24 is satisfied
///    because `@relation` is a declaration in the scanned source, not a boundary inferred from layout.
/// 2. **`onDelete: SetNull` rejects.** See [`sets_null_on_delete`].
///
/// Harvest, measured on cal.com (§26 ②): 136 firings → 97 after gate 1 → **77** after gate 2. What the 59
/// dropped columns are reported by AFTERWARDS was counted rather than assumed, because a suppression's
/// real cost is what goes silent, not what stops firing:
///
/// - **35** are already reported by `implicit-fk` — the rule whose claim about them is true, and which
///   carries `FK_CONSTRAINT_LANDING`. Its count is unchanged by this gate (measured: 57 both arms).
/// - **20** are the gate-2 columns, and going silent IS the point: their author declared the action that
///   requires the `?`, so there is no question left to put to them.
/// - **4** are reported by nothing after this change, all four `@unique` ids minted by another system
///   (`User.calcomUserId`, `OrganizationOnboarding.stripeCustomerId`,
///   `CalAiPhoneNumber.providerPhoneNumberId`, `CalAiPhoneNumber.stripeSubscriptionId`) — and one of the
///   four is the exact finding an outside auditor read and judged IGNORE.
///
/// Directions NOT taken, each measured against the 77 that remain (§26 ③ — a direction with no harvest is
/// refused, and one with harvest can still be refused on evidence GRADE): `@default` on the column, 0;
/// `@relation` written on the scalar field itself, 0; `onDelete: SetDefault`, 0. `@unique` on the column
/// would harvest 7 and a self-referential relation 3, and both are refused on §24 grounds instead — a 1:1
/// relation and a parent pointer are both perfectly expressible as required columns, so neither
/// declaration REQUIRES the `?`, and reading them as if they did would suppress on convention.
pub(in crate::structural) fn rule_nullable_fk(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
    if !field.optional {
        return;
    }
    let Some(rel) = declared_relation(model, field) else {
        return;
    };
    if sets_null_on_delete(rel) {
        return;
    }
    out.push(issue(
        "nullable-fk",
        Severity::Warning,
        &model.name,
        Some(&field.name),
    ));
}

/// A foreign-key-shaped column that no `@relation` models — the negative side of [`declared_relation`].
pub(in crate::structural) fn rule_implicit_fk(
    model: &SchemaModel,
    field: &SchemaField,
    out: &mut Vec<SchemaIssue>,
) {
    if has_attr(field, "relation") || has_attr(field, "unique") || has_pinned_literal_default(field)
    {
        return;
    }
    if declared_relation(model, field).is_some() {
        return;
    }
    out.push(issue(
        "implicit-fk",
        Severity::Info,
        &model.name,
        Some(&field.name),
    ));
}
