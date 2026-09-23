use crate::{assert_landing_precedes_imperative, sanitizer_subtraction_landing, scan, TempDir};

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
        "<template>\n  <!-- zzop-vue-v-html-ok: sanitized upstream via DOMPurify -->\n  // zzop-vue-v-html-ok: sanitized upstream via DOMPurify\n  <div v-html=\"trusted\"></div>\n</template>\n",
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

/// §33/§37 LANDING pin. This rule's axis-A verdict is a template row in
/// `rules/dsl/message_order_verdicts.rs` (`ORDER_CLAIMS`, "if the bound value is influenced by user
/// input" before "Prefer `{{ }}` text interpolation"); this pin is the other axis, on a DELIVERED
/// finding, and the two do not displace each other.
///
/// The cost is the sharper of the two arms here. `v-html` is the directive a Vue app reaches for
/// precisely when the value IS markup — a CMS body, an editor's output — so swapping it for `{{ }}`
/// puts the tags on the page as characters, and reaching for DOMPurify instead deletes by allow-list.
/// The order asserted is landing before verb; the invalidation probe is to move the constant to the
/// tail, where every token is still spelled once and only ORDER has changed.
#[test]
fn vue_v_html_landing_precedes_the_imperative() {
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
    assert_landing_precedes_imperative(
        "vue-v-html",
        &hits[0].message,
        sanitizer_subtraction_landing(),
        "Prefer `{{ }}` text interpolation",
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
