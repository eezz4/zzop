//! Contracts 6-7: pack-load determinism and pack-folder test wiring.

use std::fs;
use std::path::{Path, PathBuf};

use zzop_core::{load_dsl_packs, RulePackDef};

use crate::dsl_dir;

// ---------------------------------------------------------------------------------------------
// 6. Determinism guard — pack-load order/content must not depend on OS directory-iteration order
// ---------------------------------------------------------------------------------------------

/// Loading `rules/dsl` twice must yield the same packs, in the same order, with the same content.
/// `RulePackDef` (and everything it nests: `RuleDef`, `Matcher`, ...) derives `Debug` but not
/// `Serialize`/`PartialEq`, so this test uses `{:?}` (Debug) formatting as a pragmatic serialization
/// stand-in for the equality check — good enough to catch the class of bug this guards against (a
/// nondeterministic map/directory-listing iteration order leaking into parsed field/rule order), which is
/// exactly what `pack_loader::load_dsl_packs`'s own "sorted by file name" doc comment promises never
/// happens.
#[test]
fn loading_the_same_packs_dir_twice_yields_identical_pack_lists() {
    let dir = dsl_dir();
    let first = load_dsl_packs(&dir);
    let second = load_dsl_packs(&dir);

    assert_eq!(
        first.errors.len(),
        second.errors.len(),
        "load-error count differs between two loads of the same directory"
    );
    assert_eq!(
        first.packs.len(),
        second.packs.len(),
        "pack count differs between two loads of the same directory"
    );

    let first_ids: Vec<&str> = first.packs.iter().map(|(_, p)| p.id.as_str()).collect();
    let second_ids: Vec<&str> = second.packs.iter().map(|(_, p)| p.id.as_str()).collect();
    assert_eq!(
        first_ids, second_ids,
        "pack load ORDER differs between two loads of the same directory"
    );

    for ((path_a, pack_a), (path_b, pack_b)) in first.packs.iter().zip(second.packs.iter()) {
        assert_eq!(
            path_a, path_b,
            "pack path differs at the same index between two loads"
        );
        assert_eq!(
            format!("{pack_a:?}"),
            format!("{pack_b:?}"),
            "pack `{}` deserialized differently across two loads of the same file",
            pack_a.id
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 7. Pack-folder test wiring — every non-stub pack folder has a co-located <pack>.rs AND a
//    matching [[test]] entry in rules/Cargo.toml
// ---------------------------------------------------------------------------------------------

/// Reads `rules/Cargo.toml`'s raw text so the pack-folder-wiring test below can pragmatically check for a
/// `path = "dsl/<pack>/<pack>.rs"` substring, without pulling in a TOML parser dependency this workspace
/// otherwise has no use for (same "keep it pragmatic" approach as contract 3's grep-based check).
fn rule_packs_cargo_toml_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/Cargo.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Every `rules/dsl/<pack>/` folder that ships at least one rule must have a co-located `<pack>.rs`
/// (`rules/dsl/<pack>/<pack>.rs`) AND a matching `[[test]]` entry in `rules/Cargo.toml` whose `path` points
/// at it — otherwise the pack's end-to-end coverage would silently never run under `cargo test
/// --workspace`. Stub packs (0 rules — see `rules/README.md`'s "Stub packs") are exempt: there is nothing
/// to exercise yet. Pragmatic textual check (no TOML parser dependency, no AST comparison): looks for the
/// literal `dsl/<pack>/<pack>.rs` substring (forward slashes, as Cargo requires even on Windows) anywhere
/// in `rules/Cargo.toml`'s text.
#[test]
fn every_non_stub_pack_folder_has_a_colocated_tests_rs_and_a_cargo_toml_test_entry() {
    let dsl_root = dsl_dir();
    let entries = fs::read_dir(&dsl_root)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", dsl_root.display()));
    let cargo_toml_text = rule_packs_cargo_toml_text();

    let mut pack_dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    pack_dirs.sort();

    let mut offenders = Vec::new();
    for pack_dir in pack_dirs {
        let pack_name = pack_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("pack dir has a UTF-8 name")
            .to_string();

        // This pack's own JSON file(s) directly under the folder (mirrors load_dsl_packs's depth-1
        // subdirectory scan) — sum rule counts across them (normally exactly one file per folder).
        let json_files: Vec<PathBuf> = fs::read_dir(&pack_dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", pack_dir.display()))
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();

        let rule_count: usize = json_files
            .iter()
            .map(|p| {
                let text = fs::read_to_string(p)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()));
                let pack: RulePackDef = serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("failed to parse {}: {e}", p.display()));
                pack.rules.len()
            })
            .sum();

        if rule_count == 0 {
            continue; // stub pack — exempt
        }

        let tests_rs = pack_dir.join(format!("{pack_name}.rs"));
        if !tests_rs.is_file() {
            offenders.push(format!(
                "rules/dsl/{pack_name}/ ships {rule_count} rule(s) but has no co-located {pack_name}.rs"
            ));
            continue;
        }

        let expected_path_fragment = format!("dsl/{pack_name}/{pack_name}.rs");
        if !cargo_toml_text.contains(&expected_path_fragment) {
            offenders.push(format!(
                "rules/dsl/{pack_name}/{pack_name}.rs exists but rules/Cargo.toml has no [[test]] entry \
                 with path = \"{expected_path_fragment}\""
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "pack-folder test wiring drift: {offenders:#?}"
    );
}
