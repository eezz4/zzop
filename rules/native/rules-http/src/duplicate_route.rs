//! `duplicate-route` — same (METHOD, path) HTTP route provided 2+ times across the tree. The engine collects
//! `io_provides` and handles gating; this module only groups and reports.
//!
//! Groups whole-tree `IoProvide`s (`kind == "http"`) by normalized `http_interface_key` and flags every key
//! registered at 2+ distinct `(file, line)` sites, sorted for determinism: the first site is canonical, and
//! every later site gets one `Finding` naming it. This whole-tree pass is recomputed on every `analyze_tree`
//! call rather than cached, since its output is never read back out of a stale per-file cache entry.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — an isolated test
//! fixture's route never coexists with the "duplicate" at runtime.
//!
//! A later site is skipped when it resolves to the SAME handler `symbol` as the canonical site: the same
//! handler deliberately registered on two paths that normalize to one key is the trailing-slash-tolerance
//! idiom (e.g. gin's `router.POST("", h)` + `router.POST("/", h)`, so both `/x` and `/x/` hit `h`), not a
//! shadow — the same handler runs either way, so there is no "which handler wins?" ambiguity for the rule
//! to warn about. A later site with a DIFFERENT symbol (or an unknown symbol on either side, where sameness
//! can't be proven) is still flagged: that is the genuine shadowing case the rule exists for.

