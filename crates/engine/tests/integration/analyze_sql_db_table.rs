//! End-to-end coverage for the SQL DDL `db-table` provide channel (`zzop_parser_sql`, wired into
//! `dispatch`/`pipeline::compute_fresh_artifact` — see those modules' own docs for the wiring): a `.sql`
//! migration file's `CREATE TABLE` statements must reach `AnalyzeOutput::ir.ir.io.provides` as `db-table`
//! facts, `AnalyzeOutput::coverage.io_provides` must count them, and — the whole point of wiring `.sql`
//! into `dispatch` at all — the "bring an adapter" per-extension disclosure
//! (`analyze::diagnostics::unparsed_extension_warning`) must NOT name `.sql` anymore (it used to, see
//! `analyze_unparsed_extensions.rs`'s own note on why that fixture moved off `.sql`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "sql-db-table-fixture".to_string(),
        ..EngineConfig::default()
    }
}

/// A small Flyway-style migration tree: one `.sql` migration declaring several tables (plain, `IF NOT
/// EXISTS`, and schema-qualified — the realistic shapes this crate's own unit tests already pin one at a
/// time), plus a native `.ts` file so the tree isn't SQL-only.
fn migration_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-sql-db-table");
    dir.write(
        "db/migrations/V1__init.sql",
        concat!(
            "CREATE TABLE IF NOT EXISTS users (\n",
            "  id BIGINT PRIMARY KEY,\n",
            "  email VARCHAR(255) NOT NULL\n",
            ");\n",
            "\n",
            "CREATE TABLE public.orders (\n",
            "  id BIGINT PRIMARY KEY,\n",
            "  user_id BIGINT NOT NULL\n",
            ");\n",
        ),
    );
    dir.write("src/app.ts", "export const noop = () => 1;\n");
    dir
}

#[test]
fn create_table_statements_become_db_table_provides_in_the_assembled_ir() {
    let dir = migration_tree();
    let out = analyze_tree(dir.path(), &config());

    let io = out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected non-empty io facts (the .sql migration provides db-table facts)");
    let db_table_keys: Vec<&str> = io
        .provides
        .iter()
        .filter(|p| p.kind == "db-table")
        .map(|p| p.key.as_str())
        .collect();
    assert!(
        db_table_keys.contains(&"table:users"),
        "expected table:users among db-table provides, got: {db_table_keys:?}"
    );
    assert!(
        db_table_keys.contains(&"table:orders"),
        "expected table:orders among db-table provides (schema-qualified public.orders, bare-named), \
         got: {db_table_keys:?}"
    );

    // Both provides are attributed to the migration file, at the CREATE TABLE line.
    let users_provide = io
        .provides
        .iter()
        .find(|p| p.kind == "db-table" && p.key == "table:users")
        .unwrap();
    assert_eq!(users_provide.file, "db/migrations/V1__init.sql");
    assert_eq!(users_provide.line, 1);
    let orders_provide = io
        .provides
        .iter()
        .find(|p| p.kind == "db-table" && p.key == "table:orders")
        .unwrap();
    assert_eq!(orders_provide.line, 6);
}

#[test]
fn coverage_census_counts_the_sql_provides() {
    let dir = migration_tree();
    let out = analyze_tree(dir.path(), &config());
    assert!(
        out.coverage.io_provides >= 2,
        "expected coverage.io_provides to count both db-table facts, got: {}",
        out.coverage.io_provides
    );
    assert!(
        !out.coverage.join_contribution_zero,
        "a tree with real db-table provides must not be flagged join-contribution-zero"
    );
}

#[test]
fn sql_extension_no_longer_appears_in_the_unparsed_extension_warning() {
    let dir = migration_tree();
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.warnings.iter().any(|w| w.contains("extension .sql")),
        "wiring .sql into dispatch must make it disappear from the \"no native parser\" disclosure \
         automatically (it derives from dispatch, not a separate list): {:?}",
        out.warnings
    );
}

#[test]
fn a_non_ddl_sql_file_contributes_no_provides_and_is_not_degraded() {
    let dir = TempDir::new("zzop-engine-sql-non-ddl");
    dir.write(
        "seed.sql",
        "SELECT * FROM users;\nINSERT INTO users (id) VALUES (1);\n",
    );
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.degraded.contains(&"seed.sql".to_string()),
        "a non-DDL .sql file is well-formed for this scanner (zero matches, not a parse failure): {:?}",
        out.degraded
    );
    let has_seed_provide = out
        .ir
        .ir
        .io
        .as_ref()
        .is_some_and(|io| io.provides.iter().any(|p| p.file == "seed.sql"));
    assert!(
        !has_seed_provide,
        "a non-DDL .sql file must contribute zero db-table provides"
    );
}

/// A TypeORM tree: an `@Entity('article')` class in one file provides `table:article` (carrying the class
/// name), and a service `@InjectRepository(ArticleEntity)` in another file consumes it. The parser can't
/// key the consume (it references the CLASS, not the table string), so the engine's
/// `resolve_orm_entity_consumes` pass must fill it in from the entity index — end-to-end proof of the
/// `typeorm-repo-consume` feature.
fn typeorm_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-typeorm");
    dir.write(
        "src/article/article.entity.ts",
        "@Entity('article')\nexport class ArticleEntity {}\n",
    );
    dir.write(
        "src/article/article.service.ts",
        concat!(
            "import { Repository } from 'typeorm';\n",
            "import { ArticleEntity } from './article.entity';\n",
            "export class ArticleService {\n",
            "  constructor(\n",
            "    @InjectRepository(ArticleEntity)\n",
            "    private readonly repo: Repository<ArticleEntity>,\n",
            "  ) {}\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn typeorm_repository_consume_resolves_to_its_entity_table_key() {
    let dir = typeorm_tree();
    let out = analyze_tree(dir.path(), &config());
    let io = out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected io facts from the TypeORM tree");

    // The @Entity provide carries the class name so the resolver can key the consume.
    let entity_provide = io
        .provides
        .iter()
        .find(|p| p.kind == "db-table" && p.key == "table:article")
        .expect("expected a table:article db-table provide from @Entity('article')");
    assert_eq!(entity_provide.symbol.as_deref(), Some("ArticleEntity"));

    // The repository consume, unkeyable at parse time, is resolved to the entity's table key.
    let db_consumes: Vec<&zzop_core::IoConsume> = io
        .consumes
        .iter()
        .filter(|c| c.kind == "db-table")
        .collect();
    assert_eq!(
        db_consumes.len(),
        1,
        "expected one db-table consume, got: {db_consumes:?}"
    );
    assert_eq!(
        db_consumes[0].key.as_deref(),
        Some("table:article"),
        "the @InjectRepository(ArticleEntity) consume must resolve to table:article"
    );
    assert_eq!(db_consumes[0].raw.as_deref(), Some("ArticleEntity"));
}
