//! S6: ORM-schema silence tripwire (db-table io channel) — flags a tree that imports/declares an ORM
//! whose schema this engine has NO native db-table extractor for, so the `db-table` io channel stays
//! completely dark with no honesty signal at all (the gap a live NestJS repo full of TypeORM `@Entity`
//! decorators exposed: zero db-table io facts extracted, zero warning).

use std::collections::{BTreeMap, BTreeSet};

/// ORM package/import specifiers this engine has NO native db-table extractor for, paired with the
/// human-readable ORM name the warning names. Exact-segment matched the same way `SERVER_FRAMEWORK_SPECIFIERS`
/// is (`is_orm_schema_specifier` below): TS/JS npm specifiers (`typeorm`, `sequelize`, `drizzle-orm`), Java's
/// JPA import families at the census's own first-two-dotted-segments grain (`jakarta.persistence`/
/// `javax.persistence` — see `drain_java_candidates`'s doc), Python's `sqlalchemy` (`from sqlalchemy import
/// ...` / `import sqlalchemy`), and Go's `gorm.io/gorm`. Prisma (`@prisma/client`) IS in the vocab
/// (added 2026-07-17, round-10 dogfood): this engine's native Prisma db-table path only recognizes the
/// `getPrisma()` accessor idiom (`extract_db_table_consumes`) — a repo using the bare-singleton
/// `prisma.<model>.<method>` style (the most common idiom; a documented staged follow-up) extracts ZERO
/// db-table facts, and excluding prisma here masked exactly that gap. The exact-zero-fact gate makes the
/// entry self-correcting: when the native path DOES extract facts, the count is nonzero and this tripwire
/// stays silent regardless of the import.
const ORM_SCHEMA_SPECIFIERS: &[(&str, &str)] = &[
    ("@prisma/client", "Prisma"),
    ("typeorm", "TypeORM"),
    ("sequelize", "Sequelize"),
    ("drizzle-orm", "Drizzle"),
    ("jakarta.persistence", "Jakarta Persistence (JPA)"),
    ("javax.persistence", "javax.persistence (JPA)"),
    ("sqlalchemy", "SQLAlchemy"),
    ("gorm.io/gorm", "GORM"),
];

/// Cap on example files listed per matched ORM in the warning — same "up to 3 example paths" convention
/// `server_framework_import_warning`'s sibling tripwires use.
const MAX_EXAMPLES: usize = 3;

/// Whether `specifier` names one of `ORM_SCHEMA_SPECIFIERS`, exact-segment matched: the specifier itself
/// equals the vocab entry, or is a subpath import of it in the npm slash-subpath form (`"typeorm/decorator"`
/// still counts as `typeorm`), the Java/Python dotted-subpath form (`"sqlalchemy.orm"` still counts as
/// `sqlalchemy`), or the Rust/Go `::`-subpath form (kept for defensive symmetry with the sibling tripwires'
/// matchers, though no current vocab entry needs it). Returns the human-readable ORM name on a match.
fn is_orm_schema_specifier(specifier: &str) -> Option<&'static str> {
    ORM_SCHEMA_SPECIFIERS.iter().find_map(|(vocab, name)| {
        let matched = specifier == *vocab
            || specifier.starts_with(&format!("{vocab}/"))
            || specifier.starts_with(&format!("{vocab}."))
            || specifier.starts_with(&format!("{vocab}::"));
        matched.then_some(*name)
    })
}

/// Returns a ready-to-push `warnings` entry when at least one ORM-schema package/import
/// (`ORM_SCHEMA_SPECIFIERS`) is present anywhere in the tree while `db_table_fact_count` (io provides PLUS
/// consumes of kind `db-table`, tree-wide) is EXACTLY zero. Unlike S1-S5's near-zero floor, this gate is
/// exact-zero, matching the observed failure signature verbatim ("zero db-table io facts, no warning") — any
/// nonzero count means SOME db-table fact came from somewhere (this engine's own Prisma path, an adapter
/// overlay, ...), so the channel is not dark and this tripwire stays silent (a Prisma repo whose native
/// parser/extractor actually produced db-table facts never fires this even though `@prisma/client` IS in
/// the vocab). Pure map lookup over `package_import_files` — no disk IO, so unconditional cost.
///
/// Determinism: `package_import_files` is a `BTreeMap<specifier, BTreeSet<importing file>>` (both levels
/// already sorted), and the matched-ORM map below is keyed by the (also sorted) human-readable name, so
/// iteration order and the example-file picks are deterministic without any extra sort here — same
/// convention as `server_framework_import_warning`/`client_library_import_warning`.
pub fn orm_schema_silence_warning(
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    db_table_fact_count: usize,
) -> Option<String> {
    if db_table_fact_count > 0 {
        return None;
    }
    let mut matched: BTreeMap<&'static str, Vec<&str>> = BTreeMap::new();
    for (specifier, files) in package_import_files {
        let Some(name) = is_orm_schema_specifier(specifier) else {
            continue;
        };
        let entry = matched.entry(name).or_default();
        for f in files {
            if entry.len() >= MAX_EXAMPLES {
                break;
            }
            entry.push(f.as_str());
        }
    }
    if matched.is_empty() {
        return None;
    }
    let orm_list = matched
        .iter()
        .map(|(name, files)| format!("{name} (e.g. {})", files.join(", ")))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "ORM schema marker(s) detected but zero db-table io facts were extracted tree-wide: {orm_list} — \
this engine has no native db-table extractor for this ORM, so its schema/table facts never reach the \
cross-layer join (`cross-layer/shared-db-table` and any join finding keyed on a table will be silent for \
this tree); project this tree's tables with a Mode B overlay adapter (see the adapter examples) to restore \
visibility: a partial envelope covering just the db-table channel is enough; contract: `zzop-mcp contract \
envelope-guide` on MCP hosts, docs/NORMALIZED_AST.md in the repo."
    ))
}
