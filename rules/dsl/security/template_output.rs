use crate::{
    assert_landing_precedes_imperative, hits, label_of, sanitizer_subtraction_landing, scan,
    TempDir,
};

// --- template-unescaped-output ---

/// §33/§37 LANDING pin. Found by re-counting the class rather than by the batch brief, and it belongs:
/// this rule's own message calls `browser/unsafe-html-sink` its sibling — "the server-side
/// template-engine half that produces the HTML in the first place" — so leaving it out would be a
/// landing family with a member the pins do not follow, which is the exact silent case
/// `rules/dsl/message_order_verdicts.rs` was split to catch.
///
/// THE RULE ALREADY PROVED THE MECHANISM ON ITS OTHER HALF. Its `include` exclusion argues, at length,
/// that the escaped form "prints that markup as visible text" and that this is why EJS's own docs use
/// the raw form for partials. That is the landing's second half exactly — stated about `include`, and
/// never about the value the reader is being told to escape. The remedy's first arm switches
/// `<%- bio %>` to `<%= bio %>`, and where `bio` is stored rich text the page then shows its tags.
///
/// A `.hbs` fixture rather than the `.ejs` one directly above it: the constant has to be reachable
/// from any of this rule's three delivered spellings, and pinning the arm that is NOT the one the
/// message argues about keeps the claim from resting on the `include` paragraph.
#[test]
fn template_unescaped_output_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.hbs", "<div>\n{{{ rawHtml }}}\n</div>\n");
    let out = scan(&dir);
    let h = hits(&out, "template-unescaped-output");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "template-unescaped-output",
        &h[0].message,
        sanitizer_subtraction_landing(),
        "Use the escaped output form instead",
    );
}

#[test]
fn ejs_raw_output_tag_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.ejs", "<div>\n<%- widgetHtml %>\n</div>\n");
    let out = scan(&dir);
    let h = hits(&out, "template-unescaped-output");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
    assert_eq!(label_of(h[0]), Some("ejs-raw"));
}

#[test]
fn ejs_escaped_output_tag_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.ejs", "<div>\n<%= user.name %>\n</div>\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handlebars_triple_stache_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.hbs", "<div>\n{{{ rawHtml }}}\n</div>\n");
    let out = scan(&dir);
    let h = hits(&out, "template-unescaped-output");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
    assert_eq!(label_of(h[0]), Some("handlebars-triple"));
}

#[test]
fn handlebars_double_stache_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.hbs", "<div>\n{{ name }}\n</div>\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mustache_amp_unescaped_form_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write("views/item.mustache", "<div>\n{{& rawHtml}}\n</div>\n");
    let out = scan(&dir);
    let h = hits(&out, "template-unescaped-output");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("mustache-amp"));
}

#[test]
fn loose_inequality_operator_in_a_template_file_is_not_flagged() {
    // The Pug buffered-unescaped `!= expr` label was deliberately DROPPED (never-guess): it is
    // lexically indistinguishable from the loose-inequality operator `!=`, which appears in Pug and
    // Nunjucks conditionals (`{% if role != "guest" %}`). This pins that a covered template file
    // containing a bare `!=` operator produces NO finding — the whole point of dropping the label.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "views/page.njk",
        "{% if role != \"guest\" %}\n  <p>admin</p>\n{% endif %}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn unescaped_template_syntax_in_a_ts_file_is_not_flagged() {
    // Rule is deliberately extension-scoped to template files only: a token like `{{{` is a
    // legitimate JS/TS syntax fragment, so a non-template file must stay silent no matter what it
    // contains.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/util.ts",
        "export function f() {\n  return {{{ a: 1 }};\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}

/// `<%- include('partial') %>` is EJS's ONLY composing form — the escaped `<%= include(...) %>` prints the
/// partial's markup as visible text — so the raw tag there is required syntax, not a raw-HTML choice. The
/// same file's bare `<%- message %>` must survive: measured 2026-08-19 over 9 upstream trees, 16 of 17
/// findings were the include shape and the 17th was exactly this bare interpolation, a real finding.
#[test]
fn ejs_literal_include_is_not_flagged_while_a_bare_interpolation_on_the_next_line_still_is() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "views/login.ejs",
        "<%- include('head', { title: 'Login' }) -%>\n<%- message %>\n<%- include('../foot') -%>\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "template-unescaped-output");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 2);
}

/// The exclusion is keyed on a CLOSED string-literal argument, so a computed include path is untouched by
/// it. The CONCATENATED form is pinned beside the bare identifier because it is the one the first spelling
/// of this exclusion got wrong: that pattern only proved the argument STARTED with a quote, so
/// `include('partials/' + p)` matched it and went silent while the rule message claimed it still fired.
#[test]
fn ejs_include_with_a_non_literal_path_still_reports() {
    for line in [
        "<%- include(userPath) %>",
        "<%- include('partials/' + p) %>",
        "<%- include(`views/${name}`) %>",
    ] {
        let dir = TempDir::new("zzop-be-sec");
        dir.write("views/dyn.ejs", &format!("{line}\n"));
        let out = scan(&dir);
        assert_eq!(
            hits(&out, "template-unescaped-output").len(),
            1,
            "expected a finding for {line}: {:?}",
            out.findings
        );
    }
}

#[test]
fn template_unescaped_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "views/item.ejs",
        "// zzop-template-unescaped-output-ok: widgetHtml is sanitized upstream via DOMPurify\n<%- widgetHtml %>\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}
