//! Prisma schema IR — the normalized schema representation `zzop-parser-prisma` constructs directly while
//! parsing `.prisma` text, and the usage-evidence IR both `zzop-engine` and `zzop-parser-prisma` assemble
//! directly from their own source scans. Kept in `zzop-core` as a shared IR contract per the crate-boundary
//! split: the schema *rules* that operate on this IR
//! (structural anti-patterns, usage-aware cross-checks) moved to `zzop-rules-schema`, but these types stay
//! here since a foundational parser crate constructs them directly and must not depend on a rules crate.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A field attribute (`@id`, `@default(...)`, `@map("...")`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldAttr {
    /// "id", "default", "map", "unique", "relation", "index".
    pub name: String,
    /// Raw string inside parentheses, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    /// Declared base type ("String", "DateTime", "User", ...) without list/optional modifiers.
    pub r#type: String,
    pub optional: bool,
    pub list: bool,
    pub attrs: Vec<FieldAttr>,
}

/// A Prisma `enum Name { MEMBER ... }` block — the enum-type counterpart to `SchemaModel`, projected
/// directly by `zzop-parser-prisma::parse_schema_enums` from `.prisma` text. Unlike `SchemaModel` (whose
/// `.prisma` declaration line is looked up lazily and separately, via `model_decl_line`, only when a
/// finding needs to anchor at one), `line` is carried on the struct itself since the one consumer of this
/// type (`zzop_rules_schema::join::enum_string_drift_issues`, via `zzop-engine`) has no other line evidence
/// to fall back on for a schema-anchored enum-level finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaEnum {
    pub name: String,
    /// Declared member names, in declaration order.
    pub members: Vec<String>,
    /// 1-based line of `enum <name> {` in the (raw, un-normalized) schema text.
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SchemaModel {
    pub name: String,
    /// @@map value (snake_case table name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_name: Option<String>,
    pub fields: Vec<SchemaField>,
    /// @@unique([a, b]) — field-name arrays per entry.
    pub uniques: Vec<Vec<String>>,
    /// @@index([a]).
    pub indexes: Vec<Vec<String>>,
    /// Source identifier when merging multiple domain schema.prisma files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// Usage signal a producer extracts from BE code, cross-checked against the schema-IR by
/// `zzop_rules_schema::analyze_schema_with_usage` (dead-field, and dead-model's "is the name referenced
/// anywhere" half). A producer with no code access can omit this entirely and only the structural rules
/// run (`zzop_rules_schema::analyze_schema`).
///
/// Store-binding and migration-churn signals no longer live here — they're injected via the generic
/// entity-attribute channel instead (`zzop_core::AttributeStore`, Symbol-keyed `bound-model`/`model-churn`),
/// which dead-model and schema-churn read directly.
#[derive(Debug, Clone, Default)]
pub struct SchemaUsage {
    /// Identifier name -> total occurrences in BE source (comments/strings stripped). Drives dead-field.
    pub identifier_counts: HashMap<String, u32>,
}
