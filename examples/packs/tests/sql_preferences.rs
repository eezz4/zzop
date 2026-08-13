//! `[[test]]` target for the EXPORTED `examples/packs/sql-preferences.json` pack.
//!
//! Second increment of the `axis: opinion` export, and the first whose tests were SHARED rather than
//! dedicated. `orm-eager` moved three whole files; here six rules left a pack of thirteen whose test
//! files judge both halves at once, so the split was surgery: a moved rule's coverage had to arrive
//! here without a staying rule's coverage leaving `rules/dsl/sql/`. Nothing was deleted — see
//! `examples/packs/README.md` for the rule this repo now follows on export.
//!
//! Wired from `rules/Cargo.toml` like every bundled pack; the only difference is the path it loads
//! (`../examples/packs` instead of `dsl`), because `CARGO_MANIFEST_DIR` is still `rules/`.
//!
//! ## One pack, and why that is the ending rather than the start
//!
//! Six rules moved here on 2026-08-12; FIVE stayed. `destructive-migration` went back to the bundle the
//! same day, and its departure is the reason this file once loaded two packs at a time. It was the
//! DISCLOSURE half of a handoff: `sql/delete-no-where`, `sql/update-no-where` and
//! `sql/truncate-in-app-code` exclude migration paths *because* it covers them, so exporting it left a
//! default run saying nothing about `migrations/` at all. Its `axis: opinion` judgment was never wrong —
//! the export decision was, and the two are separate questions.
//!
//! What remains here has no cross-pack sibling: every rule below is judged by this pack alone, so
//! [`scan`] is the only scan helper and a silence in these tests is this pack's own.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

mod aggregation;
mod language_scope;
mod query_logic_density;
mod select_like;
mod suppression;

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(prefix: &str) -> Self {
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

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn write(&self, rel: &str, content: &str) {
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

/// The exported pack, loaded through `load_dsl_packs` so its `${NAME}` fragment refs resolve exactly
/// as they do at real load time. It defines no fragments of its own since `destructive-migration` went
/// back to the bundle (`sql-where-veto` and `sql-bootstrap-drop-create` were that rule's, and went with
/// it), but every rule below still refs the SHARED `${test-paths}`/`${test-paths-stories}` vocabulary —
/// a raw `serde_json::from_str` would leave the literal `"${test-paths}"` in place, which is not a valid
/// regex and would silently no-op every affected rule. The `OnceLock` is the same one
/// `rules/dsl/sql/sql.rs` uses and for the same reason: the disk
/// load parses every pack in the directory and throws the rest away, and the clone SHARES the compiled
/// regex memo (`zzop_core::dsl::RegexCache`).
pub fn preferences_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(|| pack_from(Path::new("../examples/packs"), "sql-preferences"))
        .clone()
}

fn pack_from(rel: &Path, id: &str) -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors under {}: {:?}",
        dir.display(),
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no pack `{id}` under {}", dir.display()))
}

fn config(packs: Vec<RulePackDef>) -> EngineConfig {
    EngineConfig {
        source_id: "sql-preferences-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs,
        ..EngineConfig::default()
    }
}

/// Scan with the exported pack alone — the default, and what a user who retrieves only this pack gets.
pub fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config(vec![preferences_pack()]))
}

/// Findings of one of THIS pack's rules, by bare id.
pub fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a Finding> {
    with_prefix(out, "sql-preferences", rule)
}

fn with_prefix<'a>(out: &'a AnalyzeOutput, pack: &str, rule: &str) -> Vec<&'a Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("{pack}/{rule}"))
        .collect()
}
