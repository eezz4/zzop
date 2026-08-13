//! The shipped-pack matcher-kind census, and the ONE disclosure it exists to keep honest:
//! **`symbol-scan` has zero rules in any bundled pack.**
//!
//! ## Why this needs a test and not just a sentence
//! `docs/rules/authoring-guide.md` tells a would-be rule author that `symbol-scan` is implemented and
//! demonstrated but has no production exercise behind it. That is a true and useful thing to say today
//! and a false thing to say the day someone ships the first `symbol-scan` rule — and nothing about
//! shipping that rule would make the author of it read this paragraph. A disclosure that cannot notice
//! its own expiry is the failure shape this repo keeps paying for (the version-prose guard exists for
//! the same class), so the claim is held by the census below: ship a `symbol-scan` rule and this test
//! goes red, naming the paragraph that must be deleted.
//!
//! ## Why the census is not ratcheted per kind
//! Counting all six kinds and pinning each would make every new rule a documentation edit, which buys
//! nothing — no published sentence depends on "there are 76 line-scans". Only the ZERO is load-bearing,
//! because only the zero is what the disclosure claims. So this asserts the zero exactly and reports
//! the rest for a reader, without pinning them.
//!
//! Read against the REAL committed pack tree (`tests_fragments::real_dsl_dir`), never a fixture: a
//! census of synthetic packs would answer a question nobody asked.

use std::collections::BTreeMap;
use std::fs;

use super::def::RulePackDef;
use super::tests_fragments::{real_dsl_dir, repo_rel};

/// Every bundled pack's rules, bucketed by matcher kind — the kind string is taken from the JSON's own
/// `matcher.type` tag rather than from the parsed `Matcher` enum, because the tag is what a pack AUTHOR
/// writes and what the published schema names.
fn census() -> BTreeMap<String, usize> {
    let dir = real_dsl_dir();
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    let mut packs = 0usize;
    for entry in fs::read_dir(&dir).expect("rules/dsl must be readable") {
        let entry = entry.expect("dir entry");
        if !entry.file_type().expect("file type").is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let file = entry.path().join(format!("{name}.json"));
        if !file.exists() {
            continue;
        }
        let text = fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", repo_rel(&file)));
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", repo_rel(&file)));
        // Parsing it as the real type too, so this census cannot drift onto a shape the engine would
        // reject — and so the `framework`-key removal stays exercised against every shipped pack.
        let _: RulePackDef = serde_json::from_str(&text).unwrap_or_else(|e| {
            panic!(
                "{} does not deserialize as RulePackDef: {e}",
                repo_rel(&file)
            )
        });
        packs += 1;
        for rule in value["rules"].as_array().into_iter().flatten() {
            let kind = rule["matcher"]["type"]
                .as_str()
                .unwrap_or_else(|| panic!("{} has a rule with no matcher.type", repo_rel(&file)));
            *out.entry(kind.to_string()).or_default() += 1;
        }
    }
    // Non-vacuity floor: an empty or near-empty census would satisfy the zero-assertion trivially,
    // which is exactly how a guard rots into a rubber stamp.
    assert!(
        packs >= 8,
        "censused only {packs} bundled pack(s) — the pack tree moved and this census would be \
         vacuously green"
    );
    let total: usize = out.values().sum();
    assert!(
        total >= 100,
        "censused only {total} rule(s) across {packs} pack(s) — see the note above"
    );
    out
}

/// The disclosure's own truth condition. When this fails it is USUALLY good news (someone shipped the
/// first `symbol-scan` rule) — and the fix is to delete the paragraph, not to weaken this test.
#[test]
fn no_bundled_pack_ships_a_symbol_scan_rule() {
    let c = census();
    let n = c.get("symbol-scan").copied().unwrap_or(0);
    assert_eq!(
        n, 0,
        "{n} bundled `symbol-scan` rule(s) now ship, so the paragraph in \
         docs/rules/authoring-guide.md that says none do is FALSE and must be deleted (search it for \
         `symbol-scan` ships with zero rules). Full census: {c:?}"
    );
}

/// The other five kinds are reported, not pinned — this test exists so a reader of the assertion above
/// can see what the census actually contains, and so a census that silently collapsed to one kind is
/// visible rather than merely non-zero.
#[test]
fn the_shipped_census_covers_more_than_one_matcher_kind() {
    let c = census();
    assert!(
        c.len() >= 4,
        "shipped packs use only {} matcher kind(s): {c:?} — either the pack tree shrank or this \
         census stopped seeing most of it",
        c.len()
    );
}
