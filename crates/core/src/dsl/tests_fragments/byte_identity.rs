//! Real-`rules/dsl`-tree guards for the fragment mechanism — see the `tests_fragments` module doc. These
//! load the actual committed pack files (never a synthetic fixture), proving the sentinel is used only as
//! an intended whole-value reference, that the whole tree resolves cleanly, and that the migration left
//! two non-`sql` packs' loaded `RulePackDef` byte-identical to their pre-migration form.

use std::path::{Path, PathBuf};

use super::super::def::{Matcher, RuleDef, RulePackDef};
use super::super::fragments::fragment_ref_name;
use crate::{load_dsl_packs, parse_dsl_pack};

fn real_dsl_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl")
}

/// Every field `expand_fragments` treats as a `${NAME}`-eligible pattern — the EXACT same field set, so
/// this guard can never drift from what the mechanism actually resolves. Deliberately narrower than "every
/// string in the JSON file": `message`/`id`/`suppress_marker`/`label` legitimately mention `${...}` as
/// PROSE (e.g. a message explaining what a template-literal placeholder looks like) — those are not
/// pattern fields and must not be flagged.
fn pattern_bearing_field_values(rule: &RuleDef) -> Vec<(&'static str, &str)> {
    let mut out = Vec::new();
    match &rule.matcher {
        Matcher::LineScan(m) => {
            out.push(("file_pattern", m.file_pattern.as_str()));
            if let Some(p) = &m.require_file {
                out.push(("require_file", p.as_str()));
            }
            for p in &m.require_file_all {
                out.push(("require_file_all", p.as_str()));
            }
            for p in &m.require_file_absent {
                out.push(("require_file_absent", p.as_str()));
            }
            if let Some(p) = &m.line_pattern {
                out.push(("line_pattern", p.as_str()));
            }
            for lp in m.any.iter().flatten() {
                out.push(("any[].pattern", lp.pattern.as_str()));
            }
            if let Some(p) = &m.exclude_pattern {
                out.push(("exclude_pattern", p.as_str()));
            }
            if let Some(p) = &m.file_exclude_pattern {
                out.push(("file_exclude_pattern", p.as_str()));
            }
        }
        Matcher::MethodScan(m) => {
            out.push(("file_pattern", m.file_pattern.as_str()));
            if let Some(p) = &m.require_file {
                out.push(("require_file", p.as_str()));
            }
            for p in &m.require_file_all {
                out.push(("require_file_all", p.as_str()));
            }
            for p in &m.require_file_absent {
                out.push(("require_file_absent", p.as_str()));
            }
            for lp in &m.patterns {
                out.push(("patterns[].pattern", lp.pattern.as_str()));
            }
            for lp in &m.absent {
                out.push(("absent[].pattern", lp.pattern.as_str()));
            }
            if let Some(p) = &m.file_exclude_pattern {
                out.push(("file_exclude_pattern", p.as_str()));
            }
        }
        Matcher::SymbolScan(m) => {
            out.push(("file_pattern", m.file_pattern.as_str()));
            if let Some(p) = &m.name_pattern {
                out.push(("name_pattern", p.as_str()));
            }
        }
        Matcher::IoScan(m) => {
            out.push(("file_pattern", m.file_pattern.as_str()));
            if let Some(p) = &m.file_exclude_pattern {
                out.push(("file_exclude_pattern", p.as_str()));
            }
            if let Some(p) = &m.key_pattern {
                out.push(("key_pattern", p.as_str()));
            }
        }
    }
    out
}

