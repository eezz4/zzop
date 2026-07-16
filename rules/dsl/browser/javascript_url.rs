use crate::{scan, TempDir};

// --- javascript-url ---

#[test]
fn jsx_href_literal_javascript_url_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Link.tsx",
        "export function Link() {\n  return <a href=\"javascript:alert(1)\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/javascript-url")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("jsx-href-literal")
    );
}

#[test]
fn href_property_assignment_to_javascript_url_is_flagged_href_assign() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "nav.ts",
        "declare const a: HTMLAnchorElement;\nexport function wire() {\n  a.href = 'javascript:void(0)';\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/javascript-url")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("href-assign")
    );
}

#[test]
fn set_attribute_javascript_url_is_flagged_setattr_js() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "setattr.ts",
        "declare const a: HTMLAnchorElement;\nexport function wire() {\n  a.setAttribute('href', 'javascript:doIt()');\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/javascript-url")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("setattr-js")
    );
}

#[test]
fn https_href_is_not_flagged_javascript_url() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "safe-link.tsx",
        "export function Link() {\n  return <a href=\"https://example.com\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn relative_href_is_not_flagged_javascript_url() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "rel-link.tsx",
        "export function Link() {\n  return <a href=\"/dashboard\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

/// Scope-limit claim: a dynamic (non-literal) `href` is NOT caught — only the literal `javascript:` form.
#[test]
fn dynamic_href_expression_is_not_flagged_javascript_url() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "dyn-link.tsx",
        "declare const safeUrl: string;\nexport function Link() {\n  return <a href={safeUrl}>click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn javascript_url_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented-url.ts",
        "export function f() {\n  // a.href = 'javascript:alert(1)'; -- old, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn javascript_url_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-url.ts",
        "declare const a: HTMLAnchorElement;\nexport function wire() {\n  // javascript-url-ok: intentional no-op affordance\n  a.href = 'javascript:void(0)';\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn javascript_url_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/link.tsx",
        "export function Link() {\n  return <a href=\"javascript:alert(1)\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}
