use crate::{scan, TempDir};

/// Receiver-aware claim: an arbitrary domain object's own `.location` field is never matched — only the
/// bare global/`window.` form is.
#[test]
fn unrelated_object_location_field_assignment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "user.ts",
        "declare const user: { location: string };\ndeclare const newAddress: string;\nexport function move() {\n  user.location = newAddress;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

/// `const location = useLocation();` (React Router) is a declaration, not a navigation assignment.
#[test]
fn use_location_hook_declaration_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "route.tsx",
        "declare function useLocation(): { pathname: string };\nexport function Page() {\n  const location = useLocation();\n  return location.pathname;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

#[test]
fn location_assign_dynamic_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented-loc.ts",
        "declare const target: string;\nexport function go() {\n  // location.href = target; -- old, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

#[test]
fn location_assign_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-loc.ts",
        "declare const target: string;\nexport function go() {\n  // location-assign-ok: target is checked against an allowlist above\n  location.href = target;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

#[test]
fn location_assign_dynamic_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/redirect.ts",
        "declare const target: string;\nexport function go() {\n  location.href = target;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}