/// Part 1, guard #4: no shipped `rules/dsl/**` pattern-bearing field contains the `${...}` sentinel shape
/// except as an intended, total, whole-value fragment reference. Loads every real pack's RAW (pre-
/// expansion) text — a plain `serde_json::from_str`, deliberately NOT `parse_dsl_pack`, so a sentinel is
/// still visible for this check to see — and asserts every `pattern_bearing_field_values` entry containing
/// the substring `"${"` is EXACTLY a whole-value `${NAME}` reference (`fragment_ref_name` returns `Some`),
/// never a partial/substring occurrence. `message`/`id`/`suppress_marker`/`label` are out of scope (see
/// `pattern_bearing_field_values`'s doc) — `be-security/shell-exec` and `sql/sql-delete-no-where`'s
/// messages legitimately describe `${...}` template-literal/placeholder syntax in prose. Paired with
/// `real_dsl_tree_loads_with_zero_errors` below, which proves every real reference actually resolves (an
/// unknown name is a hard load error, not a silent skip) — together the two prove expansion is total AND
/// unambiguous, per this guard's own charter.
#[test]
fn no_shipped_pattern_contains_the_sentinel_except_as_an_intended_whole_value_ref() {
    let dir = real_dsl_dir();
    let mut total_refs = 0usize;
    let mut offenders = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("rules/dsl must exist") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let json_paths: Vec<PathBuf> = if path.is_dir() {
            std::fs::read_dir(&path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .collect()
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            vec![path]
        } else {
            vec![]
        };

        for json_path in json_paths {
            let text = std::fs::read_to_string(&json_path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", json_path.display()));
            let pack: RulePackDef = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("failed to parse {}: {e}", json_path.display()));
            for rule in &pack.rules {
                for (field, value) in pattern_bearing_field_values(rule) {
                    if !value.contains("${") {
                        continue;
                    }
                    if fragment_ref_name(value).is_some() {
                        total_refs += 1;
                    } else {
                        offenders.push(format!(
                            "{}: rule \"{}\" `{field}`: {value:?}",
                            json_path.display(),
                            rule.id
                        ));
                    }
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a shipped pack's pattern-bearing field contains \"${{\" as part of a larger string rather than a \
         whole-value `${{NAME}}` fragment reference — collision-safety depends on this shape being \
         whole-value-only: {offenders:#?}"
    );
    assert!(
        total_refs > 0,
        "expected at least one real `${{NAME}}` reference across rules/dsl/** — this assertion would \
         pass vacuously (no sentinel anywhere) if the migration in this pass were ever reverted; keep it \
         as a tripwire"
    );
}

/// Loading the real, committed `rules/dsl` tree must produce zero errors — the totality half of guard #4
/// (see the test above): every `${NAME}` reference any shipped pack carries resolves against the shared
/// bundle or that pack's own `fragments` map, or `load_dsl_packs` would have surfaced a `PackLoadError`
/// naming it.
#[test]
fn real_dsl_tree_loads_with_zero_errors() {
    let result = load_dsl_packs(&real_dsl_dir());
    assert!(
        result.errors.is_empty(),
        "rules/dsl failed to load (a `${{NAME}}` fragment reference did not resolve, or another load \
         error): {:?}",
        result.errors
    );
    assert!(
        !result.packs.is_empty(),
        "expected at least one real pack to load"
    );
}

/// Part 3 of the task's mandatory byte-identity proof: the CURRENT `rules/dsl/redis/redis.json` (migrated
/// — every `file_exclude_pattern` is now a `${test-paths}` ref) parses+expands to a `RulePackDef` whose
/// `Debug` output is IDENTICAL to the PRE-MIGRATION file's (`tests_fixtures/redis_pre_migration.json`, a
/// byte-for-byte copy of this file's content before the migration in this pass, taken from git history).
/// `redis` never touches `sql`'s intentional `\bWHERE\b` fix, so this is a clean non-`sql` witness that
/// expand-then-clear is projection-neutral: `{pack:?}` — the cache-fingerprint input
/// (`crates/engine/src/cache.rs`) — is byte-for-byte unchanged by this migration.
#[test]
fn redis_pack_debug_output_is_unchanged_by_the_fragment_migration() {
    let current_text = std::fs::read_to_string(real_dsl_dir().join("redis/redis.json"))
        .expect("read current redis.json");
    let pre_migration_text = include_str!("../tests_fixtures/redis_pre_migration.json");

    let current = parse_dsl_pack(&current_text).expect("current redis.json must parse+expand");
    let mut pre_migration: RulePackDef =
        serde_json::from_str(pre_migration_text).expect("pre-migration redis.json must parse");
    pre_migration
        .expand_fragments()
        .expect("pre-migration pack has no fragment refs — this must be a no-op");

    assert_eq!(
        format!("{current:?}"),
        format!("{pre_migration:?}"),
        "the fragment migration changed redis.json's loaded RulePackDef — byte-identity is broken"
    );
}

/// Same MECHANISM as above, for `rules/dsl/http/http.json`, exercising the OTHER shared fragment name
/// (`test-paths-stories` vs. redis's `test-paths`) — but with a different fixture provenance: unlike
/// redis's git-history snapshot, `http_pre_migration.json` is REGENERATED alongside every http.json edit
/// (io-scan migration 2026-07-22 and since) as the live pack's fully-EXPANDED twin (no `${...}` refs —
/// the no-op `expand_fragments` below enforces that). What the pin proves is therefore "fragment
/// expansion is projection-neutral for this pack's exact current content", and it forces any http.json
/// edit to consciously touch the fixture in the same change.
#[test]
fn http_pack_debug_output_is_unchanged_by_the_fragment_migration() {
    let current_text = std::fs::read_to_string(real_dsl_dir().join("http/http.json"))
        .expect("read current http.json");
    let pre_migration_text = include_str!("../tests_fixtures/http_pre_migration.json");

    let current = parse_dsl_pack(&current_text).expect("current http.json must parse+expand");
    let mut pre_migration: RulePackDef =
        serde_json::from_str(pre_migration_text).expect("pre-migration http.json must parse");
    pre_migration
        .expand_fragments()
        .expect("pre-migration pack has no fragment refs — this must be a no-op");

    assert_eq!(
        format!("{current:?}"),
        format!("{pre_migration:?}"),
        "the fragment migration changed http.json's loaded RulePackDef — byte-identity is broken"
    );
}
