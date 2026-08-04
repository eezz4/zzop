//! JPA `@Entity`/`@Table` -> `db-table` PROVIDE extraction (the Java member of the ORM db-table
//! family, alongside `zzop_parser_go::adapters::gorm`, `zzop_parser_typescript`'s TypeORM adapters and
//! the Prisma/SQL provide sides). Import-gated on the JPA annotation packages
//! ([`JPA_PACKAGE_PREFIXES`] — `jakarta.persistence`/legacy `javax.persistence`, plain or glob); a file
//! importing neither yields nothing.
//!
//! A class annotated `@Entity` is a mapped table. Its table name, in never-guess order:
//! - `@Table(name = "…")` string literal — used verbatim (the annotation IS the physical name).
//! - `@Table` present with a NON-LITERAL `name` (a constant reference) — the class is SKIPPED entirely:
//!   this per-file pass cannot resolve the constant, and falling back to the default would key a table
//!   the mapping explicitly renames (a fabricated provide, worse than silence).
//! - No `@Table` name: the default is derived from the entity name (`@Entity(name = "…")` literal when
//!   present, else the class's simple name) through [`camel_to_snake`] — Spring Boot's Hibernate
//!   default (`CamelCaseToUnderscoresNamingStrategy`, reproduced ALGORITHM-FOR-ALGORITHM: an
//!   underscore only at a lowercase→uppercase→lowercase triple, never before the last character, then
//!   lowercased — `UserAccount` -> `user_account` but `OrderID` -> `orderid`, see the function doc).
//!   Documented
//!   approximation: plain JPA without Spring Boot keeps the name un-snaked; the Spring-focused scope of
//!   this crate (its route recognizer IS Spring MVC) makes the Boot default the honest pick, and
//!   `zzop_core::db_table_channel_casing` on both sides absorbs the leading-case half either way.
//!
//! Emitted as `IoProvide { kind: "db-table", key: "table:<casing(name)>", symbol: Some(<class name>) }`
//! — `symbol` carries the class name so the engine's `resolve_orm_entity_consumes` pass could key
//! entity-reference consumes against it (identical mechanism to the GORM/TypeORM provides). A CONSUME
//! side (e.g. `JpaRepository<User, Long>` repository declarations) is deliberately not built here —
//! disclosed roadmap, same one-side-at-a-time shape the SQL/Prisma provide-only arms ship with.
//!
//! Test surface is excluded by PATH (`zzop_core::is_test_file`) — same judgment as
//! `http_clients`'s module doc: Java tests live in `src/test/java/**`/`*Test(s).java` paths and the
//! language has no inline test idiom, so the path gate alone is sufficient.

use std::sync::OnceLock;

use regex::Regex;
use tree_sitter::Node;
use zzop_core::IoProvide;

use crate::util::{
    annotation_name, annotation_raw_args, annotations_of, line_of, modifiers_of, node_text,
    valid_named_children,
};

/// The JPA annotation packages the import gate accepts, as dotted prefixes — covers the exact class
/// imports (`jakarta.persistence.Entity`) and the package glob (`jakarta.persistence.*`) alike.
const JPA_PACKAGE_PREFIXES: &[&str] = &["jakarta.persistence.", "javax.persistence."];

/// Extract this file's JPA `db-table` provides — see module doc. Empty on parse failure, on a
/// test-classified path, and whenever the file imports no JPA package (never panics).
pub fn extract_jpa_db_table_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let gated = imports.values().any(|b| {
        JPA_PACKAGE_PREFIXES
            .iter()
            .any(|p| b.specifier.starts_with(p))
    });
    if !gated {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(tree.root_node(), rel, text, &mut out);
    out
}

/// Full recursion (nested entity classes reachable), matching only `class_declaration` — JPA maps
/// classes; records/interfaces/enums are not entity declarations.
fn walk(node: Node, rel: &str, src: &str, out: &mut Vec<IoProvide>) {
    if node.kind() == "class_declaration" {
        emit_entity(node, rel, src, out);
    }
    for child in valid_named_children(node) {
        walk(child, rel, src, out);
    }
}

