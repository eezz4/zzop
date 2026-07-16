use crate::{scan, TempDir};

// --- postmessage-wildcard ---

#[test]
fn postmessage_with_single_quoted_wildcard_target_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge.ts",
        "export function broadcast(data: unknown) {\n  window.postMessage(data, '*');\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/postmessage-wildcard")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

#[test]
fn postmessage_with_double_quoted_wildcard_target_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge2.ts",
        "export function relay(payload: unknown) {\n  parent.postMessage(payload, \"*\");\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/postmessage-wildcard")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

#[test]
fn postmessage_with_explicit_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge3.ts",
        "export function broadcast(data: unknown) {\n  window.postMessage(data, 'https://example.com');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}

#[test]
fn postmessage_wildcard_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge4.ts",
        "export function broadcast(data: unknown) {\n  // window.postMessage(data, '*'); -- old behavior, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}

#[test]
fn postmessage_target_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge5.ts",
        "export function broadcast(data: unknown) {\n  // postmessage-target-ok: non-sensitive heartbeat ping\n  window.postMessage(data, '*');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}

#[test]
fn postmessage_wildcard_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/bridge.ts",
        "export function broadcast(data: unknown) {\n  window.postMessage(data, '*');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}
