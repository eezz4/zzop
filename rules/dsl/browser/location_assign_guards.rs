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
        "declare const target: string;\nexport function go() {\n  // zzop-location-assign-dynamic-ok: target is checked against an allowlist above\n  location.href = target;\n}\n",
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

/// **An arrow PARAMETER named `location` is not an assignment**, and until 2026-08-21 it was read as one:
/// the negated class after `=` excluded `=` but not `>`, so `location => {` matched as `location = >`.
/// Reproduced twice in apache/superset (`transformPropsUtil.ts`, `ScatterPlotOverlay.tsx`), where the
/// printed snippet contained no navigation sink at all — which makes a reviewer distrust the printer
/// before the rule. The rule's own message promises "only the bare global `location`/`window.location`
/// is flagged"; a callback parameter is neither.
///
/// Three arrow shapes and the real sink in one call, because "arrows are quiet" and "the rule is dead"
/// are the same assertion without the positive control.
#[test]
fn an_arrow_parameter_named_location_is_not_a_navigation_sink() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "src/geo.ts",
        "export function plot(byLoc: Record<string, number>, marks: string[], userInput: string) {\n\
        \x20 Object.keys(byLoc).forEach(location => { void location; });\n\
        \x20 marks.map(location => location.length);\n\
        \x20 const pick = (location: string) => location;\n\
        \x20 void pick;\n\
        \x20 window.location.href = userInput;\n\
        }\n",
    );
    let out = scan(&dir);
    let lines: Vec<u32> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .map(|f| f.line)
        .collect();
    assert_eq!(
        lines,
        vec![6],
        "only the real `window.location.href =` sink on line 6 may fire: {:?}",
        out.findings
    );
}
