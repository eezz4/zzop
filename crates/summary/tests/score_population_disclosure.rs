//! The scored-POPULATION disclosure reaches the published `architecture.painMeaning` of a real reply,
//! on a real tree, through the real `zzop.config.jsonc` key a user actually writes.
//!
//! # Why this exists at THIS layer
//! `scores.excludeTestFilesFromFileMetrics` RE-BASES `pain` — measured on the fixed
//! `corpus/frameworks/express` checkout, the same bytes score 30.0 with the key off and 11.5 with it
//! on, while `findings.total` stays 56. `pain` is also the ONLY score number the CLI and MCP surfaces
//! publish. Those two facts together make the population sentence load-bearing rather than decorative:
//! without it, two replies over identical code carry different numbers and nothing distinguishes them,
//! which is the reading "the code improved".
//!
//! A disclosure that substitutes for a capability gets its pin at the LAST layer, not only the first —
//! the rule `overlay_fragment_collision_disclosure.rs` in this directory states and the reason it also
//! lives here. Between `EngineConfig::scores_exclude_test_files` and a user sit
//! `compute_health_index`, the facade's output view and `summary::analyze::architecture`, and a unit
//! test on any one of them stays green while the chain breaks. The specific regression this file
//! exists to catch is the cheapest one to write by accident: the engine passing a hardcoded `false` to
//! `compute_health_index`, which leaves every reply claiming the wide population over a narrowed
//! number. Nothing in `crates/metrics` or `crates/summary`'s own unit tests can see that.
//!
//! # What is asserted, and what is deliberately NOT
//! Properties, never the sentence — the same discipline as this directory's sibling. Three:
//!   1. the two configurations produce DIFFERENT `painMeaning` text (a reader can tell them apart);
//!   2. each names the key, so a reader who wants the other state knows what to set;
//!   3. `criticalTop`'s own legend does NOT move, because that list is computed outside the scores
//!      subsystem and no key narrows it — the one claim the legend makes about a sibling field.

use std::fs;
use std::path::{Path, PathBuf};

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

/// A self-cleaning temp tree (std-only; this crate's tests share no test-utils module — same pattern
/// as `disclosure_fold.rs` and `overlay_fragment_collision_disclosure.rs`).
struct TempTree(PathBuf);

impl TempTree {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "zzop-score-population-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp tree must be creatable");
        TempTree(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("temp subdir must be creatable");
        }
        fs::write(full, content).expect("temp file must be writable");
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A tree with git history, since every structural score is git-gated (`architecture` is absent from
/// a reply whose run collected no history). One source file and one test file, so the two population
/// readings are genuinely different sets.
fn tree_with_a_test_file(name: &str, config: &str) -> TempTree {
    let dir = TempTree::new(name);
    dir.write(
        "src/login.ts",
        "export function login(u: string) {\n  return u;\n}\n",
    );
    dir.write(
        "src/login.test.ts",
        "import { login } from './login';\nit('works', () => login('a'));\n",
    );
    dir.write("zzop.config.jsonc", config);
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("git must be runnable for this test")
    };
    git(&["init", "-q", "."]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.email=t@example.com",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "seed",
    ]);
    dir
}

/// The published reply's `architecture` object, from the entry point every host shares.
fn architecture_of(dir: &TempTree) -> serde_json::Value {
    let out = zzop_summary::analyze_summary(
        Some(&dir.path().display().to_string()),
        None,
        &default_filters(),
    )
    .expect("analyze must succeed on this tree");
    let reply: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    reply
        .get("architecture")
        .cloned()
        .unwrap_or_else(|| panic!("the reply must carry an `architecture` object: {out}"))
}

const WIDE: &str = r#"{ "roots": ["."] }"#;
const NARROWED: &str =
    r#"{ "roots": ["."], "scores": { "excludeTestFilesFromFileMetrics": true } }"#;

#[test]
fn the_published_pain_legend_states_which_population_the_run_actually_used() {
    let wide = architecture_of(&tree_with_a_test_file("wide", WIDE));
    let narrowed = architecture_of(&tree_with_a_test_file("narrowed", NARROWED));

    let wide_meaning = wide["painMeaning"]
        .as_str()
        .expect("painMeaning is a string");
    let narrowed_meaning = narrowed["painMeaning"]
        .as_str()
        .expect("painMeaning is a string");

    // (1) The reader can tell the two runs apart. Asserted as a DIFFERENCE rather than as a substring
    // of one, so a build that prints the narrowed sentence unconditionally is caught too.
    assert_ne!(
        wide_meaning, narrowed_meaning,
        "the config key moved `pain`'s population and the published legend says the same thing \
         either way — two replies over the same code now carry two numbers a reader cannot tell apart"
    );

    // (2) Each state names the key, so the other state is reachable from the reply alone.
    for meaning in [wide_meaning, narrowed_meaning] {
        assert!(
            meaning.contains("scores.excludeTestFilesFromFileMetrics"),
            "the population clause must name the key that moves it: {meaning}"
        );
    }

    // The clause must be findable without reading the paragraph — a fixed token both states share.
    for meaning in [wide_meaning, narrowed_meaning] {
        assert!(
            meaning.contains("POPULATION:"),
            "the population clause must lead with a locatable token: {meaning}"
        );
    }
}

/// (3) `criticalTop` is computed by `compute_criticality`, which takes no scores config, so the key
/// cannot narrow it — and the legend says so out loud. If a later change wires the population into
/// criticality without rewriting that sentence, this goes red instead of shipping a false claim.
#[test]
fn the_critical_top_legend_does_not_move_with_the_score_population() {
    let wide = architecture_of(&tree_with_a_test_file("ct-wide", WIDE));
    let narrowed = architecture_of(&tree_with_a_test_file("ct-narrowed", NARROWED));
    assert_eq!(
        wide["criticalTopMeaning"], narrowed["criticalTopMeaning"],
        "the criticalTop legend claims to hold in EVERY run; it does not"
    );
}
