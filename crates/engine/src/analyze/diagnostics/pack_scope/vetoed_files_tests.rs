//! The FILE axis of the applicability census: a file every targeting rule vetoes.
//!
//! Unit-level because the subject is a pure function of `(loaded packs, rel list, exclude config)` —
//! the same pair `compute_dsl_scope` is a pure function of. An end-to-end fixture would add a
//! filesystem walk and a parser to a question neither of them can answer differently.

use super::{compute_dsl_scope, pack_scope_warnings};
use crate::EngineConfig;
use zzop_core::{GlobalExclude, RuleConfig, RulePackDef};

/// A pack whose single rule targets every `.ts` file and then vetoes the test-path ones — the shape the
/// overwhelming majority of bundled rules ship (`file_exclude_pattern: "${test-paths}"`), reduced to one
/// rule. A FIXTURE pack, not the bundled set, and deliberately so: WHICH bundled files this can happen
/// to is a moving property of the shipped packs, and the subject module's doc owns that measurement with
/// the recount command beside it. Neither side's size is retyped here — both were written as `134` and
/// `144` and both were false the moment v0.30.0 exported 17 rules. Testing the MECHANISM keeps this pin
/// off a property that is free to change.
fn ts_pack_vetoing_tests() -> RulePackDef {
    serde_json::from_str(
        r#"{
          "id": "ts-probe",
          "schema_version": 1,
          "rules": [{
            "id": "ts-todo",
            "severity": "warning",
            "message": "A TODO comment.",
            "matcher": {
              "type": "line-scan",
              "file_pattern": "\\.ts$",
              "file_exclude_pattern": "\\.test\\.ts$",
              "line_pattern": "TODO"
            }
          }]
        }"#,
    )
    .expect("the fixture pack must parse")
}

/// A second rule with the SAME `file_pattern` and NO exclude: one rule that admits the file is enough
/// to make it judged, which is what the report must be invalidated by.
fn ts_pack_admitting_everything() -> RulePackDef {
    serde_json::from_str(
        r#"{
          "id": "ts-probe-open",
          "schema_version": 1,
          "rules": [{
            "id": "ts-fixme",
            "severity": "warning",
            "message": "A FIXME comment.",
            "matcher": { "type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "FIXME" }
          }]
        }"#,
    )
    .expect("the fixture pack must parse")
}

fn config_with(packs: Vec<RulePackDef>, global_excludes: Vec<GlobalExclude>) -> EngineConfig {
    EngineConfig {
        packs,
        rule_config: RuleConfig {
            global_excludes,
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    }
}

/// The pinned head of the report, for a run that counted exactly one file. A `fn`, not a `const`:
/// `scripts/policy-census.txt` censuses every `const NAME: &str` under `crates/engine/src` as a policy
/// name, and a test fixture string is not one.
fn vetoed_head() -> &'static str {
    "1 file(s) match a loaded DSL rule's `file_pattern`"
}

fn warnings_for(
    rels: &[&str],
    packs: Vec<RulePackDef>,
    excludes: Vec<GlobalExclude>,
) -> Vec<String> {
    let config = config_with(packs, excludes);
    let scope = compute_dsl_scope(&config.packs, rels, &config.dispatch);
    pack_scope_warnings(&config, &scope)
}

/// THE GAP. `src/a.test.ts` matches the rule's `file_pattern`, so the census counts it in
/// `files_in_scope` (and `packsLoaded[].filesInScope` publishes that count) — but the same rule's
/// `file_exclude_pattern` vetoes it, so not one DSL rule ran on it. Nothing said so before this report.
#[test]
fn a_file_every_targeting_rule_vetoes_is_reported() {
    let rels = ["src/a.test.ts", "src/b.ts"];
    let scope = compute_dsl_scope(
        &[ts_pack_vetoing_tests()],
        &rels,
        &crate::DispatchConfig::default(),
    );
    // The premise, asserted rather than assumed: the vetoed file IS counted as in scope.
    assert_eq!(
        scope.files_in_scope_by_pack,
        vec![2],
        "both .ts files match the rule's file_pattern, so the pack-level count is 2"
    );
    assert!(scope.in_scope_rels.contains("src/a.test.ts"));

    let hits = warnings_for(&rels, vec![ts_pack_vetoing_tests()], Vec::new());
    let hit = hits
        .iter()
        .find(|w| w.starts_with(vetoed_head()))
        .unwrap_or_else(|| panic!("expected the vetoed-file self-report, got: {hits:?}"));
    assert!(
        hit.contains("src/a.test.ts"),
        "the report must name the file it counted: {hit}"
    );
    assert!(
        !hit.contains("src/b.ts"),
        "src/b.ts was admitted and judged — it must not appear: {hit}"
    );
}

