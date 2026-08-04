//! Per-file IO projection: which `IoFacts` (route/egress/`db-table` provides and consumes) one already-
//! parsed file contributes, by dispatch language. Split out of `pipeline::fresh` — it was the single
//! largest block in `compute_fresh_artifact` and pushed that file past the 300-line source cap; nothing
//! about the logic changed in the move, and `fresh` remains the only caller.

use zzop_core::IoFacts;

use crate::dispatch::Language;
use crate::EngineConfig;

/// Egress consumes (TS/Python/Rust/Go/Java/C#) run only on a well-formed, in-size-cap file — hence the
/// `if !degraded` guards; the caller never calls this for an oversized file at all. Route provides come
/// io-direct for TS (Hono), Java (Spring), and C# (attribute controllers + minimal APIs); Java's + C#'s
/// provides project for ANY `.java`/`.cs` file regardless of `degraded` (extractors return empty rather
/// than guess — both gate only their consumes, inside their `extract_*_file_io` helpers). Python/Rust/Go
/// ROUTE provides travel as `router_mount_fragments` instead; Python (SQLModel/Django), Go (GORM),
/// Java (JPA `@Entity`/`@Table`) and C# (EF Core `DbSet<T>`/`[Table]`) ALSO emit io-direct `db-table`
/// provides, and Rust emits io-direct `db-table` CONSUMES (raw SQL statement strings, keyed at
/// extraction time). SQL (`CREATE TABLE`) and Prisma (`model` blocks) are provide-only `db-table` arms
/// with no consume side at all.
///
/// `prisma_io` is the one input that is not re-derived here: Prisma's io rides inside the same
/// `build_common_ir` result the caller already used for its symbols, so it is passed in rather than
/// re-extracted (see the `Language::Prisma` arm).
pub(super) fn project_file_io(
    language: Option<Language>,
    rel: &str,
    text: &str,
    degraded: bool,
    config: &EngineConfig,
    vocab: &crate::vocabulary::ResolvedVocabulary<'_>,
    prisma_io: Option<IoFacts>,
) -> Option<IoFacts> {
    match language {
        Some(Language::TypeScript) if !degraded => {
            crate::io::extract_file_io(rel, text, &config.io, vocab)
        }
        Some(Language::Java21) => crate::io::extract_java_file_io(rel, text, degraded),
        Some(Language::Python) if !degraded => {
            let mut consumes = zzop_parser_python_3::extract_python_http_consumes(rel, text);
            // ORM db-table facts (keyed engine-side): SQLModel/SQLAlchemy + Django — touches -> consumes, models -> provides.
            consumes.extend(zzop_parser_python_3::extract_sqlalchemy_db_table_consumes(
                rel, text,
            ));
            consumes.extend(zzop_parser_python_3::extract_django_db_table_consumes(
                rel, text,
            ));
            let mut provides =
                zzop_parser_python_3::extract_sqlalchemy_db_table_provides(rel, text);
            provides.extend(zzop_parser_python_3::extract_django_db_table_provides(
                rel, text,
            ));
            if consumes.is_empty() && provides.is_empty() {
                None
            } else {
                Some(IoFacts { provides, consumes })
            }
        }
        Some(Language::Rust) if !degraded => {
            let mut consumes = zzop_parser_rust::extract_rust_http_consumes(rel, text);
            // Raw-SQL `db-table` touches, keyed at extraction time (`table:<name>`) — no engine-side
            // entity resolution, unlike the GORM/SQLModel arms, because the statement names the physical
            // table itself. This is the arm that gave `.rs` a db channel at all: until it existed, a Rust
            // service's `migrations/*.sql` provides had no consumer anywhere in the tree.
            consumes.extend(zzop_parser_rust::extract_rust_raw_sql_db_table_consumes(
                rel, text,
            ));
            if consumes.is_empty() {
                None
            } else {
                Some(IoFacts {
                    provides: Vec::new(),
                    consumes,
                })
            }
        }
        Some(Language::Go) if !degraded => {
            let mut consumes = zzop_parser_go::extract_go_http_consumes(rel, text);
            // GORM db-table facts: model touches -> consumes (keyed engine-side), `gorm.Model` structs -> provides.
            consumes.extend(zzop_parser_go::extract_gorm_db_table_consumes(rel, text));
            let provides = zzop_parser_go::extract_gorm_db_table_provides(rel, text);
            if consumes.is_empty() && provides.is_empty() {
                None
            } else {
                Some(IoFacts { provides, consumes })
            }
        }
        Some(Language::Sql) => {
            let provides = zzop_parser_sql::extract_db_table_provides(rel, text);
            (!provides.is_empty()).then(|| IoFacts {
                provides,
                consumes: Vec::new(),
            })
        }
        // Prisma's `db-table` model PROVIDEs — the schema-side twin of the `Language::Sql` DDL arm right
        // above (same channel, same `zzop_core::db_table_channel_casing` key transform). Computed by the
        // caller's `parse_prisma` call rather than by an extractor of this module's own, since one
        // `build_common_ir` already yields both symbols and io; it is already `None` for a schema
        // declaring no models and `None` on the `catch_unwind` degrade, so no extra gating is needed.
        Some(Language::Prisma) => prisma_io,
        // C# projects BOTH provides (attribute controllers + minimal APIs, io-direct — no
        // `router_mount_fragments` arm) and `HttpClient` egress consumes. Unlike the egress-only arms
        // above, this is NOT gated `if !degraded`: `extract_csharp_file_io` runs its route-PROVIDES side
        // unconditionally (Java-parity), gating only the consumes internally — see its own doc.
        Some(Language::CSharp) => crate::io::extract_csharp_file_io(rel, text, degraded),
        _ => None,
    }
}
