//! Contracts 4 and 10: id uniqueness/collision hygiene and the kebab-case id convention.

use std::collections::{BTreeMap, BTreeSet};

use crate::{load_all_packs, native_metas};

// ---------------------------------------------------------------------------------------------
// 4. Id hygiene
// ---------------------------------------------------------------------------------------------

#[test]
fn dsl_pack_ids_are_unique_across_packs() {
    let packs = load_all_packs();
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for pack in &packs {
        *counts.entry(pack.id.as_str()).or_insert(0) += 1;
    }
    let dups: Vec<&str> = counts
        .into_iter()
        .filter(|&(_, c)| c > 1)
        .map(|(id, _)| id)
        .collect();
    assert!(
        dups.is_empty(),
        "duplicate DSL pack ids across rules/dsl/*.json: {dups:?}"
    );
}

#[test]
fn dsl_rule_ids_are_unique_within_each_pack() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for rule in &pack.rules {
            *counts.entry(rule.id.as_str()).or_insert(0) += 1;
        }
        for (id, c) in counts {
            if c > 1 {
                offenders.push(format!("{}/{id} (x{c})", pack.id));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "duplicate rule ids within a pack: {offenders:#?}"
    );
}

/// No DSL `"pack"` id and no DSL `"pack/rule"` id may collide with a native analysis id — `is_enabled`
/// (`crates/core/src/registry.rs`) gates every layer through ONE shared exact-string-match id space, so a
/// collision would mean disabling one layer's rule via `disabled_rules` silently also disables an unrelated
/// layer's rule (or a pack id colliding with a bare native id, same hazard).
#[test]
fn no_dsl_id_collides_with_a_native_analysis_id() {
    let packs = load_all_packs();
    let native_ids: BTreeSet<String> = native_metas().into_iter().map(|m| m.id).collect();
    let mut offenders = Vec::new();
    for pack in &packs {
        if native_ids.contains(&pack.id) {
            offenders.push(format!(
                "DSL pack id `{}` collides with a native analysis id",
                pack.id
            ));
        }
        for rule in &pack.rules {
            let full = format!("{}/{}", pack.id, rule.id);
            if native_ids.contains(&full) {
                offenders.push(format!(
                    "DSL rule id `{full}` collides with a native analysis id"
                ));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

// ---------------------------------------------------------------------------------------------
// 10. Kebab-case id hygiene — every rule id follows one casing convention
// ---------------------------------------------------------------------------------------------

/// Strips an optional leading `"cross-layer/"` namespace prefix — that prefix marks a cross-layer JOIN
/// finding's pack namespace, not part of the bare id itself, so the kebab-case check below applies to the
/// id with it removed.
fn strip_cross_layer_prefix(id: &str) -> &str {
    id.strip_prefix("cross-layer/").unwrap_or(id)
}

/// Contract #10 — every DSL pack id, every DSL rule id, and every registered native analysis id (after
/// `strip_cross_layer_prefix`) matches `^[a-z0-9]+(-[a-z0-9]+)*$`: lowercase letters/digits, single hyphens
/// between groups, no leading/trailing/double hyphens, no uppercase, no underscore, no camelCase. This is
/// the machine-enforced regression guard for the cross-layer vocabulary-unification rename underway across
/// this codebase — rule ids like `unsafeReadEndpoint`/`nonIdempotentWrite`/`fe-consumes-unprovided`/
/// `cross-layer/dead-mutation-endpoint`/`cross-layer/dangling-mutation` were converted to kebab-case as
/// part of that effort; without this test, a future rule could silently reintroduce the same
/// camelCase-vs-kebab-case drift.
#[test]
fn rule_ids_are_kebab_case() {
    let kebab = regex::Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").expect("static regex");
    let mut offenders = Vec::new();

    let packs = load_all_packs();
    for pack in &packs {
        let bare = strip_cross_layer_prefix(&pack.id);
        if !kebab.is_match(bare) {
            offenders.push(format!(
                "DSL pack id `{}` (checked as `{bare}`) is not kebab-case",
                pack.id
            ));
        }
        for rule in &pack.rules {
            let bare = strip_cross_layer_prefix(&rule.id);
            if !kebab.is_match(bare) {
                offenders.push(format!(
                    "DSL rule id `{}/{}` (checked as `{bare}`) is not kebab-case",
                    pack.id, rule.id
                ));
            }
        }
    }

    for meta in native_metas() {
        let bare = strip_cross_layer_prefix(&meta.id);
        if !kebab.is_match(bare) {
            offenders.push(format!(
                "native analysis id `{}` (checked as `{bare}`) is not kebab-case",
                meta.id
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "rule ids must match ^[a-z0-9]+(-[a-z0-9]+)*$ after stripping an optional leading `cross-layer/` \
         prefix (lowercase, single hyphens between groups, no camelCase/snake_case/uppercase) — a hit here \
         means the cross-layer vocabulary-unification rename's kebab-case convention broke again: \
         {offenders:#?}"
    );
}
