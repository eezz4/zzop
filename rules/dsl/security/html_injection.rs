use crate::{
    assert_disqualifier_summary_precedes_imperative, assert_landing_precedes_imperative, hits,
    label_of, sanitizer_subtraction_landing, scan, TempDir,
};

// --- html-response-from-request ---

/// §33/§37 LANDING pin, plus the one EXIT this carrier needs. Its axis-A verdict is a template row in
/// `rules/dsl/message_order_verdicts.rs` (`ORDER_CLAIMS`); this is the other axis, on a delivered
/// finding, and the two do not displace each other.
///
/// WHY THE SHARED CONSTANT FITS. The first of this rule's three remedies is escape-or-sanitize before
/// splicing, and the third is an auto-escaping template engine — the two halves the constant prices,
/// reached from the server side rather than the DOM side. The mechanism is the same allow-list.
///
/// WHY THE EXIT IS NOT THE CONSTANT'S. The SECOND remedy is not a sanitizer at all: declaring
/// `application/json` changes what the client does with the bytes rather than what is in them, and on a
/// path a browser navigates to that means the body is displayed or downloaded instead of rendered. The
/// message's own wording ("for pure API responses") states the precondition and not the consequence, so
/// a reader who misjudges which kind of route they are on gets a broken page from a remedy that
/// contains no sanitizer to blame.
#[test]
fn html_response_from_request_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/greet.ts",
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  res.send('<div>' + name + '</div>');\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "html-response-from-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "html-response-from-request",
        &h[0].message,
        sanitizer_subtraction_landing(),
        "Escape/sanitize request-derived values",
    );
    for needle in [
        "THE CONTENT-TYPE ROUTE IS NOT INTERCHANGEABLE",
        "displayed or downloaded instead of rendered",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/html-response-from-request: the exit lost {needle:?} — the shared constant \
             does not say it, so nothing else would catch its removal: {}",
            h[0].message
        );
    }
}

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
        "declare const res: any;\ndeclare const req: any;\nexport function greet() {\n  const name = req.query.name;\n  // zzop-html-response-from-request-ok: name is allow-listed to alpha chars upstream\n  res.send('<div>' + name + '</div>');\n}\n",
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

/// §27 pin (2026-08-29). The disqualifier names three concrete constructs a regex reads exactly as it
/// reads a true sink -- logging strings, test fixtures, non-response string building -- and until this
/// commit it sat behind "Use a template engine with auto-escaping". A reader who starts swapping a log
/// line's concatenation for a template engine is precisely the reader that sentence exists for.
///
/// WHAT MOVED: two adjacent sentences swapped. 1052 characters before and after, character multiset
/// identical. Both are self-contained -- "Kept `warning` ... because this is shape-only" takes "this"
/// from the opening description, and the remedy carries no antecedent at all -- so nothing had to be
/// repaired to make room, which is why the CLAUSE moved here and the verb moved in the two rules whose
/// downstream sentences point back at the clause.
///
/// INVALIDATION PROBE: swap the two sentences back. Both stay spelled exactly once, a `contains` pin
/// stays green, and this assertion alone goes red.
#[test]
fn dangerous_html_concat_fp_prone_clause_precedes_the_template_engine_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "dangerous-html-concat");
    // Sentence order only -- the finding itself is unchanged.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_eq!(label_of(h[0]), Some("open-tag-concat"));
    assert_disqualifier_summary_precedes_imperative(
        "dangerous-html-concat",
        &h[0].message,
        "Kept `warning` and deployed-surface-excluded",
        "Use a template engine with auto-escaping",
        "all read the same way to a regex",
    );
}

/// §33/§37 LANDING pin — the other axis on the same verb the pin above guards.
///
/// The cost is sharpest on exactly this rule's shape. What it flags is a tag literal concatenated with
/// a non-literal, and in hand-assembled markup that non-literal is very often ANOTHER FRAGMENT of
/// markup — rows built in a loop, a partial rendered above. An auto-escaping template engine escapes
/// every interpolation by default, so the fragment that used to be a list arrives as the characters of
/// its own tags, and the reader who followed the instruction sees a page, not an error.
///
/// The three-way order — disqualifier summary, then landing, then verb — is asserted across this
/// `fn` and the one below it, one axis each, because `delivered_pins` refuses both in one function.
/// The invalidation probe is to move the constant to the tail — every token stays spelled once and
/// only ORDER changes.
#[test]
fn dangerous_html_concat_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "dangerous-html-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "dangerous-html-concat",
        &h[0].message,
        sanitizer_subtraction_landing(),
        "Use a template engine with auto-escaping",
    );
}

/// The first leg of that three-way order, in its own `#[test]` because the second leg is asserted
/// through a shared helper and `delivered_pins` refuses a `fn` that holds both — an inline offset
/// comparison sitting beside a helper call is not counted, so the two together read as one axis.
#[test]
fn dangerous_html_concat_disqualifier_summary_precedes_the_landing() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/render.ts",
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "dangerous-html-concat");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let at_summary = h[0]
        .message
        .find("Kept `warning` and deployed-surface-excluded")
        .expect("disqualifier summary present");
    let at_landing = h[0]
        .message
        .find(sanitizer_subtraction_landing())
        .expect("landing present");
    assert!(
        at_summary < at_landing,
        "security/dangerous-html-concat: the landing (byte {at_landing}) moved AHEAD of the summary \
         that disqualifies this finding (byte {at_summary}) — the order is disqualifier, landing, \
         verb. In: {}",
        h[0].message
    );
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
        "declare const res: any;\ndeclare const name: string;\nexport function render() {\n  // zzop-dangerous-html-concat-ok: name is escaped via a wrapper the regex can't see\n  res.send('<div>' + name);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "dangerous-html-concat").is_empty(),
        "{:?}",
        out.findings
    );
}
