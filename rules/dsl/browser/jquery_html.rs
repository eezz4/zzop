use crate::{
    assert_disqualifier_clause_precedes_imperative, assert_landing_precedes_imperative,
    sanitizer_subtraction_landing, scan, TempDir,
};

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
        "import $ from 'jquery';\nexport function render(userHtml) {\n  // zzop-jquery-html-sink-ok: sanitized via DOMPurify above\n  $('#box').html(userHtml);\n}\n",
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
/// §27 pin (2026-08-29). The disqualifier is this rule's own "Known imprecision": a jQuery-OBJECT
/// argument, `.append($('<div>'))`, is lexically identical to an HTML-string argument, so the rule fires
/// on it and the message says so — "accepted as the FP-safe subset's price of admission". A reader who
/// acts on "Use `.text()` for plain text" against that finding replaces a DOM node insertion with the
/// node's stringified text and silently deletes the subtree. At HEAD the concession sat at byte 728 —
/// the LAST sentence of the message — and the imperative at byte 267.
///
/// This is the one repair of the four where the DISQUALIFIER moved instead of the imperative, and the
/// reason is that it could: the sentence names `.append(...)` and "an HTML string argument", both already
/// established by the opening sentence, so it carries its antecedents with it. It now sits directly
/// after that opening sentence and in front of the remedy. 991 chars before and after, character
/// multiset identical.
///
/// ⚠ This rule fires ZERO times on the blind corpus, so no delivered finding will ever carry this
/// ordering to a reader; the fixture below is the only place the repaired message is observed.
#[test]
fn jquery_html_sink_message_puts_the_known_imprecision_before_the_use_text_imperative() {
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
    // Sentence repair only — the finding itself is unchanged.
    assert_eq!(hits.len(), 1, "{:?}", out.findings);

    assert_disqualifier_clause_precedes_imperative(
        "jquery-html-sink",
        &hits[0].message,
        "Known imprecision: a DOM-element/jQuery-object argument",
        "Use `.text()` for plain text",
    );
}

/// §33/§37 LANDING pin — the other axis on the same verb. The pin above covers the reader whose
/// argument was never an HTML string; this one covers the reader whose argument IS one and who is
/// right to act. `.text()` sets the text of the matched set, so markup arrives as characters, and
/// `DOMPurify` — the message's own alternative — deletes by allow-list. Neither raises.
///
/// The invalidation probe is to move `sanitizer_subtraction_landing()` to the tail: every token stays
/// present and spelled once, and this goes red on ORDER while the pin above stays green.
#[test]
fn jquery_html_sink_landing_precedes_the_imperative() {
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
    assert_landing_precedes_imperative(
        "jquery-html-sink",
        &hits[0].message,
        sanitizer_subtraction_landing(),
        "Use `.text()` for plain text",
    );
}
