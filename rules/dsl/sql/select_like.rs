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
        "// sql-select-star-ok: internal debug dump, columns intentionally unbounded\nexport const q = \"SELECT * FROM users\";\n",
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
        "// sql-like-leading-wildcard-ok: tiny fixed lookup table, offline batch job\nexport const q = \"SELECT id FROM users WHERE name LIKE '%term'\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "like-leading-wildcard").is_empty(),
        "{:?}",
        out.findings
    );
}
