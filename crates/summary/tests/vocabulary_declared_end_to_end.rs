//! THE STARTER CONFIG'S OWN KEYS ARE NOT SILENT — the end-to-end pin that did not exist while the
//! block it guards was shipping a false answer on every tree (2026-09-15, external review round 23,
//! ledger V249).
//!
//! # What was missing, stated as a population
//! `coverage::vocabulary_declared`'s three unit tests each hand `rows()` a request they WROTE, so the
//! config front end — the thing that decides what reaches a request — was outside their population.
//! It is also the thing that was wrong: `mapper::options::build_vocabulary` strips the keys the front
//! end consumes itself, so a key the author declared can be absent from the request, and the block
//! read the request. Every unit test stayed green while `zzop coverage` told a project that a key its
//! own config declares had been ignored.
//!
//! Nothing else covered it either. `packages/cli-bin/tests/cli.rs` asserts that `zzop init` MENTIONS
//! the coverage command; no test ran that command and read what it said.
//!
//! # Why this file goes through the config crate rather than the shaper
//! The defect lives in the seam, so the test has to cross it: it renders the SHIPPED starter config,
//! loads it the way every host does, and checks the block against the file's own text. A fixture
//! request would reproduce the blind spot it exists to close.

use std::fs;

/// Every `vocabulary` key the starter config writes must be reported as DECLARED, never as silent.
///
/// The starter config is the one input every first run has — `zzop init` writes it and the reply's
/// next-step line sends the reader straight at this block — so it is the input whose answer has to be
/// right before any other.
#[test]
fn no_key_the_starter_config_declares_is_reported_silent() {
    let dir = std::env::temp_dir().join(format!("zzop-vdecl-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("zzop.config.jsonc"),
        zzop_config::template::CONFIG_TEMPLATE_JSONC,
    )
    .unwrap();
    fs::write(dir.join("src/a.ts"), "export const x = 1;\n").unwrap();

    let out = zzop_summary::coverage_summary(&[dir.display().to_string()], None)
        .expect("coverage must succeed on a configured tree");
    let reply: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    let row = &reply["vocabularyDeclared"][0];

    let silent: Vec<&str> = row["silent"]
        .as_array()
        .expect("silent is an array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    let declared_count = row["declared"].as_array().map_or(0, Vec::len);

    // FLOOR: the block has to be reporting something, or "nothing is wrongly silent" is a statement
    // about an empty reply. The starter config declares most of the surface by construction.
    assert!(
        declared_count >= 30,
        "the block reports {declared_count} declared key(s) — that is not a real starter config \
         reading, so the assertion below would be vacuous: {row}"
    );

    // The config file is its own answer key: a key whose quoted name appears in the text it shipped
    // was declared, whatever any later layer does with it.
    let config_text = zzop_config::template::CONFIG_TEMPLATE_JSONC;
    let wrongly_silent: Vec<&&str> = silent
        .iter()
        .filter(|k| config_text.contains(&format!("\"{k}\"")))
        .collect();
    assert!(
        wrongly_silent.is_empty(),
        "these keys are written in the starter config this run loaded and the reply calls them \
         SILENT: {wrongly_silent:?}\nA block that exists to report what was not judged is reporting a \
         declaration as unjudged, which is the one error it cannot afford. `workspaceSkipDirs` was \
         the first: the config front end consumes it and never forwards it, so reading the request \
         instead of the author's file makes every declaration of it invisible."
    );
}

/// The negative half. Without it, a `rows()` that reported EVERY key as declared would pass the test
/// above — and the whole point of the block is the split.
#[test]
fn a_key_the_config_does_not_declare_is_still_reported_silent() {
    let dir = std::env::temp_dir().join(format!("zzop-vdecl-min-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    // A minimal config: one vocabulary key, so everything else on the surface must read as silent.
    fs::write(
        dir.join("zzop.config.jsonc"),
        "{ \"roots\": [\".\"], \"vocabulary\": { \"skipDirs\": [\"dist\"] } }\n",
    )
    .unwrap();
    fs::write(dir.join("src/a.ts"), "export const x = 1;\n").unwrap();

    let out = zzop_summary::coverage_summary(&[dir.display().to_string()], None)
        .expect("coverage must succeed on a configured tree");
    let reply: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    let row = &reply["vocabularyDeclared"][0];

    let declared: Vec<&str> = row["declared"]
        .as_array()
        .expect("declared is an array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        declared,
        vec!["skipDirs"],
        "a config declaring one key must report exactly that one as declared: {row}"
    );
    assert!(
        row["silent"].as_array().map_or(0, Vec::len) >= 30,
        "the rest of the surface must read as silent: {row}"
    );
    // And the auth subset, which is the row a reader acts on first, must be non-empty here.
    assert!(
        row["silentAuth"].as_array().map_or(0, Vec::len) >= 3,
        "a config with no auth vocabulary must say so: {row}"
    );
}
