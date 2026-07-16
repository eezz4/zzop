//! Contract 12: cross-pack policy-vocabulary pin — be-reliability/sync-fs-in-handler and
//! be-db/client-per-request must share one "handler-context" evidence definition.

use zzop_core::{Matcher, RuleDef, RulePackDef};

use crate::load_all_packs;

/// Finds a loaded pack by id, panicking with a clear message if it's missing — same "fail loudly" spirit as
/// `load_all_packs`.
fn find_pack<'a>(packs: &'a [RulePackDef], id: &str) -> &'a RulePackDef {
    packs
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("pack `{id}` not loaded"))
}

/// Finds a rule by id within a pack, panicking with a clear message if it's missing.
fn find_rule<'a>(pack: &'a RulePackDef, id: &str) -> &'a RuleDef {
    pack.rules
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("rule `{}/{id}` not loaded", pack.id))
}

/// Extracts a `Matcher::MethodScan` rule's `patterns[]` entry with the given `label`, panicking if the rule
/// isn't a method-scan rule or has no pattern with that label — both are authoring errors this pin exists to
/// catch, not conditions worth silently tolerating.
fn method_scan_pattern_by_label<'a>(rule: &'a RuleDef, label: &str) -> &'a str {
    match &rule.matcher {
        Matcher::MethodScan(m) => m
            .patterns
            .iter()
            .find(|lp| lp.label == label)
            .unwrap_or_else(|| panic!("{}: no patterns[] entry labeled `{label}`", rule.id))
            .pattern
            .as_str(),
        other => panic!("{}: expected a MethodScan matcher, got {other:?}", rule.id),
    }
}

/// Policy pin: `be-reliability/sync-fs-in-handler` and `be-db/client-per-request` both approximate "this
/// function looks like a request handler" with a `patterns[]` entry labeled `handler-context` — the SAME
/// evidence definition, deliberately duplicated across the two packs (a DSL rule can't reference another
/// pack's pattern). Nothing else stops one pack's copy drifting from the other's during an unrelated edit —
/// each pack's own fixtures only exercise its own copy, so a silent fork of what counts as "handler context"
/// (e.g. one pack keeping the naive `res` bare-word evidence a mono-hub 0.10.0 field review found false-
/// positives on, while the other adopts the tightened one) would ship unnoticed. This test loads both
/// shipped DSL packs fresh (via `load_dsl_packs`, same helper every other contract here uses — never a hand-
/// copied inline fixture), extracts each rule's own `handler-context` pattern string, and asserts they are
/// byte-identical, so a future edit to one without the other fails loudly here instead of drifting unnoticed.
#[test]
fn handler_context_pattern_is_identical_across_be_reliability_and_be_db() {
    let packs = load_all_packs();
    let be_reliability = find_pack(&packs, "be-reliability");
    let be_db = find_pack(&packs, "be-db");

    let sync_fs_rule = find_rule(be_reliability, "sync-fs-in-handler");
    let client_per_request_rule = find_rule(be_db, "client-per-request");

    let sync_fs_pattern = method_scan_pattern_by_label(sync_fs_rule, "handler-context");
    let client_per_request_pattern =
        method_scan_pattern_by_label(client_per_request_rule, "handler-context");

    assert_eq!(
        sync_fs_pattern, client_per_request_pattern,
        "be-reliability/sync-fs-in-handler and be-db/client-per-request's `handler-context` patterns have \
         drifted — they encode the same handler-evidence policy and must stay byte-identical (see this \
         test's own doc comment)"
    );
}
