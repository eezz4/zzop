//! Exercises `examples/packs/sql-preferences.json`'s `select-star` and `like-leading-wildcard`
//! line-scan rules, moved verbatim from `rules/dsl/sql/select_like.rs` when both rules left the
//! bundle. Only the pack path and the qualified ids changed; the LANGUAGE half of the same two rules
//! lives in this directory's `language_scope.rs`.

use crate::{hits, scan, TempDir};

// --- select-star ---

#[test]
fn select_star_from_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write("q.ts", "export const q = \"SELECT * FROM users\";\n");
    let out = scan(&dir);
    let h = hits(&out, "select-star");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn select_count_star_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write("q.ts", "export const q = \"SELECT COUNT(*) FROM users\";\n");
    let out = scan(&dir);
    assert!(hits(&out, "select-star").is_empty(), "{:?}", out.findings);
}

#[test]
fn select_star_from_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write("tests/q.ts", "export const q = \"SELECT * FROM users\";\n");
    let out = scan(&dir);
    assert!(hits(&out, "select-star").is_empty(), "{:?}", out.findings);
}

#[test]
fn sql_select_star_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "q.ts",
        "// zzop-select-star-ok: internal debug dump, columns intentionally unbounded\nexport const q = \"SELECT * FROM users\";\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "select-star").is_empty(), "{:?}", out.findings);
}

// --- like-leading-wildcard ---

#[test]
fn like_leading_wildcard_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "search.ts",
        "export const q = \"SELECT id FROM users WHERE name LIKE '%term'\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "like-leading-wildcard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
}

#[test]
fn like_trailing_only_wildcard_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "search.ts",
        "export const q = \"SELECT id FROM users WHERE name LIKE 'term%'\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "like-leading-wildcard").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn like_leading_wildcard_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "tests/search.ts",
        "export const q = \"SELECT id FROM users WHERE name LIKE '%term'\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "like-leading-wildcard").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sql_like_leading_wildcard_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "search.ts",
        "// zzop-like-leading-wildcard-ok: tiny fixed lookup table, offline batch job\nexport const q = \"SELECT id FROM users WHERE name LIKE '%term'\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "like-leading-wildcard").is_empty(),
        "{:?}",
        out.findings
    );
}