/// INVALIDATION. One rule with the same `file_pattern` and no exclude admits the file, so it was
/// judged and the report must vanish. Without this a report that fired unconditionally would pass.
#[test]
fn one_rule_without_a_veto_silences_the_report() {
    let hits = warnings_for(
        &["src/a.test.ts", "src/b.ts"],
        vec![ts_pack_vetoing_tests(), ts_pack_admitting_everything()],
        Vec::new(),
    );
    assert!(
        !hits.iter().any(|w| w.starts_with(vetoed_head())),
        "a second rule admits the file — nothing fell out of the flow: {hits:?}"
    );
}

/// A file the caller ALREADY declared in `exclude` is accounted for: `scoring_scope_warning` owns it,
/// and re-reporting a decision the reader made is the noise this channel refuses to become. This is
/// the "everything except what you already excluded" half of the reconciliation, in the only place it
/// is computable.
#[test]
fn a_file_the_caller_excluded_is_not_reported() {
    let hits = warnings_for(
        &["src/a.test.ts", "src/b.ts"],
        vec![ts_pack_vetoing_tests()],
        vec![GlobalExclude {
            path: None,
            glob: Some("**/*.test.ts".to_string()),
        }],
    );
    assert!(
        !hits.iter().any(|w| w.starts_with(vetoed_head())),
        "the caller declared this file in `exclude` — it is accounted for: {hits:?}"
    );
}

/// A file no rule's `file_pattern` matches at all is OUT OF SCOPE, not vetoed. Counting it here would
/// be the "118 skips whose examples were .md and .png" failure, one report over.
#[test]
fn an_out_of_scope_file_is_never_counted() {
    let hits = warnings_for(
        &["docs/notes.md", "assets/logo.png", "src/b.ts"],
        vec![ts_pack_vetoing_tests()],
        Vec::new(),
    );
    assert!(
        !hits.iter().any(|w| w.starts_with(vetoed_head())),
        "no rule targets .md/.png — they lost no coverage: {hits:?}"
    );
}

/// With no packs loaded nothing is in scope, so nothing can be vetoed out of it — `zero_packs_warning`
/// owns that disclosure.
#[test]
fn no_packs_loaded_leaves_the_report_silent() {
    let hits = warnings_for(&["src/a.test.ts"], Vec::new(), Vec::new());
    assert!(
        !hits.iter().any(|w| w.starts_with(vetoed_head())),
        "no packs loaded — the report must be silent: {hits:?}"
    );
}

/// ONE aggregate entry with a bounded sample, never one line per file: the cap is named in the message
/// and the remainder is disclosed as `+N more`.
#[test]
fn many_vetoed_files_produce_one_line_with_a_capped_sample() {
    let rels: Vec<String> = (0..7).map(|i| format!("src/m{i}.test.ts")).collect();
    let rel_refs: Vec<&str> = rels.iter().map(String::as_str).collect();
    let hits = warnings_for(&rel_refs, vec![ts_pack_vetoing_tests()], Vec::new());
    let head = "7 file(s) match a loaded DSL rule's `file_pattern`";
    let matching: Vec<&String> = hits.iter().filter(|w| w.starts_with(head)).collect();
    assert_eq!(matching.len(), 1, "exactly one aggregate line: {hits:?}");
    let hit = matching[0];
    assert!(
        hit.contains("src/m0.test.ts, src/m1.test.ts, src/m2.test.ts, +4 more"),
        "sorted, capped at 3, remainder disclosed: {hit}"
    );
    // The out-of-scope note this report is NOT allowed to make: it closes the FILE axis only.
    assert!(
        hit.contains("require_file"),
        "the message must say the content-gate axis stays open: {hit}"
    );
}
