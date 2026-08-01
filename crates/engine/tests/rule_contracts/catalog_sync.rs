//! Contract 5: catalog sync — docs/rules/catalog.md must match the loaded reality, not a hand-updated
//! snapshot. Totals, id mentions, and (since 2026-07-31) the sightline surface: the set of catalog rows
//! carrying a sightline paragraph must equal the set of `RuleSightline` declarations the build composes.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{load_all_packs, native_ids};

fn catalog_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/rules/catalog.md")
}

fn catalog_text() -> String {
    let path = catalog_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Parses `docs/rules/catalog.md`'s totals sentence (the `**Totals** (...): N DSL packs, N DSL rules, N
/// native analysis ids.` line near the top of the file) and asserts the three numbers match what
/// `load_dsl_packs`/`register_native_analyses` actually produce. The sentence is intentionally phrased in
/// one fixed, easily-`regex`-parsed shape (restructured for this test — see the doc's own totals line) so a
/// human editing prose around it doesn't accidentally break this test's ability to find the numbers; if you
/// legitimately need to reword that sentence, keep the `N DSL packs, N DSL rules, N native analysis ids`
/// clause's shape intact (or update this regex to match, deliberately, in the same commit).
#[test]
fn catalog_totals_match_loaded_rule_and_analysis_counts() {
    let text = catalog_text();
    let re =
        regex::Regex::new(r"(\d+)\s+DSL packs,\s*(\d+)\s+DSL rules,\s*(\d+)\s+native analysis ids")
            .expect("static regex");
    let caps = re.captures(&text).unwrap_or_else(|| {
        panic!(
            "docs/rules/catalog.md's totals sentence not found in the expected \"N DSL packs, N DSL \
             rules, N native analysis ids\" shape — either the doc's totals line was reworded (update it \
             back to that shape, or update this test's regex in the same commit) or the file moved"
        )
    });
    let stated_packs: usize = caps[1].parse().expect("digits");
    let stated_rules: usize = caps[2].parse().expect("digits");
    let stated_natives: usize = caps[3].parse().expect("digits");

    let packs = load_all_packs();
    let actual_rules: usize = packs.iter().map(|p| p.rules.len()).sum();
    let actual_natives = native_ids().len();

    assert_eq!(
        stated_packs,
        packs.len(),
        "catalog.md states {stated_packs} DSL packs, but rules/dsl/*.json loads {}",
        packs.len()
    );
    assert_eq!(
        stated_rules, actual_rules,
        "catalog.md states {stated_rules} DSL rules, but the loaded packs total {actual_rules}"
    );
    assert_eq!(
        stated_natives, actual_natives,
        "catalog.md states {stated_natives} native analysis ids, but register_native_analyses registers \
         {actual_natives}"
    );
}

#[test]
fn catalog_mentions_every_native_analysis_id() {
    let text = catalog_text();
    let missing: Vec<String> = native_ids()
        .into_iter()
        .filter(|id| !text.contains(id.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "native analysis ids registered but absent from docs/rules/catalog.md's text: {missing:?}"
    );
}

#[test]
fn catalog_mentions_every_dsl_pack_id() {
    let text = catalog_text();
    let packs = load_all_packs();
    let missing: Vec<&str> = packs
        .iter()
        .map(|p| p.id.as_str())
        .filter(|id| !text.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "DSL pack ids loaded but absent from docs/rules/catalog.md's text: {missing:?}"
    );
}

/// The gap the v0.23.0 release audit found: the pins above cover TOTALS, native ids and PACK ids, so a
/// count-preserving rule-id RENAME — exactly what `df78842` did to 7 ids — would ship with the catalog
/// still naming the old ids and no test objecting. That matters more than an ordinary doc drift: the
/// catalog is `include_str!`-embedded as the MCP resource `zzop://contract/rule-catalog`
/// (`crates/summary/src/contracts.rs`), and `scripts/check-docs-rule-ids.sh` DERIVES its valid-id universe
/// from this same file — so one stale id here becomes a stale wire contract AND blesses stale ids in
/// every other doc the guard checks.
///
/// Matched as `` `<id>` `` (backticked), which is how every rule row spells its id — a bare `contains`
/// would let a rule id that appears only inside prose about a different rule count as present.
#[test]
fn catalog_mentions_every_dsl_rule_id() {
    let text = catalog_text();
    let packs = load_all_packs();
    let missing: Vec<String> = packs
        .iter()
        .flat_map(|p| p.rules.iter().map(move |r| (p.id.as_str(), r.id.as_str())))
        .filter(|(_, rule_id)| !text.contains(&format!("`{rule_id}`")))
        .map(|(pack_id, rule_id)| format!("{pack_id}/{rule_id}"))
        .collect();
    assert!(
        missing.is_empty(),
        "DSL rule ids loaded but absent from docs/rules/catalog.md as `<id>`: {missing:?} — a rule \
         rename must update the catalog in the same commit (it is the MCP rule-catalog resource and \
         check-docs-rule-ids.sh's id universe)"
    );
}

/// The rule ids whose catalog row publishes a sightline claim. A row "claims" when it contains the
/// phrase `language sightline` or `evidence sightline` (case-insensitive) — deliberately the PHRASE,
/// not only the full `**Language sightline:**` / `**Evidence sightline:**` marker, because half the
/// claiming rows today claim by back-reference instead (`Same language sightline as
/// \`soft-delete-bypass\` above`, `the same **language sightline**:`, `Same evidence sightline as the
/// row above`) and a marker-only parse would read those four rows as claim-free. The claim is
/// attributed to the row's OWN id (first backticked cell) — never to ids the prose mentions.
fn catalog_sightline_row_ids(text: &str) -> std::collections::BTreeSet<String> {
    let phrase = regex::Regex::new(r"(?i)(language|evidence) sightline").expect("static regex");
    let row_id = regex::Regex::new(r"^\|\s*`([^`]+)`\s*\|").expect("static regex");
    let mut ids = std::collections::BTreeSet::new();
    for line in text.lines().filter(|l| phrase.is_match(l)) {
        let caps = row_id.captures(line).unwrap_or_else(|| {
            panic!(
                "docs/rules/catalog.md mentions a sightline outside a `| `<id>` | ...` rule row, so \
                 this guard cannot attribute the claim to a rule id — move the sentence into the \
                 owning rule's row, or extend catalog_sightline_row_ids deliberately: {line:?}"
            )
        });
        ids.insert(caps[1].to_string());
    }
    ids
}

/// Rule ids allowed to carry a catalog sightline paragraph WITHOUT a `RuleSightline` declaration —
/// `(id, reason)`, and an entry must actually exempt something (asserted below) so it cannot go stale.
///
/// EMPTY today, on purpose. The two candidates the sightline DECISIONS block records as deliberately
/// undeclared (`crates/engine/src/sightlines.rs`) need no entry because their catalog rows carry no
/// sightline paragraph either — both directions already agree — and the red a future marker on either
/// row would raise is CORRECT, not noise:
/// - `mutating-route-no-auth`: its trigger IS witnessed in every call-graph-covered extension (`.go`
///   routes included) — the gap is call-EDGE evidence, route-conditional, owned by the S8
///   framework-silence warning the coverage reply forwards per tree. A catalog sightline paragraph
///   would mis-type that per-run conditional disclosure as a build capability: on red, remove the
///   marker — unless the rule's disclosure has genuinely become extension-conditional, in which case
///   declare a `RuleSightline` next to the rule instead.
/// - `route-shadowing`: its language gate is an EXEMPTION on routing semantics (first-match vs
///   most-specific), not an evidence channel — semantic, not evidential, so an extension cross cannot
///   express it. Same red-on-marker reasoning.
const CATALOG_SIGHTLINE_EXEMPT: &[(&str, &str)] = &[];

/// The REVERSE guard of the sightline mechanism. `crates/engine/src/sightlines.rs` pins declared →
/// registered, but nothing pinned the prose SSOT against the machine half: a rule publishing a
/// sightline paragraph in docs/rules/catalog.md while declaring no `RuleSightline` failed silently —
/// exactly how `mutating-route-no-auth`'s (deliberate) absence reached stage-2 review with two owners
/// disagreeing about whether it was an omission. Set equality, both directions:
/// (a) every catalog sightline row is declared or exempted — a new rule shipping the paragraph
///     without the declaration goes red;
/// (b) every declaration's id has a catalog sightline row — the machine half must never claim more
///     than the prose SSOT.
#[test]
fn catalog_sightline_rows_and_declared_rule_sightlines_are_the_same_set() {
    let catalog = catalog_sightline_row_ids(&catalog_text());
    let declared: std::collections::BTreeSet<String> = zzop_engine::rule_sightlines()
        .iter()
        .map(|s| s.rule_id.to_string())
        .collect();
    assert!(
        !catalog.is_empty(),
        "sanity: no catalog sightline rows found at all"
    );
    assert!(
        !declared.is_empty(),
        "sanity: no RuleSightline declared at all"
    );

    let exempt: std::collections::BTreeSet<&str> =
        CATALOG_SIGHTLINE_EXEMPT.iter().map(|(id, _)| *id).collect();
    for (id, reason) in CATALOG_SIGHTLINE_EXEMPT {
        assert!(
            catalog.contains(*id) && !declared.contains(*id),
            "stale sightline exemption {id:?} ({reason}) — it no longer exempts anything (the row \
             dropped its sightline paragraph, or the rule now declares); delete the entry"
        );
    }

    let undeclared: Vec<&String> = catalog
        .iter()
        .filter(|id| !declared.contains(*id) && !exempt.contains(id.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "docs/rules/catalog.md publishes a sightline paragraph for {undeclared:?}, but \
         zzop_engine::rule_sightlines() declares no RuleSightline for them — the coverage query would \
         stay silent about a blind spot the prose promises. Declare a RuleSightline next to the rule \
         (see the owning crate's rule_sightlines()), or add a documented exemption to \
         CATALOG_SIGHTLINE_EXEMPT with its reason"
    );

    let unpublished: Vec<&String> = declared
        .iter()
        .filter(|id| !catalog.contains(*id))
        .collect();
    assert!(
        unpublished.is_empty(),
        "RuleSightline declared for {unpublished:?}, but their docs/rules/catalog.md rows carry no \
         sightline paragraph — the machine half must never claim more than the prose SSOT; add the \
         row's sightline paragraph in the same commit (or drop the declaration)"
    );
}
