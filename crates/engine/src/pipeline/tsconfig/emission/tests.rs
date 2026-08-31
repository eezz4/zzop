//! Guard (c) at its source: which `compilerOptions` spellings turn the export-side cycle arm off, and
//! — the half that matters more — which ones do NOT, so the gate cannot quietly disable the whole
//! feature on an ordinary tree.

use super::*;
use crate::pipeline::testutil::TempDir;

fn scan(dir: &TempDir, rels: &[&str]) -> bool {
    tsconfig_preserves_type_imports(dir.path(), rels.iter().map(|r| r.to_string()))
}

#[test]
fn verbatim_module_syntax_preserves() {
    let dir = TempDir::new("zzop-tsconfig-verbatim");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    assert!(scan(&dir, &["tsconfig.json"]));
}

#[test]
fn preserve_value_imports_preserves() {
    let dir = TempDir::new("zzop-tsconfig-pvi");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"preserveValueImports": true}}"#,
    );
    assert!(scan(&dir, &["tsconfig.json"]));
}

#[test]
fn imports_not_used_as_values_preserve_preserves() {
    let dir = TempDir::new("zzop-tsconfig-inuav");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"importsNotUsedAsValues": "preserve"}}"#,
    );
    assert!(scan(&dir, &["tsconfig.json"]));
}

/// The control: `importsNotUsedAsValues` has other values, and the two boolean keys can be written
/// FALSE. None of those preserves anything, and reading them as if they did would switch the export-
/// side arm off on ordinary trees — a silent, total loss of the feature with no finding to notice.
#[test]
fn the_off_spellings_do_not_preserve() {
    let dir = TempDir::new("zzop-tsconfig-off");
    dir.write(
        "a/tsconfig.json",
        r#"{"compilerOptions": {"importsNotUsedAsValues": "remove"}}"#,
    );
    dir.write(
        "b/tsconfig.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": false, "preserveValueImports": false}}"#,
    );
    dir.write(
        "c/tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": ".", "strict": true}}"#,
    );
    assert!(!scan(
        &dir,
        &["a/tsconfig.json", "b/tsconfig.json", "c/tsconfig.json"]
    ));
}

/// The measured corpus shape: a monorepo puts the flag in a shared base config every package
/// `extends`. One level of local relative `extends` is followed, matching `load_effective`.
#[test]
fn a_locally_extended_base_config_preserves() {
    let dir = TempDir::new("zzop-tsconfig-extends");
    dir.write(
        "tsconfig.base.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    dir.write(
        "pkg/tsconfig.json",
        r#"{"extends": "../tsconfig.base", "compilerOptions": {"strict": true}}"#,
    );
    assert!(scan(&dir, &["pkg/tsconfig.json"]));
}

#[test]
fn a_bare_package_extends_is_not_chased() {
    let dir = TempDir::new("zzop-tsconfig-bare-extends");
    dir.write(
        "tsconfig.json",
        r#"{"extends": "@tsconfig/node20/tsconfig.json"}"#,
    );
    assert!(!scan(&dir, &["tsconfig.json"]));
}

/// One config anywhere in the tree is enough — the verdict is tree-level, and it only ever KEEPS
/// findings, so the generous direction is the safe one.
#[test]
fn any_config_in_the_tree_decides_for_the_whole_tree() {
    let dir = TempDir::new("zzop-tsconfig-anyconfig");
    dir.write("tsconfig.json", r#"{"compilerOptions": {"strict": true}}"#);
    dir.write(
        "apps/web/tsconfig.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    assert!(scan(&dir, &["tsconfig.json", "apps/web/tsconfig.json"]));
}

#[test]
fn unreadable_and_invalid_configs_degrade_to_false_without_panicking() {
    let dir = TempDir::new("zzop-tsconfig-degrade");
    dir.write("tsconfig.json", "{ this is not json");
    assert!(!scan(
        &dir,
        &["tsconfig.json", "does/not/exist/tsconfig.json"]
    ));
}

#[test]
fn jsonc_comments_and_trailing_commas_are_tolerated() {
    let dir = TempDir::new("zzop-tsconfig-jsonc");
    dir.write(
        "tsconfig.json",
        "{\n  // the flag\n  \"compilerOptions\": { \"verbatimModuleSyntax\": true, },\n}",
    );
    assert!(scan(&dir, &["tsconfig.json"]));
}
