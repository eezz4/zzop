//! Covers `classify_path`'s four FSD layers, `module_of`/`module_root`'s slice/base/top-level/external
//! resolution, `top_subdir`, `dir_for`, `is_upward_import`'s exemptions, `is_external`, `round`, and
//! `is_index_barrel` — all against `ScoresConfig::default()`.
use super::*;

fn cfg() -> ScoresConfig {
    ScoresConfig::default()
}

#[test]
fn classify_path_l1_entry_prefix() {
    let c = cfg();
    assert_eq!(
        classify_path(&c, "pages/home/Index.tsx"),
        PathClass {
            layer: 1,
            slice: None
        }
    );
    assert_eq!(
        classify_path(&c, "api/users.ts"),
        PathClass {
            layer: 1,
            slice: None
        }
    );
}

#[test]
fn classify_path_l1_top_level_no_slash() {
    let c = cfg();
    assert_eq!(
        classify_path(&c, "index.ts"),
        PathClass {
            layer: 1,
            slice: None
        }
    );
}

#[test]
fn classify_path_l2_slice_carries_slice_id() {
    let c = cfg();
    assert_eq!(
        classify_path(&c, "features/auth/model.ts"),
        PathClass {
            layer: 2,
            slice: Some("features/auth".to_string())
        }
    );
    assert_eq!(
        classify_path(&c, "domains/billing/api.ts"),
        PathClass {
            layer: 2,
            slice: Some("domains/billing".to_string())
        }
    );
}

#[test]
fn classify_path_l3_shared_prefix() {
    let c = cfg();
    assert_eq!(
        classify_path(&c, "core/util.ts"),
        PathClass {
            layer: 3,
            slice: None
        }
    );
    assert_eq!(
        classify_path(&c, "ui/Button.tsx"),
        PathClass {
            layer: 3,
            slice: None
        }
    );
}

#[test]
fn classify_path_l4_base_relative_or_unrecognized() {
    let c = cfg();
    assert_eq!(
        classify_path(&c, "../sibling/x.ts"),
        PathClass {
            layer: 4,
            slice: None
        }
    );
    assert_eq!(
        classify_path(&c, "src/base/thing/x.ts"),
        PathClass {
            layer: 4,
            slice: None
        }
    );
    assert_eq!(
        classify_path(&c, "app/main.ts"),
        PathClass {
            layer: 4,
            slice: None
        }
    );
}

#[test]
fn module_of_returns_slice_for_l2_path() {
    assert_eq!(
        module_of(&cfg(), "features/auth/model.ts"),
        Some("features/auth".to_string())
    );
}

#[test]
fn module_of_returns_base_module() {
    assert_eq!(
        module_of(&cfg(), "src/base/thing/x.ts"),
        Some("base/thing".to_string())
    );
}

#[test]
fn module_of_falls_back_to_top_level_dir() {
    assert_eq!(module_of(&cfg(), "app/main.ts"), Some("app".to_string()));
}

#[test]
fn module_of_null_for_external_specifiers() {
    assert_eq!(module_of(&cfg(), "react"), None);
    assert_eq!(module_of(&cfg(), "@scope/pkg"), None);
}

#[test]
fn module_root_returns_slice_for_l2_path() {
    assert_eq!(
        module_root(&cfg(), "features/auth/model.ts"),
        Some("features/auth".to_string())
    );
}

#[test]
fn module_root_returns_base_module() {
    assert_eq!(
        module_root(&cfg(), "src/base/thing/x.ts"),
        Some("base/thing".to_string())
    );
}

#[test]
fn module_root_no_top_level_fallback() {
    assert_eq!(module_root(&cfg(), "app/main.ts"), None);
}

#[test]
fn module_root_null_for_external_specifiers() {
    assert_eq!(module_root(&cfg(), "react"), None);
}

#[test]
fn top_subdir_returns_first_subdir_under_module_root() {
    assert_eq!(
        top_subdir("features/auth/widgets/Form.tsx", "features/auth"),
        Some("widgets".to_string())
    );
}

#[test]
fn top_subdir_null_when_tail_is_a_file() {
    assert_eq!(top_subdir("features/auth/model.ts", "features/auth"), None);
}

#[test]
fn top_subdir_null_when_module_root_absent() {
    assert_eq!(top_subdir("features/other/x.ts", "features/auth"), None);
}

#[test]
fn dir_for_returns_directory_portion() {
    assert_eq!(dir_for("a/b/c.ts"), "a/b");
}

#[test]
fn dir_for_empty_when_no_slash() {
    assert_eq!(dir_for("x.ts"), "");
}

#[test]
fn is_upward_import_true_child_imports_ancestor_same_module() {
    // from dir "features/auth/widgets/inner" -> to dir "features/auth/widgets" (ancestor)
    // toLast "widgets" not shared, not an index barrel, topSubdir(to, "features/auth") = "widgets" != null
    assert!(is_upward_import(
        &cfg(),
        "features/auth/widgets/inner/Form.tsx",
        "features/auth/widgets/helper.ts"
    ));
}

#[test]
fn is_upward_import_false_sibling_import() {
    assert!(!is_upward_import(
        &cfg(),
        "features/auth/ui/Form.tsx",
        "features/auth/utils/format.ts"
    ));
}

#[test]
fn is_upward_import_false_same_directory() {
    assert!(!is_upward_import(
        &cfg(),
        "features/auth/ui/Form.tsx",
        "features/auth/ui/Button.tsx"
    ));
}

#[test]
fn is_upward_import_false_ancestor_is_shared_dir() {
    assert!(!is_upward_import(
        &cfg(),
        "features/auth/utils/inner/x.ts",
        "features/auth/utils/helper.ts"
    ));
}

#[test]
fn is_upward_import_false_importing_index_barrel() {
    assert!(!is_upward_import(
        &cfg(),
        "features/auth/widgets/inner/Form.tsx",
        "features/auth/widgets/index.ts"
    ));
}

#[test]
fn is_external_true_for_scoped_and_bare_specifiers() {
    assert!(is_external("@scope/pkg"));
    assert!(is_external("react"));
}

#[test]
fn is_external_false_for_relative_or_pathed_specifiers() {
    assert!(!is_external("./local"));
    assert!(!is_external("a/b"));
}

#[test]
fn round_rounds_half_up() {
    assert_eq!(round(1.5), 2.0);
    assert_eq!(round(1.4), 1.0);
    assert_eq!(round(2.5), 3.0);
}

#[test]
fn is_index_barrel_true_for_known_extensions() {
    assert!(is_index_barrel("index.ts"));
    assert!(is_index_barrel("index.tsx"));
    assert!(is_index_barrel("index.js"));
    assert!(is_index_barrel("index.mjs"));
    assert!(is_index_barrel("index.cjs"));
}

#[test]
fn is_index_barrel_false_for_non_barrel_basenames() {
    assert!(!is_index_barrel("model.ts"));
    assert!(!is_index_barrel("indexs.ts"));
    assert!(!is_index_barrel("index.css"));
}
