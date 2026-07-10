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
    //! Unit tests for `duplicate_route_findings`'s grouping logic (e2e coverage: `packages/engine/tests/pack_fullstack.rs`).
    use super::*;

    fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
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
    fn non_http_provides_are_ignored() {
        let provides = vec![
            zzop_core::IoProvide {
                kind: "queue".to_string(),
                key: "topic".to_string(),
                file: "a.ts".to_string(),
                line: 1,
                symbol: None,
            },
            zzop_core::IoProvide {
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
