use crate::{hits, label_of, scan, TempDir};

// --- template-unescaped-output ---

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

#[test]
fn template_unescaped_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "views/item.ejs",
        "// template-unescaped-ok: widgetHtml is sanitized upstream via DOMPurify\n<%- widgetHtml %>\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "template-unescaped-output").is_empty(),
        "{:?}",
        out.findings
    );
}
