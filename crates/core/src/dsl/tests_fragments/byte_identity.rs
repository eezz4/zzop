//! Real-`rules/dsl`-tree guards for the fragment mechanism — see the `tests_fragments` module doc. These
//! load the actual committed pack files (never a synthetic fixture), proving the sentinel is used only as
//! an intended whole-value reference, that the whole tree resolves cleanly, and that the migration left
//! two non-`sql` packs' loaded `RulePackDef` byte-identical to their pre-migration form.

use super::super::def::{for_each_pattern_field, RuleDef, RulePackDef};
use super::super::fragments::fragment_ref_name;
use super::{raw_packs, real_dsl_dir};
use crate::{load_dsl_packs, parse_dsl_pack};

/// Every field `expand_fragments` treats as a `${NAME}`-eligible pattern — literally the same walk the
/// mechanism performs (`def::pattern_fields::for_each_pattern_field`), so this guard can never drift from
/// it. It used to be a hand-copied twin of `expand_fragments`'s match arms whose doc PROMISED "the EXACT
/// same field set" with no way to keep the promise, and the promise was already false: neither list
/// mentioned `IoScan::symbol_pattern` or `IoScan::anchor_exclude_pattern`.
///
/// Deliberately narrower than "every string in the JSON file": `message`/`id`/`label` legitimately mention
/// `${...}` as PROSE (e.g. a message explaining what a template-literal placeholder looks like) — those
/// are not pattern fields and must not be flagged.
fn pattern_bearing_field_values(rule: &mut RuleDef) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for_each_pattern_field::<std::convert::Infallible>(rule, &mut |field, value| {
        out.push((field, value.clone()));
        Ok(())
    })
    .expect("the collecting callback is infallible");
    out
}

/// Part 1, guard #4: no shipped `rules/dsl/**` pattern-bearing field contains the `${...}` sentinel shape
/// except as an intended, total, whole-value fragment reference. Reads every real pack's RAW (pre-
/// expansion) form via `raw_packs` — deliberately NOT `parse_dsl_pack`, so a sentinel is
/// still visible for this check to see — and asserts every `pattern_bearing_field_values` entry containing
/// the substring `"${"` is EXACTLY a whole-value `${NAME}` reference (`fragment_ref_name` returns `Some`),
/// never a partial/substring occurrence. `message`/`id`/`label` are out of scope (see
/// `pattern_bearing_field_values`'s doc) — `security/shell-exec-interpolation` and `sql/delete-no-where`'s
/// messages legitimately describe `${...}` template-literal/placeholder syntax in prose. Paired with
/// `real_dsl_tree_loads_with_zero_errors` below, which proves every real reference actually resolves (an
/// unknown name is a hard load error, not a silent skip) — together the two prove expansion is total AND
/// unambiguous, per this guard's own charter.
#[test]
fn no_shipped_pattern_contains_the_sentinel_except_as_an_intended_whole_value_ref() {
    let mut total_refs = 0usize;
    let mut offenders = Vec::new();

    for (json_path, mut pack) in raw_packs() {
        for rule in &mut pack.rules {
            let rule_id = rule.id.clone();
            for (field, value) in pattern_bearing_field_values(rule) {
                if !value.contains("${") {
                    continue;
                }
                if fragment_ref_name(&value).is_some() {
                    total_refs += 1;
                } else {
                    offenders.push(format!(
                        "{}: rule \"{rule_id}\" `{field}`: {value:?}",
                        json_path.display(),
                    ));
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
/// `Debug` output is IDENTICAL to the PRE-MIGRATION file's (`tests_fixtures/redis_pre_migration.json`).
/// That fixture STARTED as a byte-for-byte copy taken from git history, but it is REGENERATED alongside
/// every later `redis.json` edit that changes `RuleDef`'s shape or content (the suppress-marker derivation
/// pass rewrote both) — same standing as the http twin below. So it witnesses that THIS migration is
/// projection-neutral; it is not an independent snapshot of the pack's whole history.
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
