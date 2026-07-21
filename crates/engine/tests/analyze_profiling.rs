//! Tests for `EngineConfig::profile_rules` (per-rule wall-clock timing, the ESLint `TIMING=1` / oxlint
//! rule-timing equivalent). Profiling off is the
//! default, byte-for-byte unchanged behavior (`AnalyzeOutput::rule_timings == None`); profiling on adds
//! exactly one `RuleTiming` per enabled DSL rule id and per whole-graph native analysis id that actually
//! ran this call, without changing `findings`/`ir` at all (a differential test against an unprofiled run
//! over the identical fixture tree), sorted deterministically — verified structurally (nanos descending,
//! `rule_id` ascending tie-break) rather than by asserting exact `nanos` values, since raw wall-clock time
//! is not reproducible run-to-run (see `zzop_core::dsl::RuleTiming`'s own doc).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
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

/// The real `rules/dsl/be-security/be-security.json`, filtered to the three Java security-concern rules
/// (`sql-taint`/`weak-crypto`/`cmd-injection`) that moved into `be-security` when the language-named
/// `java-security` pack was dissolved (v0.15) — all three share a `.java`-ish `file_pattern`, so every one
/// of them gets evaluated, and therefore timed, against any `.java` file the pack applies to (see
/// `zzop_core::pack_loader::applies_to`'s "any rule matches" semantics). Filtering to exactly these three
/// keeps the timed-rule set deterministic (loading the full be-security pack would time every
/// `.java`-applicable rule in it). Resolved from `CARGO_MANIFEST_DIR` the same way `zzop_engine`'s own
/// crate tests do.
fn be_security_java_pack() -> RulePackDef {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl/be-security/be-security.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    // Goes through `zzop_core::parse_dsl_pack` (not a raw `serde_json::from_str`) so this pack's
    // `${NAME}` fragment refs resolve exactly like they do at real load time.
    let mut pack: RulePackDef = zzop_core::parse_dsl_pack(&text).expect("parse be-security.json");
    pack.rules
        .retain(|r| matches!(r.id.as_str(), "sql-taint" | "weak-crypto" | "cmd-injection"));
    pack
}

/// A circular TS import pair (exercises the `circular` native analysis id) plus a Java file matching
/// `be-security/sql-taint` (exercises a DSL finding; `weak-crypto`/`cmd-injection` still run against this
/// same file — they just don't fire, since the file has no weak-crypto/exec pattern). No git history is
/// configured, so `scores`/`health`/`recommendations`/`criticality`/`seams` never run (see
/// `analyze::assemble`'s git-gating) — this keeps the expected native-analysis-id set to exactly the
/// analyses `is_enabled`-gated unconditionally: `circular`/`unreachable`/`dead-candidates`/`dead-exports`/
/// `schema-usage`/`duplicate-route`/`route-shadowing`/`unprovided-consume`. This fixture has no HTTP
/// routes at all, so `unsafe-read-endpoint`/`non-idempotent-write`/`mutating-route-no-auth` — every rule gated
/// behind `run_callgraph_rules`'s `api_endpoints.is_empty()` early return — never run and never appear here.
fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-profiling-fixture");
    dir.write(
        "a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    dir.write(
        "b.ts",
        "import { a } from './a';\nexport function b() { return a(); }\n",
    );
    dir.write(
        "legacy/C.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\");\n  }\n}\n",
    );
    dir
}

fn config(profile_rules: bool) -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![be_security_java_pack()],
        profile_rules,
        ..EngineConfig::default()
    }
}

const EXPECTED_IDS: &[&str] = &[
    "circular",
    "unreachable",
    "dead-candidates",
    "dead-exports",
    "schema-usage",
    "duplicate-route",
    "route-shadowing",
    "unprovided-consume",
    "be-security/sql-taint",
    "be-security/weak-crypto",
    "be-security/cmd-injection",
];

#[test]
fn profiling_off_leaves_rule_timings_none() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(false));
    assert!(
        out.rule_timings.is_none(),
        "default (profile_rules: false) must leave rule_timings at None"
    );
}

#[test]
fn profiling_on_produces_exactly_one_timing_per_ran_rule_and_native_analysis() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(true));
    let timings = out.rule_timings.expect("profiling on -> Some(timings)");

    let mut ids: Vec<&str> = timings.iter().map(|t| t.rule_id.as_str()).collect();
    ids.sort();
    let mut expected: Vec<&str> = EXPECTED_IDS.to_vec();
    expected.sort();
    assert_eq!(ids, expected, "timings: {timings:?}");

    // "exactly once" per id -> no duplicate rule_id in the vector.
    let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "duplicate rule_id in {timings:?}");
}

#[test]
fn profiling_does_not_change_findings_or_ir_versus_an_unprofiled_run() {
    let dir = fixture_tree();
    let unprofiled = analyze_tree(dir.path(), &config(false));
    let profiled = analyze_tree(dir.path(), &config(true));

    assert_eq!(
        serde_json::to_value(&unprofiled.ir).unwrap(),
        serde_json::to_value(&profiled.ir).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&unprofiled.findings).unwrap(),
        serde_json::to_value(&profiled.findings).unwrap()
    );
    assert_eq!(unprofiled.degraded, profiled.degraded);
    assert_eq!(unprofiled.file_count, profiled.file_count);
    assert!(
        !profiled.findings.is_empty(),
        "sanity: the fixture tree should still produce findings (circular + sql-taint)"
    );
}

#[test]
fn rule_timings_are_sorted_nanos_descending_then_rule_id_ascending() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(true));
    let timings = out.rule_timings.expect("profiling on -> Some(timings)");
    assert_eq!(timings.len(), EXPECTED_IDS.len());
    for pair in timings.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            a.nanos > b.nanos || (a.nanos == b.nanos && a.rule_id < b.rule_id),
            "not sorted (nanos desc, rule_id asc tie-break): {a:?} appears before {b:?}"
        );
    }
}

#[test]
fn sql_taint_dsl_rule_timing_reflects_the_finding_it_produced() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(true));
    let timings = out.rule_timings.expect("profiling on -> Some(timings)");
    let sql_taint = timings
        .iter()
        .find(|t| t.rule_id == "be-security/sql-taint")
        .expect("sql-taint timing present");
    assert_eq!(sql_taint.findings, 1, "{sql_taint:?}");

    // weak-crypto/cmd-injection still ran (same pack, same file) but matched nothing in this fixture.
    for id in ["be-security/weak-crypto", "be-security/cmd-injection"] {
        let t = timings
            .iter()
            .find(|t| t.rule_id == id)
            .unwrap_or_else(|| panic!("expected a timing entry for {id}"));
        assert_eq!(t.findings, 0, "{t:?}");
    }
}
