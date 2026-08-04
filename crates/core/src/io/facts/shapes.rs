//! The witnessed/declared SHAPE types the io channel carries — request-body evidence
//! ([`ConsumeBodyShape`], [`ProvideBodyShape`]/[`ProvideBodyField`], `body-shape-v1`) and the
//! declared-response contract ([`ProvideResponseShape`], `response-shape-v1`). Split from `facts.rs`
//! purely for the repo per-file line cap; every type stays re-exported from its old paths
//! (`crate::io::*` / `crate::*`), and the serde wire contract is unchanged (frozen, do not reshape).

use serde::{Deserialize, Serialize};

/// The statically witnessed shape of a request-body object literal at an HTTP consume site.
/// Extraction is evidence-only: keys are recorded exactly as written (dotted paths, depth <= 2 —
/// one level under each top-level key, which is all the DTO comparison needs), and NOTHING is
/// inferred about parts the literal does not show. A body passed as an identifier/expression is
/// not represented at all (`IoConsume::body: None`), never approximated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeBodyShape {
    /// Dotted key paths witnessed in the literal (e.g. `"user"`, `"user.email"`). A shorthand
    /// property (`{ user }`) contributes its key; its children stay unwitnessed.
    pub keys: Vec<String>,
    /// Paths whose DIRECT children are exhaustively listed in `keys` — `""` for the top level.
    /// A level containing a spread, computed key, getter, or non-literal nested value is omitted,
    /// which suppresses any "missing field" comparison at that level (incomplete evidence stays
    /// silent). "Extra key" comparisons only need the witnessed key itself, so they survive.
    pub complete_at: Vec<String>,
}

/// One declared field of a request-body DTO class (name + whether the contract requires it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideBodyField {
    pub name: String,
    /// `true` when the field is `?`-optional or carries an `@IsOptional()` decorator.
    pub optional: bool,
}

/// The request-body contract a route handler declares (`@Body() dto: CreateUserDto`).
/// Emitted by the parser with only `dto_ref` set (the DTO class usually lives in another file);
/// assemble resolves the ref against the tree-wide merged class-shape map and fills `fields`.
/// An unresolvable or ambiguous ref drops the whole shape (never guessed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideBodyShape {
    /// `@Body('user')` sub-key — the DTO describes `body.user`, not the body root. `None` = root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_key: Option<String>,
    /// Unresolved DTO class name as written in the parameter type annotation. Present on parser
    /// emit; cleared by assemble once `fields` is materialized (an adapter overlay may instead
    /// supply `fields` directly and leave this `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dto_ref: Option<String>,
    /// Resolved DTO fields (empty until assemble resolves `dto_ref`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ProvideBodyField>,
    /// `false` when the DTO's field list may be partial (an `extends` clause, constructor
    /// parameter properties, an index signature, or computed keys) — suppresses "extra key"
    /// claims, since the unseen parent may declare the key.
    #[serde(default)]
    pub complete: bool,
}

/// The response contract a route handler DECLARES via its return-type annotation
/// (`response-shape-v1`: `async findOne(): Promise<UserDto>` — `Promise<X>` unwrapped syntactically,
/// plain single-identifier types only). Declaration-based, never flow-traced: return-statement
/// expressions and service-layer flows are out of scope by contract (see
/// the catalog row for `cross-layer/sensitive-response-field`, which carries the public
/// distillation of the declaration-only projection contract).
///
/// Emitted by the parser with only `dto_ref` set (the DTO class/interface usually lives in another
/// file); assemble resolves the ref against the same tree-wide merged class-shape map the request-body
/// path uses and fills `fields`. An unresolvable or ambiguous ref drops the whole shape (never guessed).
///
/// ONE deliberate overload: `dto_ref: None` AND `fields` empty is the parser's "handler declared NO
/// return type" sentinel — a zero-information shape that is stripped to `None` and disclosed as a
/// "declare a return type to enable this analysis" warning (the honesty half of never-guess: silence
/// must be distinguishable from coverage-zero). BOTH assembly lanes run that same strip/resolve pass
/// (`zzop_engine`'s native assemble, and Mode A envelope ingestion via `zzop_engine`'s
/// `envelope::shapes` — the identical functions, reused), so it never survives assembly on either
/// lane and downstream consumers (rules, the cross-layer join, JSON output) only ever see a resolved
/// shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideResponseShape {
    /// Unresolved response DTO class/interface name as written in the return-type annotation
    /// (`Promise<UserDto>` -> `"UserDto"`). Present on parser emit; cleared by assemble once
    /// `fields` is materialized (an adapter overlay may instead supply `fields` directly and leave
    /// this `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dto_ref: Option<String>,
    /// Resolved declared response fields (empty until assemble resolves `dto_ref`). Reuses
    /// [`ProvideBodyField`] — same name+optionality shape, same producer (the class-shape merge).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ProvideBodyField>,
    /// `false` when the DTO's field list may be partial (an `extends` clause, constructor parameter
    /// properties, an index signature, or computed keys) — same semantics as
    /// [`ProvideBodyShape::complete`].
    #[serde(default)]
    pub complete: bool,
}
