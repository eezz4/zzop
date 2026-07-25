//! Contracts 4, 10 and 13: id uniqueness/collision hygiene, the kebab-case id convention, and the
//! kebab-case convention for the SECOND name layer DSL packs declare — `LabeledPattern::label`.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::Matcher;

use crate::{load_all_packs, native_ids};

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
    let native_ids: BTreeSet<String> = native_ids().into_iter().collect();
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

    for id in native_ids() {
        let bare = strip_cross_layer_prefix(&id);
        if !kebab.is_match(bare) {
            offenders.push(format!(
                "native analysis id `{id}` (checked as `{bare}`) is not kebab-case"
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

// ---------------------------------------------------------------------------------------------
// 13. Kebab-case label hygiene — the second name layer, which contract 10 structurally cannot see
// ---------------------------------------------------------------------------------------------

/// Every `LabeledPattern::label` a shipped pack declares, tagged with the matcher field it came from.
/// `SymbolScan`/`IoScan` carry no `LabeledPattern` at all, so the two matcher arms below are the whole
/// population — see the test doc.
fn labeled_pattern_sites(rule: &zzop_core::RuleDef) -> Vec<(&'static str, &str)> {
    let mut out = Vec::new();
    match &rule.matcher {
        Matcher::LineScan(m) => {
            for lp in m.any.iter().flatten() {
                out.push(("any[].label", lp.label.as_str()));
            }
        }
        Matcher::MethodScan(m) => {
            for lp in &m.patterns {
                out.push(("patterns[].label", lp.label.as_str()));
            }
            for lp in &m.absent {
                out.push(("absent[].label", lp.label.as_str()));
            }
        }
        Matcher::SymbolScan(_) | Matcher::IoScan(_) => {}
    }
    out
}

/// Contract #13 — every `LabeledPattern::label` in every shipped pack matches the SAME
/// `^[a-z0-9]+(-[a-z0-9]+)*$` shape contract 10 demands of ids. Same form, different namespace, and two
/// different reasons for the same form — one per bullet below:
///
/// - `LineScan::any[].label` is a WIRE key. It is the only value that reaches a user as
///   `Finding.data.label` (`crates/core/src/dsl/line_scan.rs`'s `json!({ "snippet": …, "label": label })`),
///   and for a multi-arm rule it is the only stable answer to "which arm fired" — `snippet` is verbatim
///   source text and cannot serve as a key. A consumer groups/filters on it, so it must be greppable and
///   quote-free. This is the layer contract 10 cannot reach: it walks rule IDS, and no id enumeration ever
///   visits a label. The defect that motivated this test was three labels that were English SENTENCES
///   (`"ECB mode (no diffusion)"` and two siblings, all in `security/weak-crypto`) going out on the wire.
/// - `MethodScan::patterns[].label`/`absent[].label` never reach the wire (that matcher emits
///   `{"snippet", "method"}`), but `trigger`/`after` REFERENCE `patterns[].label` by exact string, so those
///   labels are identifiers and get identifier hygiene for the ordinary reason.
///
/// What this deliberately does NOT assert, unlike contract 10: uniqueness, global or otherwise. Label
/// scope is rule-local — a user never types one (no config key, no `<id>-ok` marker, no `zzop explain`
/// argument), so the same word means unrelated things in unrelated rules by design (`read` appears in
/// several) and a cross-rule collision is not a defect here.
///
/// Why this lives here and not in `docs/contracts/rule-pack.schema.json` as a `pattern`: that schema is
/// also THIRD-PARTY packs' contract, and a house naming convention must not make an outside pack fail to
/// load or fail `validate-rule-pack`. This test walks `load_all_packs()` — our packs only — which is
/// exactly the scope the convention claims.
#[test]
fn dsl_pattern_labels_are_kebab_case() {
    let kebab = regex::Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").expect("static regex");
    let mut offenders = Vec::new();
    for pack in &load_all_packs() {
        for rule in &pack.rules {
            for (site, label) in labeled_pattern_sites(rule) {
                if !kebab.is_match(label) {
                    offenders.push(format!(
                        "`{}/{}` {site} = {label:?} is not kebab-case",
                        pack.id, rule.id
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "DSL pattern labels must match ^[a-z0-9]+(-[a-z0-9]+)*$ — a label is an identification tag, not a \
         description: prose belongs in the rule's `message`, which already says it and would then rot in \
         two places. `any[].label` additionally ships to users verbatim as `Finding.data.label`: \
         {offenders:#?}"
    );
}
