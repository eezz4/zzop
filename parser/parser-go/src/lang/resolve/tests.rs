use super::*;

const MODULE: &str = "github.com/acme/app";

#[test]
fn nested_import_resolves_to_relative_dir() {
    assert_eq!(
        go_package_dir_of("github.com/acme/app/internal/db", MODULE),
        Some("internal/db".to_string())
    );
}

#[test]
fn single_segment_nested_import() {
    assert_eq!(
        go_package_dir_of("github.com/acme/app/handlers", MODULE),
        Some("handlers".to_string())
    );
}

#[test]
fn module_root_import_resolves_to_empty_dir() {
    assert_eq!(go_package_dir_of(MODULE, MODULE), Some(String::new()));
}

#[test]
fn non_matching_external_import_resolves_to_none() {
    assert_eq!(go_package_dir_of("github.com/gin-gonic/gin", MODULE), None);
}

#[test]
fn stdlib_import_resolves_to_none() {
    assert_eq!(go_package_dir_of("net/http", MODULE), None);
}

#[test]
fn prefix_collision_without_slash_boundary_does_not_match() {
    // "github.com/acme/apples" must NOT match module "github.com/acme/app" — a naive
    // `starts_with` (without the `/` boundary) would incorrectly match here.
    assert_eq!(go_package_dir_of("github.com/acme/apples", MODULE), None);
}

#[test]
fn module_path_itself_as_prefix_with_no_remaining_segment_is_none() {
    // "github.com/acme/app/" (trailing-slash-only import path) — degenerate, no real segment.
    assert_eq!(go_package_dir_of("github.com/acme/app/", MODULE), None);
}

#[test]
fn empty_module_path_never_resolves() {
    assert_eq!(go_package_dir_of("github.com/acme/app", ""), None);
}
