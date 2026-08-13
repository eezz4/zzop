//! The shipped `scoreMeanings.publicApi` sentence, pinned against what `is_root_import` actually
//! does — kept apart from the behaviour tests so the pin is findable by its subject rather than by
//! its position. (It was a sibling `mod legend_tests` inside `public_api.rs` until 2026-08-13, when
//! the file crossed the per-file line cap and both test modules moved out; the separation itself is
//! older and is the reason the split fell here.)
//!
//! Every test in this file is deliberately BEHAVIOUR-FIRST: it establishes the verdict by running the
//! metric, then requires the shipped sentence to be consistent with what it just observed. A pin on
//! the wording alone would go stale exactly the way the wording did.

use super::*;
use crate::scores::meanings::score_meaning;

/// The legend went false the day it was written and stayed false for thirteen releases.
///
/// `git log -S`: `!after_root.contains('/')` has decided this since v0.16.0 (`7de1a97`), while the
/// sentence *"go through a module's index"* landed at v0.29.0 (`b9dfa3f`) — describing a barrel
/// check that the anchored `^index\.(?:tsx?|jsx?|mjs|cjs)$` regex could never make, because every
/// string it accepts is slash-free and the `!contains('/')` operand beside it already passed them.
/// The v0.29.0 test suite ALREADY asserted a non-index root file scores 100. Nothing compared the
/// sentence to the behaviour, so nothing went red; `meanings/tests.rs` checks only that every key
/// HAS a sentence.
///
/// This test is that missing comparison.
#[test]
fn the_shipped_legend_describes_the_top_level_rule_the_metric_actually_applies() {
    // Behaviour first: a NON-index file directly under another module is a public-surface import.
    let d: DepGraph = [(
        "features/cart/cart.ts".to_string(),
        vec!["features/auth/login.ts".to_string()],
    )]
    .into_iter()
    .collect();
    let r = compute_public_api(&d, &ScoresConfig::default(), &|_| true);
    assert_eq!(r.total_cross_module_imports, 1);
    assert!(
        r.deep_imports.is_empty(),
        "fixture: a non-index root file must count as public surface — if this flipped, the \
         metric changed and the legend below has to be rewritten to match the NEW behaviour"
    );

    // The sentence that ships beside the number must therefore not promise a barrel.
    let legend = score_meaning("publicApi").expect("publicApi has a shipped legend");
    assert!(
        legend.contains("TOP LEVEL") || legend.contains("top level"),
        "the legend must state the rule the metric applies (any file directly under the module), \
         not just the barrel case: {legend}"
    );
    assert!(
        !legend.contains("go through a module's index"),
        "this exact phrasing was false from v0.29.0 to 2026-08-12 — it promises a barrel check the \
         anchored index regex never performed: {legend}"
    );
}

/// The 100 this metric publishes has TWO causes, and only one of them is good news.
///
/// `module_root` resolves a declared FSD slice/base directory, else the FIRST PATH SEGMENT. A
/// repository that keeps its code under a single top-level directory therefore has almost no
/// imports that cross a module, reaches `total == 0`, and takes the `100.0` arm — a perfect
/// barrel-discipline score for a tree the metric never judged. Measured 2026-08-13 over the
/// 17-repo corpus (`corpus/oss/zzop.config.jsonc`): a MAJORITY of the trees report
/// `totalCrossModuleImports: 0`, every one of them a single-`src/`-style layout, while the trees
/// that do carry several code-bearing top-level directories produce populations in the tens or
/// hundreds. The axis is not being widened on that evidence — see `is_root_import`'s doc for the
/// same refusal on the nested-barrel question. The retest trigger for reopening it is a corpus that
/// actually contains monorepo/FSD layouts, so cross-module edges become a population worth judging;
/// and when it is reopened, the nested-barrel question has to be decided in the SAME commit, because
/// both axes run through this one resolver.
///
/// What DID move is the sentence: `health.contributors` already told this story (`population: 0`,
/// `gap: null`, dropped from `pain` — the 2026-08-08 renormalization), but `scores.publicApi`
/// shipped a bare `100` whose denominator sat unremarked in the field beside it. This test is the
/// pin that the legend now names the module axis and the empty-denominator reading.
#[test]
fn the_legend_discloses_that_a_100_can_mean_an_empty_denominator() {
    // Behaviour first: a single-top-level-directory tree produces NO cross-module imports at all,
    // and the score guard turns that into a perfect 100.
    let d: DepGraph = [(
        "src/app/routes/user.ts".to_string(),
        vec!["src/db/repo/user.ts".to_string()],
    )]
    .into_iter()
    .collect();
    let r = compute_public_api(&d, &ScoresConfig::default(), &|_| true);
    assert_eq!(
        r.total_cross_module_imports, 0,
        "fixture: both paths sit under the single top-level segment `src`, so they are one \
         module and nothing crosses — if this changed, the module axis was widened and the \
         decision plus this legend need revisiting in the same commit"
    );
    assert_eq!(
        r.score, 100.0,
        "the empty-denominator guard is what makes the legend's warning necessary"
    );

    // The sentence beside that 100 must therefore point at the denominator and name the axis.
    let legend = score_meaning("publicApi").expect("publicApi has a shipped legend");
    assert!(
        legend.contains("totalCrossModuleImports"),
        "the legend must name the denominator field a reader has to check before quoting a \
         100: {legend}"
    );
    assert!(
        legend.contains("first path segment"),
        "the legend must state the module axis, since that axis is WHY the denominator empties \
         on ordinary layouts: {legend}"
    );
}
