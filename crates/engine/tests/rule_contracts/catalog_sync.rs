//! Contract 5: catalog sync — docs/rules/catalog.md must match the loaded reality, not a hand-updated
//! snapshot.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{load_all_packs, native_metas};

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
    let actual_natives = native_metas().len();

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
    let missing: Vec<String> = native_metas()
        .into_iter()
        .map(|m| m.id)
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
