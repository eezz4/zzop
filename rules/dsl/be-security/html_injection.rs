use crate::{hits, label_of, scan, TempDir};

// --- html-response-from-request ---

#[test]
fn res_send_with_req_query_and_html_tag_literal_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/greet.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  res.send('<div>' + name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "html-response-from-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn res_write_with_req_body_and_content_type_html_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/report.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function report() {\n  res.setHeader('Content-Type', 'text/html');\n  const title = req.body.title;\n  res.write(title);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "html-response-from-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn res_send_with_json_body_and_no_html_marker_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/data.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function data() {\n  const id = req.params.id;\n  res.send({ id });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn res_send_with_html_tag_but_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/static.ts",
        "declare const res: any;\nexport function landing() {\n  res.send('<div>welcome</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn res_send_with_sanitizer_present_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/greet.ts",
        "declare const res: any;\ndeclare const req: any;\ndeclare function escapeHtml(s: string): string;\nexport function greet() {\n  const name = req.query.name;\n  res.send('<div>' + escapeHtml(name) + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_response_from_request_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "tests/greet.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  res.send('<div>' + name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_response_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/greet.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  // html-response-ok: name is allow-listed to alpha chars upstream\n  res.send('<div>' + name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "html-response-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- dangerous-html-concat ---

#[test]
fn opening_tag_literal_concatenated_with_a_variable_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "dangerous-html-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_eq!(label_of(h[0]), Some("open-tag-concat"));
}

#[test]
fn variable_concatenated_with_a_closing_tag_literal_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send(name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "dangerous-html-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("close-tag-concat"));
}

#[test]
fn pure_literal_html_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\nexport function render() {\n  res.send('<div>' + '</div>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn non_html_concatenation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/query.ts",
        "declare const res: any;\ndeclare const col: string;\nexport function build() {\n  res.send('SELECT ' + col);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn variable_concatenated_with_a_non_tag_string_ending_in_gt_is_not_flagged() {
    // close-tag-concat tightening: the trailing literal must contain actual tag markup (a `<`
    // before its `>`), so a benign arrow/annotation string that merely ENDS in `>` (`'arrow ->'`)
    // is not mistaken for a closing HTML tag.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/log.ts",
        "declare const res: any;\ndeclare const msg: string;\nexport function render() {\n  res.send(msg + 'arrow -> text');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_tag_concatenation_with_no_response_context_gate_present_is_not_flagged() {
    // require_file gate claim: without any res./response./content-type mention anywhere in the
    // file, the concatenation shape alone stays silent (e.g. a CLI/log-formatting helper).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "cli/format.ts",
        "declare const name: string;\nexport function format() {\n  return '<div>' + name;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_concat_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "tests/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn html_concat_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  // html-concat-ok: name is escaped via a wrapper the regex can't see\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}
