use crate::{assert_landing_precedes_imperative, scan, TempDir};

// --- javascript-url ---

/// §33/§37 LANDING for `javascript-url`, spliced ahead of "Never construct a link/image/script target".
///
/// WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). Two costs,
/// one per half of the remedy. The scheme test over-rejects: this rule's own remedy names an "http(s)
/// allowlist", and the obvious spelling of one refuses a same-site path, a query-only or fragment-only
/// href, and the `mailto:`, `tel:`, `blob:` and `data:` targets a page uses for contact links,
/// generated downloads and inline previews — a whole app's internal links fail a check written for one
/// scheme. And the flagged text is usually a placeholder: `javascript:void(0)` and `javascript:;` are
/// how a pre-framework page spelled "this anchor is a button", so deleting the attribute is not
/// neutral — an anchor without `href` leaves the tab order, loses its focus ring, its `:link` styling
/// and its pointer cursor, and stops being announced as a link.
///
/// WHY NOT `SANITIZER_SUBTRACTION_LANDING`, this batch's other landing (§37's most-dangerous-reuse
/// test). That constant's noun is an allow-list over MARKUP that deletes elements and attributes from
/// content; this one is an allow-list over SCHEMES that refuses whole destinations, and the anchor half
/// has no analogue there at all. Same word, different subject, different failure.
///
/// WHY NOT SHARED WITH `location-assign-dynamic`, `open-redirect` or `ssrf-user-url`, which also
/// prescribe an allow-list — recorded here so the next author does not "complete the family". Those
/// three take a value that is already a destination and ask whether it is an ALLOWED one; the
/// over-rejection this constant prices is specific to filtering by SCHEME, and their own messages
/// already say "a known set of paths, or a same-origin check" rather than http(s). Sharing would
/// require re-reading all three, which is a rule-sized decision rather than a splice.
///
/// NOT A DISQUALIFIER. A literal `javascript:` href is a real finding even when it is a placeholder —
/// it is still a scheme that executes what follows it. What changes is that removing it has a cost the
/// message did not name.
///
/// POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
/// every token stays present and spelled exactly once, and the pin goes red on ORDER alone.
const URL_SCHEME_ALLOWLIST_LANDING: &str = "AN http(s)-ONLY SCHEME TEST REJECTS MORE THAN THE ATTACK, AND MOST OF WHAT IT REJECTS IS ORDINARY: a same-site path (`/settings`), a query-only or fragment-only href, and the `mailto:`, `tel:`, `blob:` and `data:` targets a page uses for contact links, generated downloads and inline previews all fail a check spelled as `startsWith('http')`. Resolve the candidate against the document's base URL first and compare the resulting `protocol` against the schemes this app actually uses, so a relative href keeps working. The literal findings are their own case: `javascript:void(0)` and `javascript:;` are placeholder hrefs, and DELETING the attribute is not neutral — an anchor with no `href` is not a link, so it leaves the tab order, loses its focus ring, its `:link`/`:visited` styling and its pointer cursor, and stops announcing itself as a link, while swapping in `#` adds a history entry and jumps the page to the top. Where the element was only ever a click target, make it a `button` and move the handler onto it; where it is a real destination, put that destination in the `href`.";

/// §33/§37 LANDING pin. This rule's axis-A verdict is a `NO_DISQUALIFIER` row (a false-negative scope
/// disclosure); this is the other axis, on a delivered finding.
#[test]
fn javascript_url_landing_precedes_the_imperative() {
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
    assert_landing_precedes_imperative(
        "javascript-url",
        &hits[0].message,
        URL_SCHEME_ALLOWLIST_LANDING,
        "Never construct a link/image/script target from a `javascript:` scheme",
    );
}

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
        "declare const a: HTMLAnchorElement;\nexport function wire() {\n  // zzop-javascript-url-ok: intentional no-op affordance\n  a.href = 'javascript:void(0)';\n}\n",
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
