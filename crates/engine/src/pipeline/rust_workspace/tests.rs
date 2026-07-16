use super::*;

// --- parse_package_name: section-boundary awareness ---------------------------------------------------

#[test]
fn parse_package_name_reads_the_package_section_name() {
    let toml = "[package]\nname = \"zzop-core\"\nversion = \"0.0.0\"\n";
    assert_eq!(parse_package_name(toml), Some("zzop-core".to_string()));
}

#[test]
fn parse_package_name_tolerates_whitespace_and_single_quotes() {
    let toml = "[package]\nname   =   'zzop-core'\n";
    assert_eq!(parse_package_name(toml), Some("zzop-core".to_string()));
}

#[test]
fn parse_package_name_ignores_a_name_key_under_dependencies() {
    // The load-bearing regression case: a `name =` line under `[dependencies]` (or any OTHER section)
    // must NOT be picked up as the package's own name.
    let toml = "[dependencies]\nname = \"not-the-package\"\n\n[package]\nname = \"real-name\"\n";
    assert_eq!(parse_package_name(toml), Some("real-name".to_string()));
}

#[test]
fn parse_package_name_returns_none_when_no_package_section_at_all() {
    let toml = "[dependencies]\nserde = \"1\"\n";
    assert_eq!(parse_package_name(toml), None);
}

#[test]
fn parse_package_name_stops_at_the_next_bracket_section() {
    let toml = "[package]\nedition = \"2021\"\n\n[[bin]]\nname = \"cli-bin-name\"\n";
    // `[package]` has no `name` key of its own here (only `edition`); the `[[bin]]` table's `name` must
    // not be mistaken for the package's own name.
    assert_eq!(parse_package_name(toml), None);
}

#[test]
fn parse_package_name_ignores_a_commented_out_name_line() {
    let toml = "[package]\n# name = \"commented\"\nname = \"real\"\n";
    assert_eq!(parse_package_name(toml), Some("real".to_string()));
}

// --- scan_rust_workspace: end-to-end manifest discovery ------------------------------------------------

#[test]
fn scan_rust_workspace_maps_raw_and_underscore_normalized_names() {
    let dir = std::env::temp_dir().join(format!(
        "zzop-rust-workspace-scan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("crates/core/src")).unwrap();
    std::fs::write(
        dir.join("crates/core/Cargo.toml"),
        "[package]\nname = \"zzop-core\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("crates/core/src/lib.rs"), "pub fn f() {}\n").unwrap();

    let walked = vec!["crates/core/src/lib.rs"];
    let map = scan_rust_workspace(&dir, walked.into_iter());

    assert_eq!(
        map.get("zzop-core"),
        Some(&vec![
            "crates/core/src/lib.rs".to_string(),
            "crates/core/src/main.rs".to_string(),
        ])
    );
    assert_eq!(
        map.get("zzop_core"),
        Some(&vec![
            "crates/core/src/lib.rs".to_string(),
            "crates/core/src/main.rs".to_string(),
        ])
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_rust_workspace_ignores_a_directory_with_no_cargo_toml() {
    let dir = std::env::temp_dir().join(format!(
        "zzop-rust-workspace-scan-none-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();

    let walked = vec!["src/main.rs"];
    let map = scan_rust_workspace(&dir, walked.into_iter());
    assert!(map.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_target_paths_reads_only_target_shaped_sections() {
    let text = r#"
[package]
name = "demo"
path = "not-a-target.rs"

[dependencies]
path = "also-not-a-target.rs"

[lib]
path = "src/custom_lib.rs"

[[test]]
name = "http"
path = "dsl/http/http.rs"

[[bin]]
# a comment line inside the section is skipped
path = "tools/gen.rs"

[[example]]
path = "examples_alt/demo.rs"

[[bench]]
path = "perf/bench_main.rs"
"#;
    assert_eq!(
        parse_target_paths(text),
        vec![
            "src/custom_lib.rs".to_string(),
            "dsl/http/http.rs".to_string(),
            "tools/gen.rs".to_string(),
            "examples_alt/demo.rs".to_string(),
            "perf/bench_main.rs".to_string(),
        ]
    );
}

#[test]
fn declared_rust_target_paths_resolves_against_the_manifest_directory() {
    let dir = std::env::temp_dir().join(format!(
        "zzop-rust-target-paths-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("rules/dsl/http")).unwrap();
    std::fs::write(
        dir.join("rules/Cargo.toml"),
        "[package]\nname = \"rules\"\n\n[[test]]\nname = \"http\"\npath = \"dsl/http/http.rs\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("rules/dsl/http/http.rs"), "// pack test\n").unwrap();

    let walked = vec!["rules/dsl/http/http.rs"];
    let entries = declared_rust_target_paths(&dir, walked.into_iter());
    assert_eq!(
        entries.into_iter().collect::<Vec<_>>(),
        vec!["rules/dsl/http/http.rs".to_string()]
    );

    let _ = std::fs::remove_dir_all(&dir);
}