fn emit_entity(class: Node, rel: &str, src: &str, out: &mut Vec<IoProvide>) {
    let mut entity_args: Option<Option<String>> = None; // outer None = @Entity absent
    let mut table_args: Option<Option<String>> = None; // outer None = @Table absent
    for ann in annotations_of(modifiers_of(class)) {
        match annotation_name(ann, src).as_deref() {
            Some("Entity") => entity_args = Some(annotation_raw_args(ann, src)),
            Some("Table") => table_args = Some(annotation_raw_args(ann, src)),
            _ => {}
        }
    }
    let Some(entity_args) = entity_args else {
        return; // not an entity — a bare @Table without @Entity maps nothing.
    };
    let Some(name_node) = class.child_by_field_name("name") else {
        return;
    };
    let class_name = node_text(name_node, src);

    let table = match table_args {
        Some(Some(args)) => match name_attr_state(&args) {
            NameAttr::Literal(name) => Some(name),
            NameAttr::NonLiteral => None, // renamed to something this pass cannot read — skip.
            NameAttr::Absent => default_table_name(entity_args.as_deref(), class_name),
        },
        // Marker `@Table` (no parens) or no `@Table` at all — the default naming path.
        _ => default_table_name(entity_args.as_deref(), class_name),
    };
    let Some(table) = table else {
        return;
    };
    out.push(IoProvide {
        response: None,
        kind: "db-table".to_string(),
        key: format!("table:{}", zzop_core::db_table_channel_casing(&table)),
        file: rel.to_string(),
        line: line_of(name_node),
        symbol: Some(class_name.to_string()),
        body: None,
    });
}

/// The default table name: `@Entity(name = "…")` literal when present (a non-literal one skips the
/// class — `None`), else the class's simple name; either way snaked per the module doc.
fn default_table_name(entity_args: Option<&str>, class_name: &str) -> Option<String> {
    let base = match entity_args.map(name_attr_state) {
        Some(NameAttr::Literal(name)) => name,
        Some(NameAttr::NonLiteral) => return None,
        Some(NameAttr::Absent) | None => class_name.to_string(),
    };
    Some(camel_to_snake(&base))
}

enum NameAttr {
    Literal(String),
    NonLiteral,
    Absent,
}

/// One annotation's raw args on the `name` axis — literal / present-but-non-literal / absent, the same
/// tri-state ladder `provides::annotations::route_path_state` walks for route paths.
fn name_attr_state(args: &str) -> NameAttr {
    if let Some(c) = name_attr_literal_re().captures(args) {
        return NameAttr::Literal(c[1].to_string());
    }
    if name_attr_re().is_match(args) {
        return NameAttr::NonLiteral;
    }
    NameAttr::Absent
}

/// `CamelCaseToUnderscoresNamingStrategy`'s ACTUAL algorithm (Hibernate's own, not an approximation):
/// walk positions `i` in `1..len-1` (the LAST character never takes a separator) and insert an
/// underscore before `name[i]` exactly when `name[i-1]` is lowercase, `name[i]` is uppercase AND
/// `name[i+1]` is lowercase (Java's `Character.isLowerCase`/`isUpperCase`, so a digit is neither and
/// never triggers the triple); then lowercase the whole result. `UserAccount` -> `user_account`,
/// `OrderItem` -> `order_item`, but `OrderID` -> `orderid` (the `D` after uppercase `I` fails the
/// triple), `UserDTO` -> `userdto`, `Line1Item` -> `line1item` (`1` is not lowercase), `UserA` ->
/// `usera` (last char excluded) and `HTTPLog` -> `httplog`.
fn camel_to_snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if i >= 1
            && i + 1 < chars.len()
            && chars[i - 1].is_lowercase()
            && c.is_uppercase()
            && chars[i + 1].is_lowercase()
        {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// A `name = "…"` attribute at an attribute boundary (start or just after `(`/`,`), literal RHS —
/// boundary-anchored like `provides::annotations::named_path_attr_re`, so a `name=` buried inside
/// another attribute's string value is never mistaken for the attribute.
fn name_attr_literal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?:^|[(,])\s*name\s*=\s*"([^"]*)""#).unwrap())
}

/// The boolean, quote-agnostic counterpart of [`name_attr_literal_re`] — detects a `name` attribute
/// whose RHS is NOT a quoted literal (a constant reference).
fn name_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:^|[(,])\s*name\s*=").unwrap())
}

#[cfg(test)]
mod tests;
