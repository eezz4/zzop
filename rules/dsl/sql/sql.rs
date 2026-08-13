//! Exercises `rules/dsl/sql/sql.json`'s SQL/ORM-usage rule pack end-to-end via `zzop_engine::analyze_tree` so
//! `Matcher::MethodScan` rules run against real parser-derived `SourceSymbol` body spans. See `sql.json` for
//! each rule's exact matcher shape and message.
//!
//! ⚠ This pack was thirteen rules until 2026-08-12. Five of the six that declared `axis: opinion` —
//! `query-logic-density`, both `app-side-aggregation-*`, `select-star` and `like-leading-wildcard` —
//! left for `examples/packs/sql-preferences.json`, and their tests went with them
//! (`examples/packs/tests/`, wired as its own `[[test]]` target). Three of the modules here were
//! SHARED and were split rather than moved: `aggregation.rs`, `suppression.rs` and `language_scope.rs`
//! each kept the staying rules' fixtures and handed the rest over.
//!
//! The SIXTH, `destructive-migration`, was exported that same day and brought back the same day, and the
//! reason is worth keeping: its `axis: opinion` judgment was and is correct, but `delete-no-where`,
//! `update-no-where` and `truncate-in-app-code` exclude migration paths *because* it discloses them.
//! Exporting it turned that exclusion from a handoff into a silence — measured, not feared: a real
//! `DROP TABLE` on a populated table sat unreported in the dogfood corpus. It is the one bundled rule
//! that declares `opinion`, and that is the shape of the lesson: what axis a rule argues on and whether
//! a default run makes sense without it are two different questions.
//!
//! `race-condition-toctou` is a co-occurrence approximation: method-scan has no variable-binding
//! memory, so it doesn't verify the same variable is on both sides of the pattern (a guard/receiver
//! anywhere in the function body counts).
//!
//! Out of scope (a check that can't be expressed accurately ships as nothing, not half-right):
//! cache-invalidation-on-write (needs cross-file key-vocabulary resolution) and hardcoded-record-ref
//! detection (needs AST-structural object-literal traversal) — both beyond the DSL's four matcher shapes.
//!
//! Every rule's `// <marker>-ok:` suppression case is covered below, using the fixed "finding's own line
//! OR the single line directly above" window (`MARKER_LOOKBACK_LINES`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

mod aggregation;
mod destructive_migration;
mod language_scope;
mod no_where;
mod nplus1;
mod raw_sql_check_then_write;
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

/// Loads the real `sql.json` pack, co-located with this test file. Goes through
/// `zzop_core::parse_dsl_pack` (not a raw `serde_json::from_str`) so `${NAME}` fragment refs (the shared
/// test-path exclusions — `test-paths-migrations` included, promoted to the shared bundle 2026-08-03 —
/// plus this pack's own `sql-where-veto` fragment) resolve
/// exactly like they do at real load time — a raw struct deserialize would leave the literal
/// `"${sql-where-veto}"` string in place, which is not a valid regex and would silently no-op every
/// affected rule.
/// The pack this file's tests scan with. The disk load below parses EVERY pack JSON in the directory
/// and throws all but this one away, so doing it per test cost this binary that work once per test; the
/// `OnceLock` makes it once per binary. How many packs that is is not written here: it moved inside one
/// release (v0.30.0 exported a whole pack) and the sentence needs no size to make its point — the same
/// spelling `examples/packs/tests/sql_preferences.rs` already uses. The clone is cheap and — importantly
/// — SHARES the pack's compiled-regex memo (`zzop_core::dsl::RegexCache`), so the second test onward
/// also skips recompiling every pattern.
fn sql_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(sql_pack_uncached).clone()
}

fn sql_pack_uncached() -> RulePackDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl/sql/sql.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    zzop_core::parse_dsl_pack(&text).expect("parse sql.json")
}

fn config(packs: Vec<RulePackDef>) -> EngineConfig {
    EngineConfig {
        source_id: "sql-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs,
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config(vec![sql_pack()]))
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("sql/{rule}"))
        .collect()
}
