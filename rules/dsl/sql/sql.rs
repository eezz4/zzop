//! Exercises `rules/dsl/sql/sql.json`'s SQL/ORM-usage rule pack end-to-end via `zzop_engine::analyze_tree` so
//! `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans. See `sql.json` for
//! each rule's exact matcher shape and message.
//!
//! `query-logic-density` counts CASE-WHEN branches within one SQL literal via a whole-file `require_file`
//! gate (an SQL anchor keyword plus two `WHEN`s) paired with a `line_pattern` on the literal's `CASE` line,
//! since `Matcher::LineScan` has no cross-line aggregation.
//!
//! `app-side-aggregation-reduce`/`-filter-length` and `race-condition-toctou` are co-occurrence
//! approximations: method-scan has no variable-binding memory, so they don't verify the same variable is
//! on both sides of the pattern (a guard/receiver anywhere in the function body counts).
//!
//! Out of scope (a check that can't be expressed accurately ships as nothing, not half-right):
//! cache-invalidation-on-write (needs cross-file key-vocabulary resolution) and hardcoded-record-ref
//! detection (needs AST-structural object-literal traversal) — both beyond the DSL's four matcher shapes.
//!
//! Every rule's `// <marker>-ok:` suppression case is covered below, using the fixed "finding's own line
//! OR the single line directly above" window (`MARKER_LOOKBACK_LINES`). `destructive-migration` also
//! recognizes a `--`-comment marker in `.sql` files specifically (`dsl.rs::is_sql_file`), since real
//! migrations of this pack's target class are commonly raw `.sql`, not `.ts`/`.js`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

mod aggregation;
mod destructive_migration;
mod no_where;
mod nplus1;
mod query_logic_density;
mod select_like;
mod suppression;
mod toctou;
mod truncate;

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

/// Loads the real `sql.json` pack, co-located with this test file.
fn sql_pack() -> RulePackDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl/sql/sql.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("parse sql.json")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "sql-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs: vec![sql_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("sql/{rule}"))
        .collect()
}
