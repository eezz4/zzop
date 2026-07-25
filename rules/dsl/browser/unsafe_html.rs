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
        "declare const el: HTMLElement;\ndeclare const trusted: string;\nexport function render() {\n  // unsafe-html-sink-ok: value is sanitized upstream via DOMPurify\n  el.innerHTML = trusted;\n}\n",
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

// --- sanitizer-passage veto (mono-hub measurement: 6 of 7 findings were sanitized values) ---

#[test]
fn local_escape_helper_wrapping_the_whole_value_is_not_flagged() {
    // FP class #1: a local `escapeHtml` is the whole value, so the HTML that reaches the sink is
    // already escaped. Same escape/sanitize/validate vocabulary `security/taint-flow` vetoes on.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "render.ts",
        "declare const el: HTMLElement;\ndeclare function escapeHtml(s: string): string;\ndeclare const userInput: string;\nexport function render() {\n  el.innerHTML = escapeHtml(userInput);\n}\n",
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
fn dompurify_method_qualified_sanitize_is_not_flagged() {
    // FP class #2: DOMPurify reached as a method (`DOMPurify.sanitize`) — the optional
    // method-qualifier in the veto is what lets a receiver-prefixed sanitizer count.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "svg.tsx",
        "import DOMPurify from \"dompurify\";\ndeclare const raw: string;\nexport function Svg() {\n  return <div dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(raw) }} />;\n}\n",
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
fn safe_suffixed_wrapper_is_not_flagged() {
    // FP class #3: a domain-specific wrapper named `jsonLdSafe` — covered by the `*Safe` suffix arm,
    // which is this rule's deliberate widening of taint-flow's escape/sanitize/validate vocabulary.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "seo.tsx",
        "declare function jsonLdSafe(v: unknown): string;\ndeclare const data: unknown;\nexport function Ld() {\n  return <script dangerouslySetInnerHTML={{ __html: jsonLdSafe(data) }} />;\n}\n",
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
fn raw_json_stringify_into_an_html_sink_still_fires() {
    // `JSON.stringify` is NOT a sanitizer and must never be vetoed here. JSON encoding escapes only
    // `"`, `\` and control characters — `<`, `>`, `&` and `/` pass through verbatim. So a value
    // containing `</script><img src=x onerror=alert(1)>` breaks straight out of the surrounding
    // `<script>` block; `el.innerHTML = JSON.stringify(userData)` is the same hole without the tag.
    // The `jsonLdSafe` wrapper measured in the corpus (`JSON.stringify(x)` then `.replace(/</g, ...)`
    // into `<`) exists for precisely this reason: vetoing raw `JSON.stringify` would declare that
    // wrapper pointless while silencing the very hole it was written to close.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "ld.tsx",
        "declare const jsonLd: unknown;\nexport function Ld() {\n  return <script dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }} />;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 3);
}

// -- WHOLE-VALUE is enforced, not merely asserted: anything left OUTSIDE the sanitized call fires --

#[test]
fn a_sanitizer_concatenated_with_raw_html_still_fires() {
    // The half-escaped classic: the title is escaped, the tail is not, and the tail is what runs.
    // A head-anchored veto (sanitizer merely FIRST) would silence this, which is why the veto also
    // consumes the argument list and demands the value END right after it.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "concat.ts",
        "declare const el: HTMLElement;\ndeclare function escapeHtml(s: string): string;\ndeclare const title: string;\ndeclare const rawUserHtml: string;\nexport function render() {\n  el.innerHTML = escapeHtml(title) + rawUserHtml;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 6);
}

#[test]
fn a_sanitizer_used_only_as_a_ternary_condition_still_fires() {
    // `sanitizeSvg(a) ? raw : other` never puts the sanitized string in the sink at all — the
    // sanitizer is the TEST, and both branches are raw.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "ternary.ts",
        "declare const el: HTMLElement;\ndeclare function sanitizeSvg(s: string): string;\ndeclare const a: string;\ndeclare const raw: string;\ndeclare const other: string;\nexport function render() {\n  el.innerHTML = sanitizeSvg(a) ? raw : other;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.rule_id == "browser/unsafe-html-sink")
            .count(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_sanitizer_whose_result_is_post_processed_still_fires() {
    // A method chain after the sanitizer can undo it (`.replace` re-injecting attacker text), so the
    // sanitized string is not what reaches the sink.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "chain.ts",
        "declare const el: HTMLElement;\ndeclare function sanitize(s: string): string;\ndeclare const x: string;\ndeclare const evil: string;\nexport function render() {\n  el.innerHTML = sanitize(x).replace(/a/, evil);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.rule_id == "browser/unsafe-html-sink")
            .count(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_sanitizer_concatenated_inside_a_jsx_html_prop_still_fires() {
    // Same boundary on the JSX arm of the matcher, where the value ends at `}}` rather than `;`.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Concat.tsx",
        "declare function sanitize(s: string): string;\ndeclare const a: string;\ndeclare const evil: string;\nexport function Comp() {\n  return <div dangerouslySetInnerHTML={{ __html: sanitize(a) + evil }} />;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.rule_id == "browser/unsafe-html-sink")
            .count(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_sanitizer_call_nesting_parens_two_deep_still_fires() {
    // RESIDUAL 3, pinned in the SAFE direction: one level of nested parens is recognized
    // (`escapeHtml(String(x))` below stays vetoed), two is not, so the deeper shape fires. A
    // line regex cannot balance parentheses; erring toward a finding is the only honest bail.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "deep.ts",
        "declare const el: HTMLElement;\ndeclare function escapeHtml(s: string): string;\ndeclare function a(v: string): string;\ndeclare function b(v: string): string;\ndeclare const c: string;\nexport function render() {\n  el.innerHTML = escapeHtml(a(b(c)));\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.rule_id == "browser/unsafe-html-sink")
            .count(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_sanitizer_call_nesting_parens_one_deep_is_still_vetoed() {
    // The control for the test above — the common `escapeHtml(String(obj[k] ?? ""))` idiom measured
    // in the corpus must keep passing, or residual 3 would have eaten the veto's real cases.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "one-deep.ts",
        "declare const el: HTMLElement;\ndeclare function escapeHtml(s: string): string;\ndeclare const v: unknown;\nexport function render() {\n  el.innerHTML = escapeHtml(String(v));\n}\n",
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
fn escaped_fragment_spliced_into_an_href_still_fires() {
    // THE TRUE POSITIVE that must survive: a markdown renderer escapes the link text but then builds
    // the `href` from it with no URL-scheme allowlist, so `javascript:` still reaches the attribute.
    // The veto is WHOLE-VALUE — a sanitizer merely PRESENT inside a template literal is not passage,
    // which is exactly what separates this from the vetoed fixtures above. The concat/ternary/chain
    // fixtures below pin the same boundary on the non-template spellings.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "markdown.ts",
        "declare const el: HTMLElement;\ndeclare function escapeHtml(s: string): string;\ndeclare const link: { text: string };\nexport function render() {\n  el.innerHTML = `<a href=\"${escapeHtml(link.text)}\">${escapeHtml(link.text)}</a>`;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 5);
}

#[test]
fn sanitizer_stored_in_a_variable_still_fires() {
    // RESIDUAL 2 of the message, pinned: the sink value is a bare identifier bound on an earlier line
    // to a sanitizer call. The veto reads the sink LINE only, so it cannot see the binding. Measured
    // 2026-07-25 on mono-hub as the dominant survivor class (4 of the 5 sanitized findings that still
    // fire): `previewSvg = ... ? sanitizeSvg(trimmed) : ""` then `__html: previewSvg`.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "svg.tsx",
        "declare function sanitizeSvg(s: string): string;\ndeclare const raw: string;\nexport function Svg() {\n  const previewSvg = sanitizeSvg(raw);\n  return <div dangerouslySetInnerHTML={{ __html: previewSvg }} />;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 5);
}

#[test]
fn sanitized_value_arriving_as_a_component_prop_still_fires() {
    // RESIDUAL 2, second spelling: the value is a PROP, so whatever cleaned it lives in the caller —
    // another file, often another package. Measured shape: `MonoQrPreview({ svg })` renders
    // `__html: svg` while both call sites pass `sanitizeSvg(...)`.
    // Note the asymmetry the message now states: this counts as a residual only when the caller set is
    // CLOSED. The fixture below is `export`ed, and for a component a package really exports the caller
    // set is open — `svg: string` is not a sanitized type — so firing is the CORRECT answer there, not
    // a limitation. Either way the assertion is the same; only the label on it differs.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Preview.tsx",
        "export function Preview({ svg }: { svg: string }) {\n  return <div dangerouslySetInnerHTML={{ __html: svg }} />;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.rule_id == "browser/unsafe-html-sink")
            .count(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_differently_named_safe_builder_still_fires() {
    // RESIDUAL 1, the FALSE-NEGATIVE-free half of the name vocabulary: `buildRedirectScript` sits at
    // the head of the sink value and is genuinely safe (it JSON.stringify-encodes its only input), but
    // its name carries no escape/sanitize/validate/purify/*Safe token, so the veto does not apply.
    // Naming is the only signal a line-scan matcher has; widening it would silence real sinks.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect.tsx",
        "declare function buildRedirectScript(k: string): string;\nexport function Redirect({ k }: { k: string }) {\n  return <script dangerouslySetInnerHTML={{ __html: buildRedirectScript(k) }} />;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.rule_id == "browser/unsafe-html-sink")
            .count(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn a_do_nothing_sanitizer_named_call_is_silenced_by_the_name_scoped_veto() {
    // SEAL TEST for RESIDUAL 1's false-negative half — it asserts a FALSE NEGATIVE on purpose, so the
    // limit cannot be forgotten and cannot change silently. `sanitizeFoo` below returns its argument
    // untouched, so raw attacker-controlled HTML reaches the sink, yet the NAME alone drops the finding:
    // sanitizer-NAMED is not sanitizer-PROVEN, and a line-scan matcher cannot read the callee's body.
    // Its FP-side twin `a_differently_named_safe_builder_still_fires` pins the other half of the same
    // name vocabulary; together they bound what naming can and cannot buy. If a future change makes this
    // line fire, that is an IMPROVEMENT, not a regression: re-measure, then update this test and the
    // disclosure (browser.json's message, docs/rules/catalog.md, site/rules.html) in the same change.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "noop.ts",
        "declare const el: HTMLElement;\ndeclare const userInput: string;\nfunction sanitizeFoo(s: string): string {\n  return s;\n}\nexport function render() {\n  el.innerHTML = sanitizeFoo(userInput);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "documented residual-1 blind spot changed — re-measure before editing: {:?}",
        out.findings
    );
}

#[test]
fn a_plain_unsanitized_call_value_still_fires() {
    // Scope pin: the veto is NAME-scoped, so an ordinary helper call is untouched by it.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget.ts",
        "declare const el: HTMLElement;\ndeclare function getHtml(): string;\nexport function render() {\n  el.innerHTML = getHtml();\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        out.findings
            .iter()
            .filter(|f| f.rule_id == "browser/unsafe-html-sink")
            .count(),
        1,
        "{:?}",
        out.findings
    );
}
