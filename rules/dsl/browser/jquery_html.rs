use crate::{scan, TempDir};

// --- jquery-html-sink ---

#[test]
fn jquery_html_call_with_a_variable_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  $('#box').html(userHtml);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/jquery-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn jquery_append_with_a_variable_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget2.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  $('#box').append(userHtml);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/jquery-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn jquery_text_call_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget3.js",
        "import $ from 'jquery';\nexport function render(userText) {\n  $('#box').text(userText);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_call_with_a_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget4.js",
        "import $ from 'jquery';\nexport function render() {\n  $('#box').html('<b>static</b>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

/// Non-jQuery `.append(` on an unrelated object in a file that never mentions jQuery/`$(` is not flagged —
/// the `require_file` gate keeps this rule honest outside jQuery codebases.
#[test]
fn append_call_in_a_non_jquery_file_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "list.js",
        "export function addItem(list, item) {\n  list.append(item);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_sink_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented-jq.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  // $('#box').html(userHtml); -- old, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-jq.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  // jquery-html-ok: sanitized via DOMPurify above\n  $('#box').html(userHtml);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_sink_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/widget.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  $('#box').html(userHtml);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}
