use crate::{scan, TempDir};

// --- unsanitized-markdown-html ---

#[test]
fn marked_output_into_inner_html_with_no_sanitizer_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Post.tsx",
        "import { marked } from 'marked';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  const html = marked(article.body);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsanitized-markdown-html")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn markdown_it_render_into_dangerously_set_inner_html_with_no_sanitizer_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Post2.tsx",
        "declare const md: { render(s: string): string };\ndeclare const article: { body: string };\nexport function Comp() {\n  const html = md.render(article.body);\n  return <div dangerouslySetInnerHTML={{ __html: html }} />;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsanitized-markdown-html")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn marked_output_sanitized_with_dompurify_before_inner_html_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "SafePost.tsx",
        "import { marked } from 'marked';\nimport DOMPurify from 'dompurify';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  const raw = marked(article.body);\n  const html = DOMPurify.sanitize(raw);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

/// Bare-word claim: `marked`/`remark` are ordinary English words too, so the pattern requires call syntax
/// (`marked(`) — plain prose mentioning the word with no call form must not fire.
#[test]
fn marked_as_a_plain_english_word_in_a_string_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "prose.ts",
        "declare const el: HTMLElement;\nexport function render() {\n  const note = 'this field is marked as required';\n  el.innerHTML = note;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

/// Co-occurrence-limitation claim: a markdown render in one function and the HTML sink in a different
/// function (different spans) does not co-fire.
#[test]
fn marked_render_and_sink_in_different_functions_does_not_co_fire() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "split.ts",
        "import { marked } from 'marked';\ndeclare const article: { body: string };\nexport function toHtml() {\n  return marked(article.body);\n}\ndeclare const el: HTMLElement;\nexport function render(html: string) {\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

/// Documented `.vue` limitation: this engine has no symbol/span parser for `.vue`, so a `.vue` file's
/// `<script>`+`<template>` co-occurrence (the exact fe-vue corpus shape) does not co-fire even though
/// `.vue` is in the rule's `file_pattern`.
#[test]
fn marked_and_v_html_in_the_same_vue_sfc_does_not_co_fire_no_span_support() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Article.vue",
        "<template>\n  <div v-html=\"renderedHtml\"></div>\n</template>\n<script setup>\nimport { marked } from 'marked';\ndeclare const article: { body: string };\nconst renderedHtml = marked(article.body);\n</script>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn markdown_html_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-md.tsx",
        "import { marked } from 'marked';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  // markdown-html-ok: sanitize option enabled in marked config\n  const html = marked(article.body);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsanitized_markdown_html_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/Post.tsx",
        "import { marked } from 'marked';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  const html = marked(article.body);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}
