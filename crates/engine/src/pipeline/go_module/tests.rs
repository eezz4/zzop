use super::*;

// --- parse_go_module_path: first-token-on-the-line discipline -------------------------------------------

#[test]
fn parse_go_module_path_reads_the_module_directive() {
    let text = "module example.com/app\n\ngo 1.21\n";
    assert_eq!(
        parse_go_module_path(text),
        Some("example.com/app".to_string())
    );
}

#[test]
fn parse_go_module_path_tolerates_a_trailing_line_comment() {
    let text = "module example.com/app // the module\n";
    assert_eq!(
        parse_go_module_path(text),
        Some("example.com/app".to_string())
    );
}

#[test]
fn parse_go_module_path_ignores_a_commented_out_line() {
    let text = "// module fake.example.com/not-real\nmodule example.com/real\n";
    assert_eq!(
        parse_go_module_path(text),
        Some("example.com/real".to_string())
    );
}

#[test]
fn parse_go_module_path_requires_module_as_the_lines_own_first_token() {
    // A `require` entry naming a dependency literally containing "module" in its path must not match.
    let text = "require example.com/some-module v1.0.0\n";
    assert_eq!(parse_go_module_path(text), None);
}

#[test]
fn parse_go_module_path_returns_none_when_no_module_directive_at_all() {
    let text = "go 1.21\n\nrequire (\n\tgithub.com/some/dep v1.0.0\n)\n";
    assert_eq!(parse_go_module_path(text), None);
}

#[test]
fn parse_go_module_path_tolerates_leading_whitespace() {
    let text = "  module   example.com/app  \n";
    assert_eq!(
        parse_go_module_path(text),
        Some("example.com/app".to_string())
    );
}

// --- scan_go_modules: end-to-end manifest discovery ------------------------------------------------------

fn temp_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "zzop-go-module-scan-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn scan_go_modules_maps_the_manifest_directory_to_its_module_path() {
    let dir = temp_dir("basic");
    std::fs::create_dir_all(dir.join("internal/db")).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/app\n\ngo 1.21\n").unwrap();
    std::fs::write(dir.join("main.go"), "package main\n").unwrap();
    std::fs::write(dir.join("internal/db/db.go"), "package db\n").unwrap();

    let walked = vec!["main.go", "internal/db/db.go"];
    let map = scan_go_modules(&dir, walked.into_iter());

    assert_eq!(map.get(""), Some(&"example.com/app".to_string()));
    assert_eq!(map.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_go_modules_ignores_a_directory_with_no_go_mod() {
    let dir = temp_dir("none");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.go"), "package main\n").unwrap();

    let walked = vec!["src/main.go"];
    let map = scan_go_modules(&dir, walked.into_iter());
    assert!(map.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_go_modules_supports_a_nested_second_module() {
    let dir = temp_dir("nested");
    std::fs::create_dir_all(dir.join("vendor/nested")).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/app\n").unwrap();
    std::fs::write(dir.join("main.go"), "package main\n").unwrap();
    std::fs::write(
        dir.join("vendor/nested/go.mod"),
        "module example.com/nested\n",
    )
    .unwrap();
    std::fs::write(dir.join("vendor/nested/lib.go"), "package nested\n").unwrap();

    let walked = vec!["main.go", "vendor/nested/lib.go"];
    let map = scan_go_modules(&dir, walked.into_iter());

    assert_eq!(map.get(""), Some(&"example.com/app".to_string()));
    assert_eq!(
        map.get("vendor/nested"),
        Some(&"example.com/nested".to_string())
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// --- governing_go_module: nearest-ancestor-wins resolution -------------------------------------------

#[test]
fn governing_go_module_finds_the_root_manifest() {
    let mut modules = GoModuleMap::new();
    modules.insert(String::new(), "example.com/app".to_string());
    assert_eq!(
        governing_go_module("internal/db/db.go", &modules),
        Some(("", "example.com/app"))
    );
}

#[test]
fn governing_go_module_prefers_the_nearest_ancestor_over_the_root() {
    let mut modules = GoModuleMap::new();
    modules.insert(String::new(), "example.com/app".to_string());
    modules.insert(
        "vendor/nested".to_string(),
        "example.com/nested".to_string(),
    );
    assert_eq!(
        governing_go_module("vendor/nested/lib.go", &modules),
        Some(("vendor/nested", "example.com/nested"))
    );
    // A file OUTSIDE the nested module's own directory still resolves to the outer (root) module.
    assert_eq!(
        governing_go_module("main.go", &modules),
        Some(("", "example.com/app"))
    );
}

#[test]
fn governing_go_module_returns_none_when_no_ancestor_has_a_manifest() {
    let modules = GoModuleMap::new();
    assert_eq!(governing_go_module("main.go", &modules), None);
}

// --- join_dir ---------------------------------------------------------------------------------------

#[test]
fn join_dir_handles_the_module_root_empty_segment() {
    assert_eq!(join_dir("vendor/nested", ""), "vendor/nested");
    assert_eq!(join_dir("", ""), "");
}

#[test]
fn join_dir_joins_a_non_empty_segment() {
    assert_eq!(join_dir("", "internal/db"), "internal/db");
    assert_eq!(
        join_dir("vendor/nested", "internal/db"),
        "vendor/nested/internal/db"
    );
}
