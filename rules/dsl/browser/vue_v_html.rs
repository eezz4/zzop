use crate::{scan, TempDir};

// --- vue-v-html ---

#[test]
fn v_html_directive_in_a_vue_file_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Article.vue",
        "<template>\n  <div v-html=\"renderedHtml\"></div>\n</template>\n<script setup>\nconst renderedHtml = article.body;\n</script>\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/vue-v-html")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

#[test]
fn interpolation_binding_in_a_vue_file_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Safe.vue",
        "<template>\n  <div>{{ plainText }}</div>\n</template>\n<script setup>\nconst plainText = 'hi';\n</script>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn v_html_mentioned_only_in_a_vue_template_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Commented.vue",
        "<template>\n  // v-html=\"x\" (not real Vue syntax, just exercising the JS-style comment skip)\n</template>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn vue_v_html_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Vetted.vue",
        "<template>\n  <!-- vue-v-html-ok: sanitized upstream via DOMPurify -->\n  // vue-v-html-ok: sanitized upstream via DOMPurify\n  <div v-html=\"trusted\"></div>\n</template>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn vue_v_html_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/Article.vue",
        "<template>\n  <div v-html=\"renderedHtml\"></div>\n</template>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}
