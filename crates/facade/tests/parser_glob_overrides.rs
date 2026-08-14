//! `parsers.globOverrides` — the config surface for parser ROUTING.
//!
//! The capability (`DispatchConfig::glob_overrides`) shipped long before this test: it was checked
//! ahead of the extension map, and its own doc said it existed "so a project can force-route paths the
//! extension map would otherwise miss or mis-tag". No config key, request field, CLI flag or MCP
//! argument reached it, so no project could. These tests pin the door, not the room.

use std::fs;

use zzop_facade::analyze_json;

fn tmp(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("zzop-glob-override-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A house extension the extension map has never heard of is invisible by default and analyzed once
/// the config routes it — the whole point of the knob, stated as a before/after on the SAME tree.
#[test]
fn a_house_extension_is_analyzed_only_once_a_glob_override_routes_it() {
    let root = tmp("house-ext");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src").join("api.houseml"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();

    let req = |parsers: &str| {
        format!(
            r#"{{"root":{root:?},"sourceId":"t","packDefs":[],"git":{{}}{parsers}}}"#,
            root = root.display().to_string(),
        )
    };

    // The discriminator is EXTRACTION, not `fileCount` — the walker counts the file either way, which is
    // precisely why the gap was easy to miss from the outside: a tree with an unroutable extension looks
    // fully analyzed by every count zzop reports about it, and only the io facts are empty.
    let consumes = |v: &serde_json::Value| -> Vec<String> {
        v["ir"]["io"]["consumes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| c["key"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };

    let before: serde_json::Value = serde_json::from_str(&analyze_json(&req("")).unwrap()).unwrap();
    assert!(
        consumes(&before).is_empty(),
        "an unknown extension must be extracted from nowhere until the config says otherwise, got: {:?}",
        consumes(&before)
    );

    let after: serde_json::Value = serde_json::from_str(
        &analyze_json(&req(
            r#","parsers":{"globOverrides":[{"glob":"**/*.houseml","language":"typescript"}]}"#,
        ))
        .unwrap(),
    )
    .unwrap();
    assert!(
        consumes(&after).iter().any(|k| k.contains("/api/users")),
        "the override must route the file to the TypeScript frontend and yield its io facts, got: {:?}",
        consumes(&after)
    );
}

/// `parsers.globOverrides` is an ADDITIVE tier, not a replacement one: it is consulted AHEAD of the
/// extension map and every path it does not match falls through to that map unchanged. Pinned because
/// `crates/facade/src/config/declared.rs`'s module doc stated the opposite until 2026-08-14 — it gave
/// "a declared value is applied WHOLE, the empty declaration included" as an unconditional rule covering
/// everything that file lands, and read that way a declared `parsers` object REPLACES parser routing
/// rather than prepending to it. The witness is a `.ts` file that no declared override matches: under
/// the whole-replacement reading it stops being parsed, under the real one the extension map still
/// answers for it.
#[test]
fn a_declared_override_that_matches_nothing_leaves_the_extension_map_answering() {
    let root = tmp("additive-tier");
    fs::write(
        root.join("a.ts"),
        "export const load = () => fetch('/api/users');\n",
    )
    .unwrap();
    let out = analyze_json(&format!(
        r#"{{"root":{:?},"sourceId":"t","packDefs":[],"git":{{}},"parsers":{{"globOverrides":[{{"glob":"legacy/**/*.houseml","language":"prisma"}}]}}}}"#,
        root.display().to_string()
    ))
    .expect("a declared override must not fail the run");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let consumes: Vec<String> = v["ir"]["io"]["consumes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["key"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        consumes.iter().any(|k| k.contains("/api/users")),
        "the extension map must still route .ts when a declared override does not match it — the tier \
         prepends, it does not replace: {consumes:?}"
    );
}

/// An unknown language name is an authoring mistake: it must be SAID, and it must not take the run
/// down. A silent skip would be the worst outcome — the file still gets analyzed by extension, so the
/// author sees plausible output and concludes the override worked.
#[test]
fn an_unknown_language_warns_by_name_and_leaves_the_run_standing() {
    let root = tmp("unknown-lang");
    fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
    let out = analyze_json(&format!(
        r#"{{"root":{:?},"sourceId":"t","packDefs":[],"git":{{}},"parsers":{{"globOverrides":[{{"glob":"**/*.zig","language":"zig"}}]}}}}"#,
        root.display().to_string()
    ))
    .expect("an unknown language must not fail the run");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = v["warnings"].as_array().cloned().unwrap_or_default();
    let named = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("zig"))
        .unwrap_or_else(|| panic!("the rejected language must be named: {warnings:?}"));
    assert!(
        named.contains("typescript") && named.contains("prisma"),
        "the warning must list what IS accepted, not just what was refused: {named}"
    );
    assert_eq!(
        v["fileCount"].as_u64().unwrap_or(0),
        1,
        "the rest of the tree must still be analyzed"
    );
}
