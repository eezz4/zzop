use crate::{scan, TempDir};

// --- location-assign-dynamic ---

#[test]
fn bare_location_assigned_a_dynamic_value_is_flagged_location_href() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect.ts",
        "declare const target: string;\nexport function go() {\n  location = target;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

#[test]
fn window_location_href_assigned_a_dynamic_value_is_flagged_location_href() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect2.ts",
        "declare const target: string;\nexport function go() {\n  window.location.href = target;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

#[test]
fn location_assign_call_with_a_dynamic_value_is_flagged_location_assign_call() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect3.ts",
        "declare const target: string;\nexport function go() {\n  location.assign(target);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-assign-call")
    );
}

#[test]
fn location_replace_call_with_a_dynamic_value_is_flagged_location_assign_call() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect4.ts",
        "declare const target: string;\nexport function go() {\n  window.location.replace(target);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-assign-call")
    );
}

#[test]
fn location_href_relative_path_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "login.ts",
        "export function goLogin() {\n  location.href = '/login';\n}\n",
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
fn location_href_absolute_url_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "external.ts",
        "export function goExternal() {\n  location.href = \"https://x.com\";\n}\n",
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

/// Calibration pin (opus-reviewer): the immich `getBaseUrl() + '/admin/…/' + filename` shape is NOT a
/// navigation sink — prepending a base pins the origin/path, so the trailing dynamic segment is a path
/// component, not a scheme/origin. The `exclude_pattern`'s `+ '/…'` path-literal-concat alternative vetoes
/// it. This is the false positive that motivated adding that alternative.
#[test]
fn base_plus_path_literal_concat_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "backup.ts",
        "declare function getBaseUrl(): string;\ndeclare const filename: string;\nexport function download() {\n  location.href = getBaseUrl() + '/admin/database-backups/' + filename;\n}\n",
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

/// Literal-first concat (`'/admin/' + x`): the RHS opens with a string path literal, so the navigation
/// path is pinned by that leading literal. Silent for two independent reasons — the `line_pattern`'s
/// negative class rejects a leading quote, AND the `exclude_pattern` would veto a `+ '/…'` concat anyway —
/// this pins the literal-first form the previous suite only covered for a whole-literal `"https://x.com"`.
#[test]
fn literal_first_path_concat_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "adminnav.ts",
        "declare const x: string;\nexport function go() {\n  location.href = '/admin/' + x;\n}\n",
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

/// TP preserved after the exclude: a bare dynamic value (`location.href = returnUrl`) is a classic open
/// redirect — nothing pins the destination, no `+ '/…'` concat to veto — must still fire.
#[test]
fn bare_dynamic_return_url_still_fires_after_the_concat_exclude() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect5.ts",
        "declare const returnUrl: string;\nexport function go() {\n  location.href = returnUrl;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

/// TP preserved after the exclude: a query-suffix concat (`base + '?next=' + q`) does NOT pin the
/// destination origin — the `+ '?…'` literal is a query string, not a `+ '/…'` path literal, so the
/// exclude's path-concat alternative does not match it and the finding still fires.
#[test]
fn base_plus_query_suffix_concat_still_fires_after_the_concat_exclude() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "querysuffix.ts",
        "declare const base: string;\ndeclare const q: string;\nexport function go() {\n  location.href = base + '?next=' + q;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

/// The two shapes that LOOK like an assignment to the global and are not, plus the control that a real
/// sink on the same axis still fires. Both were measured, not imagined: koel's `openPopup` passes
/// `window.open` a feature string whose `, location=no` is lexically an assignment (1 of that repo's
/// 2 findings from this rule), and grafana routes a `location` PROP (`<GrafanaRoute location={location} />`,
/// 6 lines). The discriminator is notation rather than vocabulary — a feature string and a JSX prop
/// write `k=v` with no space, and no formatter writes a JS assignment that way — so it needs no list
/// of feature names or component names to stay correct as either grows.
#[test]
fn a_feature_string_and_a_jsx_prop_are_not_assignments_but_a_spaced_assignment_still_fires() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "popup.ts",
        concat!(
            "export const openPopup = (url: string, w: number, h: number, parent: Window) => {\n",
            "  return parent.open(url, \"p\", \"toolbar=no, location=no, directories=no, width=\" + w)\n",
            "}\n",
        ),
    );
    dir.write(
        "Route.tsx",
        concat!(
            "declare const location: any;\n",
            "declare const route: any;\n",
            "export const R = () => <GrafanaRoute route={route} location={location} />;\n",
        ),
    );
    // Two controls, in the same assertion. A real dynamic sink written the way source is written —
    // and a `window.`-QUALIFIED write with no space, which the whitespace rule must NOT swallow:
    // neither false-positive shape can produce one (a feature string writes bare `location=no`, and
    // `<C window.location={x}/>` is not valid JSX), so requiring the space there would have cost
    // recall for nothing. That branch is what the first version of this pin failed to hold.
    dir.write(
        "nav.ts",
        "declare const returnUrl: string;\nexport function go() {\n  location.href = returnUrl;\n}\n",
    );
    dir.write(
        "legacy.ts",
        "declare const url: string;\nexport function go() {\n  window.location=url;\n}\n",
    );
    let out = scan(&dir);
    let mut files: Vec<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .map(|f| f.file.as_str())
        .collect();
    files.sort_unstable();
    assert_eq!(
        files,
        vec!["legacy.ts", "nav.ts"],
        "the two assignments are sinks and the feature string and JSX prop are not: {:?}",
        out.findings
    );
}
