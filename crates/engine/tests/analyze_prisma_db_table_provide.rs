//! End-to-end coverage for the Prisma schema `db-table` PROVIDE channel — the schema-side twin of
//! `analyze_sql_db_table.rs`'s DDL channel.
//!
//! Two things are pinned here, both of which used to be untrue:
//! 1. **The provide reaches the assembled IR at all.** `zzop_parser_prisma::build_common_ir` always
//!    computed a `db-table` `IoProvide` per model, but the engine's sole call site
//!    (`pipeline::parsers::parse_prisma`) discarded `ir.ir.io` — a computed-then-dropped orphan
//!    documented by `rule_contracts/capability_matrix.rs` and now wired through
//!    (`pipeline::fresh`'s `Language::Prisma` io arm). A `model` block's table now joins the cross-layer
//!    `db-table` channel exactly like a `CREATE TABLE`'s does, which is what makes the
//!    `prisma.<accessor>` CONSUME side (`zzop_parser_typescript::adapters::db_table_consume`) join
//!    something inside its OWN tree instead of dangling.
//! 2. **`@@map` no longer splits the channel.** A `@@map`ed model provides BOTH its accessor key (what
//!    the Prisma client spells) AND its physical table key (what the DDL / a non-Prisma ORM spells) —
//!    see `zzop_parser_prisma::analysis::db_table_provide_keys` for why both rather than a swap.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, analyze_trees, EngineConfig};

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
        source_id: "prisma-db-table-fixture".to_string(),
        ..EngineConfig::default()
    }
}

fn db_table_provide_keys(out: &zzop_engine::AnalyzeOutput) -> Vec<String> {
    out.ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            io.provides
                .iter()
                .filter(|p| p.kind == "db-table")
                .map(|p| p.key.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The plain shape: two models, no `@@map`, plus a bare-singleton Prisma client call site so the tree
/// carries a real CONSUME on the same channel.
fn plain_schema_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-prisma-provide");
    dir.write(
        "prisma/schema.prisma",
        concat!(
            "// datasource omitted (irrelevant to this parser)\n",
            "model Article {\n",
            "  id String @id\n",
            "}\n",
            "\n",
            "model UserProfile {\n",
            "  id String @id\n",
            "}\n",
        ),
    );
    dir.write(
        "src/prisma/prisma-client.ts",
        "import { PrismaClient } from '@prisma/client';\nexport const prisma = new PrismaClient();\n",
    );
    dir.write(
        "src/article.service.ts",
        concat!(
            "import { prisma } from './prisma/prisma-client';\n",
            "export function listArticles() { return prisma.article.findMany(); }\n",
        ),
    );
    dir
}

#[test]
fn schema_models_become_db_table_provides_in_the_assembled_ir() {
    let dir = plain_schema_tree();
    let out = analyze_tree(dir.path(), &config());

    let keys = db_table_provide_keys(&out);
    assert!(
        keys.contains(&"table:article".to_string()),
        "expected table:article among db-table provides (model Article), got: {keys:?}"
    );
    assert!(
        keys.contains(&"table:userProfile".to_string()),
        "expected table:userProfile (multi-word PascalCase lower-firsts on the first char only), \
         got: {keys:?}"
    );

    let io = out.ir.ir.io.as_ref().expect("io facts expected");
    let article = io
        .provides
        .iter()
        .find(|p| p.kind == "db-table" && p.key == "table:article")
        .unwrap();
    assert_eq!(article.file, "prisma/schema.prisma");
    assert_eq!(
        article.line, 2,
        "the provide sits at the model's declaration line"
    );
    assert_eq!(
        article.symbol, None,
        "a Prisma model provide carries no class symbol — `resolve_orm_entity_consumes` indexes only \
         provides that DO, so this must never start feeding that resolver"
    );
}

#[test]
fn a_model_without_at_map_provides_exactly_one_key() {
    // Pins the UNCHANGED single-key behavior for the ordinary model: the `@@map` work below is strictly
    // additive, and a plain model must never start emitting a second (invented) key.
    let dir = plain_schema_tree();
    let out = analyze_tree(dir.path(), &config());
    let mut keys = db_table_provide_keys(&out);
    keys.sort();
    assert_eq!(keys, vec!["table:article", "table:userProfile"]);
}

#[test]
fn the_prisma_accessor_consume_joins_the_schema_provide_inside_its_own_tree() {
    // The point of the wiring: `prisma.article.findMany()` keys `table:article`, and the schema's own
    // model now provides it — so the linker emits an edge instead of parking the consume in
    // `unprovided_consumes`.
    let dir = plain_schema_tree();
    let trees = vec![(dir.path().to_path_buf(), config())];
    let out = analyze_trees(&trees);

    let joined: Vec<&str> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "db-table")
        .map(|e| e.key.as_str())
        .collect();
    assert!(
        joined.contains(&"table:article"),
        "expected a db-table edge on table:article (consume src/article.service.ts -> provide \
         prisma/schema.prisma), got edges: {:?}",
        out.cross_layer.edges
    );
    assert!(
        !out.cross_layer
            .unprovided_consumes
            .iter()
            .any(|c| c.consume.kind == "db-table"),
        "no db-table consume should dangle once the schema provides its table: {:?}",
        out.cross_layer.unprovided_consumes
    );
}