pub fn duplicate_route_findings(io_provides: &[zzop_core::IoProvide]) -> Vec<zzop_core::Finding> {
    let mut by_key: std::collections::BTreeMap<&str, Vec<&zzop_core::IoProvide>> =
        std::collections::BTreeMap::new();
    for p in io_provides {
        if p.kind == "http" && !zzop_core::is_test_file(&p.file) {
            by_key.entry(p.key.as_str()).or_default().push(p);
        }
    }

    let mut findings = Vec::new();
    for (key, mut sites) in by_key {
        if sites.len() < 2 {
            continue;
        }
        sites.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        let first = sites[0];
        for dup in &sites[1..] {
            // Same-handler multi-registration (trailing-slash tolerance / framework convention) is not a
            // shadow — skip it. Only proven-same non-empty symbols count; an absent symbol on either side
            // leaves the conservative warning in place. See this module's doc.
            if let (Some(a), Some(b)) = (first.symbol.as_deref(), dup.symbol.as_deref()) {
                if !a.is_empty() && a == b {
                    continue;
                }
            }
            findings.push(zzop_core::Finding {
                rule_id: "duplicate-route".to_string(),
                severity: zzop_core::Severity::Warning,
                file: dup.file.clone(),
                line: dup.line,
                message: format!(
                    "route `{key}` is registered more than once (first at {}:{}) — later registrations are shadowed or ambiguous depending on the framework. Merge the handlers or remove the duplicate. {} if this is intentional (e.g. a framework convention that legitimately registers the same route twice).",
                    first.file, first.line, zzop_core::disable_hint("duplicate-route")
                ),
                data: Some(serde_json::json!({
                    "key": key,
                    "first": {"file": first.file, "line": first.line},
                    "sites": sites.len(),
                })),
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    //! Unit tests for `duplicate_route_findings`'s grouping logic (e2e coverage: `crates/engine/tests/pack_fullstack.rs`).
    use super::*;

    fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        }
    }

    fn provide_sym(key: &str, file: &str, line: u32, symbol: &str) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            symbol: Some(symbol.to_string()),
            ..provide(key, file, line)
        }
    }

    #[test]
    fn single_registration_of_a_route_is_not_flagged() {
        let provides = vec![provide("GET /api/users", "a.ts", 3)];
        assert!(duplicate_route_findings(&provides).is_empty());
    }

    #[test]
    fn two_distinct_routes_are_not_flagged() {
        let provides = vec![
            provide("GET /api/users", "a.ts", 3),
            provide("POST /api/users", "b.ts", 5),
        ];
        assert!(duplicate_route_findings(&provides).is_empty());
    }

    #[test]
    fn same_route_registered_twice_flags_only_the_later_site() {
        let provides = vec![
            provide("GET /api/users", "b.ts", 10),
            provide("GET /api/users", "a.ts", 3),
        ];
        let found = duplicate_route_findings(&provides);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "b.ts");
        assert_eq!(found[0].line, 10);
        assert_eq!(found[0].rule_id, "duplicate-route");
        assert_eq!(found[0].severity, zzop_core::Severity::Warning);
        assert!(found[0].message.contains("a.ts:3"));
    }

    #[test]
    fn three_registrations_of_one_route_flag_the_two_later_sites() {
        let provides = vec![
            provide("GET /api/users", "c.ts", 1),
            provide("GET /api/users", "a.ts", 9),
            provide("GET /api/users", "a.ts", 2),
        ];
        // sorted by (file, line): a.ts:2 (first/canonical), a.ts:9, c.ts:1
        let found = duplicate_route_findings(&provides);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].file.as_str(), found[0].line), ("a.ts", 9));
        assert_eq!((found[1].file.as_str(), found[1].line), ("c.ts", 1));
        for f in &found {
            assert!(f.message.contains("a.ts:2"));
        }
    }

    #[test]
    fn duplicate_with_one_site_in_a_test_file_is_not_flagged() {
        let provides = vec![
            provide("GET /api/users", "routes/__tests__/api.test.ts", 12),
            provide("GET /api/users", "src/routes/users.ts", 3),
        ];
        // only one real (non-test) site remains, so < 2 sites -> no finding
        assert!(duplicate_route_findings(&provides).is_empty());
    }

    #[test]
    fn duplicate_with_both_sites_in_test_files_is_not_flagged() {
        let provides = vec![
            provide("GET /api/users", "src/routes/__tests__/a.test.ts", 1),
            provide("GET /api/users", "src/routes/__tests__/b.test.ts", 2),
        ];
        assert!(duplicate_route_findings(&provides).is_empty());
    }

    #[test]
    fn duplicate_across_two_prod_files_still_fires_alongside_a_coincidental_test_file_provide() {
        let provides = vec![
            provide("GET /api/users", "src/routes/legacy.ts", 20),
            provide("GET /api/users", "src/routes/users.ts", 3),
            provide("GET /api/users", "src/routes/__tests__/users.test.ts", 99),
        ];
        // two real (non-test) sites remain; the test-file provide must not be counted or anchor the finding.
        let found = duplicate_route_findings(&provides);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "src/routes/users.ts");
        assert_eq!(found[0].line, 3);
        assert!(found[0].message.contains("src/routes/legacy.ts:20"));
    }

    #[test]
    fn same_handler_on_a_trailing_slash_pair_is_not_flagged() {
        // gin's `router.POST("", h)` + `router.POST("/", h)` both normalize to one key with the SAME
        // handler symbol — the tolerance idiom, not a shadow. No finding.
        let provides = vec![
            provide_sym(
                "POST /api/articles",
                "articles/routers.go",
                15,
                "ArticleCreate",
            ),
            provide_sym(
                "POST /api/articles",
                "articles/routers.go",
                16,
                "ArticleCreate",
            ),
        ];
        assert!(duplicate_route_findings(&provides).is_empty());
    }

    #[test]
    fn same_key_with_different_handlers_still_flags_the_shadow() {
        // Two DIFFERENT handlers on one normalized key is the genuine "which handler wins?" ambiguity.
        let provides = vec![
            provide_sym("GET /api/users", "a.ts", 3, "listUsers"),
            provide_sym("GET /api/users", "b.ts", 9, "legacyListUsers"),
        ];
        let found = duplicate_route_findings(&provides);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].file.as_str(), found[0].line), ("b.ts", 9));
    }

    #[test]
    fn same_key_with_unknown_symbol_on_one_side_stays_conservative() {
        // Can't prove same handler when a symbol is missing -> keep the warning.
        let provides = vec![
            provide_sym("GET /api/users", "a.ts", 3, "listUsers"),
            provide("GET /api/users", "b.ts", 9),
        ];
        assert_eq!(duplicate_route_findings(&provides).len(), 1);
    }

    #[test]
    fn a_third_divergent_handler_is_flagged_while_the_same_handler_pair_is_not() {
        // Sorted by (file, line): a_routers.go:27 is canonical (A); a_routers.go:28 (A) matches it and is
        // skipped as the tolerance pair; z_legacy.go:5 (B) diverges from the canonical handler and flags.
        let provides = vec![
            provide_sym("GET /api/articles", "a_routers.go", 27, "ArticleList"),
            provide_sym("GET /api/articles", "a_routers.go", 28, "ArticleList"),
            provide_sym("GET /api/articles", "z_legacy.go", 5, "OldArticleList"),
        ];
        let found = duplicate_route_findings(&provides);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].file.as_str(), found[0].line), ("z_legacy.go", 5));
    }

    #[test]
    fn non_http_provides_are_ignored() {
        let provides = vec![
            zzop_core::IoProvide {
                body: None,
                kind: "queue".to_string(),
                key: "topic".to_string(),
                file: "a.ts".to_string(),
                line: 1,
                symbol: None,
            },
            zzop_core::IoProvide {
                body: None,
                kind: "queue".to_string(),
                key: "topic".to_string(),
                file: "b.ts".to_string(),
                line: 2,
                symbol: None,
            },
        ];
        assert!(duplicate_route_findings(&provides).is_empty());
    }
}
