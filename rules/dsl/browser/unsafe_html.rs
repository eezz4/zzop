use crate::{scan, TempDir};

// --- unsafe-html-sink ---

#[test]
fn innerhtml_assign_with_a_variable_is_flagged_innerhtml_assign() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "render.ts",
        "declare const el: HTMLElement;\ndeclare const userInput: string;\nexport function render() {\n  el.innerHTML = userInput;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 4);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("innerhtml-assign")
    );
}

#[test]
fn outerhtml_plus_equals_with_a_call_is_flagged_innerhtml_assign() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "append.ts",
        "declare const el: HTMLElement;\ndeclare function getHtml(): string;\nexport function append() {\n  el.outerHTML += getHtml();\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn innerhtml_plain_string_literal_assignment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "safe.ts",
        "declare const el: HTMLElement;\nexport function render() {\n  el.innerHTML = \"<b>safe</b>\";\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_strict_equality_comparison_is_not_flagged() {
    // FP guard: `el.innerHTML === originalHtml` is a read + comparison, not an assignment — the `=` added
    // to the negative char class rejects the second `=` of `===`/`==` right after the assignment position.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "cmp.ts",
        "declare const el: HTMLElement;\ndeclare const originalHtml: string;\nexport function unchanged() {\n  return el.innerHTML === originalHtml;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_loose_equality_comparison_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "cmp2.ts",
        "declare const target: HTMLElement;\ndeclare const prev: string;\nexport function same() {\n  return target.innerHTML == prev;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_inequality_comparison_is_not_flagged() {
    // FP guard: `el.innerHTML != x` — the `!` sits where the pattern demands `[+]?=`, so the assignment
    // position never matches and the negative class never even gets consulted.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "cmp3.ts",
        "declare const el: HTMLElement;\ndeclare const x: string;\nexport function changed() {\n  return el.innerHTML != x;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_plain_template_literal_with_no_interpolation_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "safe2.ts",
        "declare const el: HTMLElement;\nexport function render() {\n  el.innerHTML = `<b>safe</b>`;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_template_literal_with_interpolation_is_flagged_innerhtml_template() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "greet.ts",
        "declare const el: HTMLElement;\ndeclare const name: string;\nexport function render() {\n  el.innerHTML = `<b>${name}</b>`;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("innerhtml-template")
    );
}

#[test]
fn insert_adjacent_html_with_a_variable_argument_is_flagged_insert_adjacent() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "insert.ts",
        "declare const el: HTMLElement;\ndeclare const userHtml: string;\nexport function insert() {\n  el.insertAdjacentHTML('beforeend', userHtml);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("insert-adjacent")
    );
}

#[test]
fn insert_adjacent_html_with_a_literal_html_argument_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "insert2.ts",
        "declare const el: HTMLElement;\nexport function insert() {\n  el.insertAdjacentHTML('beforeend', '<b>safe</b>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn dangerously_set_inner_html_with_a_variable_is_flagged_dangerously_set() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Comp.tsx",
        "declare const data: { html: string };\nexport function Comp() {\n  return <div dangerouslySetInnerHTML={{ __html: data.html }} />;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("dangerously-set")
    );
}

#[test]
fn dangerously_set_inner_html_with_a_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Comp2.tsx",
        "export function Comp() {\n  return <div dangerouslySetInnerHTML={{ __html: \"<b>safe</b>\" }} />;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsafe_html_sink_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented.ts",
        "declare const el: HTMLElement;\ndeclare const userInput: string;\nexport function render() {\n  // el.innerHTML = userInput; -- old implementation, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsafe_html_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted.ts",
        "declare const el: HTMLElement;\ndeclare const trusted: string;\nexport function render() {\n  // unsafe-html-ok: value is sanitized upstream via DOMPurify\n  el.innerHTML = trusted;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsafe_html_sink_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/render.ts",
        "declare const el: HTMLElement;\ndeclare const userInput: string;\nexport function render() {\n  el.innerHTML = userInput;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}