/// The S6 ORM-schema-silence tripwire (`framework_silence::orm_schema_silence`) counts db-table
/// provides PLUS consumes and fires only at EXACTLY zero. Wiring the schema provides therefore silences
/// it for any tree that ships a `schema.prisma` — including one whose CONSUME idiom this engine does not
/// recognize, which is a DELIBERATE narrowing pinned here rather than left to be discovered.
///
/// Why it is the right verdict, not a lost signal: S6's own message says the tree's "schema/table facts
/// do not reach the cross-layer join", and after this wiring they do — every model's table is joinable.
/// This is exactly how S6 already behaves for TypeORM (an `@Entity` provide with no recognized
/// repository consume silences it too), so Prisma now follows the same rule instead of a special one.
/// A consume-side extraction gap is a different claim than "the channel is dark", and S6 is the
/// darkness tripwire.
#[test]
fn a_schema_only_prisma_tree_silences_the_s6_orm_schema_tripwire() {
    const S6_WARNING_SUBSTRING: &str = "ORM schema marker(s) detected but zero db-table io facts";
    let dir = TempDir::new("zzop-engine-prisma-s6");
    dir.write(
        "prisma/schema.prisma",
        "model Article {\n  id String @id\n}\n",
    );
    // `@prisma/client` IS in S6's vocabulary, and this call shape carries no evidence `db` is a Prisma
    // client, so the CONSUME side extracts nothing — provides alone are what keep the count nonzero.
    dir.write(
        "src/article.service.ts",
        concat!(
            "import { PrismaClient } from '@prisma/client';\n",
            "export function listArticles(db) { return db.article.findMany(); }\n",
        ),
    );
    let out = analyze_tree(dir.path(), &config());

    let io = out.ir.ir.io.as_ref().expect("io facts expected");
    assert!(
        io.consumes.iter().all(|c| c.kind != "db-table"),
        "fixture precondition: this shape must extract no db-table consume, got: {:?}",
        io.consumes
    );
    assert!(
        io.provides.iter().any(|p| p.kind == "db-table"),
        "the schema's own provide is what makes the count nonzero"
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains(S6_WARNING_SUBSTRING)),
        "a tree whose schema tables DO reach the join must not be told its db-table channel is dark: \
         {:?}",
        out.warnings
    );
}

/// The `@@map` shape: the model is `Article` (client accessor `prisma.article`) but the physical table
/// is `articles`, declared by a migration in the same tree.
fn at_map_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-prisma-atmap");
    dir.write(
        "prisma/schema.prisma",
        concat!(
            "model Article {\n",
            "  id String @id\n",
            "\n",
            "  @@map(\"articles\")\n",
            "}\n",
        ),
    );
    dir.write(
        "db/migrations/V1__init.sql",
        "CREATE TABLE articles (id TEXT PRIMARY KEY);\n",
    );
    dir.write(
        "src/prisma/prisma-client.ts",
        "import { PrismaClient } from '@prisma/client';\nexport const prisma = new PrismaClient();\n",
    );
    dir.write(
        "src/article.service.ts",
        concat!(
            "import { prisma } from './prisma/prisma-client';\n",
            "export function listArticles() { return prisma.article.findMany(); }\n",
        ),
    );
    dir
}

#[test]
fn an_at_mapped_model_provides_both_its_accessor_key_and_its_physical_table_key() {
    let dir = at_map_tree();
    let out = analyze_tree(dir.path(), &config());
    let io = out.ir.ir.io.as_ref().expect("io facts expected");

    let schema_keys: Vec<&str> = io
        .provides
        .iter()
        .filter(|p| p.kind == "db-table" && p.file == "prisma/schema.prisma")
        .map(|p| p.key.as_str())
        .collect();
    assert_eq!(
        schema_keys,
        vec!["table:article", "table:articles"],
        "a @@map'ed model declares TWO identities — the client accessor (joins the .ts consume) and the \
         physical table (joins the DDL / a non-Prisma ORM) — and emits both, accessor first"
    );

    // The physical key is byte-identical to what the SQL side independently produced for the same
    // table, which is the join this closes.
    let sql_key = io
        .provides
        .iter()
        .find(|p| p.kind == "db-table" && p.file.ends_with(".sql"))
        .map(|p| p.key.clone())
        .expect("the migration must provide a db-table key");
    assert_eq!(
        sql_key, "table:articles",
        "parser-sql keys the DDL name; the @@map-derived prisma key must equal it or the two layers \
         ride different keys for one physical table"
    );
}

#[test]
fn the_at_map_second_key_does_not_break_the_accessor_join() {
    // The regression the "emit BOTH" choice exists to prevent: swapping to the @@map key alone would
    // strand every `prisma.<accessor>` consume. The accessor edge must survive.
    let dir = at_map_tree();
    let trees = vec![(dir.path().to_path_buf(), config())];
    let out = analyze_trees(&trees);

    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.kind == "db-table" && e.key == "table:article"),
        "the accessor-keyed consume must still join the model's accessor provide, got: {:?}",
        out.cross_layer.edges
    );
    assert!(
        !out.cross_layer
            .unprovided_consumes
            .iter()
            .any(|c| c.consume.kind == "db-table"),
        "got dangling db-table consumes: {:?}",
        out.cross_layer.unprovided_consumes
    );
}
