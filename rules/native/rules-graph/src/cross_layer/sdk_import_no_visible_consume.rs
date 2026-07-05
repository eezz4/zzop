//! `cross-layer/sdk-import-no-visible-consume` (info) — a tree that imports an SDK-shaped package
//! (`@scope/sdk`, `*-sdk`, `openapi*`, `*api-client*`) from several files while having almost no
//! statically visible `http` consumes. A tree consuming its API exclusively through a generated SDK client
//! produces zero fetch-shaped consumes for the join to see, so not even
//! `cross-layer/unresolved-consume-ratio` (which needs >= 5 visible consumes) can report the blind spot.
//! This rule is that report: consumption exists (the import fan-out proves it) but flows through a client
//! the egress extractor cannot see, so join-based findings are structurally weak for this tree.
//!
//! Fires only below `unresolved_consume_ratio`'s `MIN_TOTAL_CONSUMES` floor — the two rules partition the
//! blind-spot space and never co-fire on the same tree.

use std::collections::BTreeMap;

use zzop_core::{Finding, Severity};

use super::{PackageImportSite, MIN_TOTAL_CONSUMES};

/// An SDK package must be imported from at least this many distinct files before the tree-level
/// "consumption flows through an SDK" claim is credible — a single dangling import proves nothing.
const MIN_SDK_IMPORTING_FILES: usize = 3;

/// SDK-shaped package specifier: a whole `sdk`/`openapi` name segment, an `api-client` compound, or a
/// GraphQL client library (`@apollo/client`, `urql`, `graphql-request`) — the same join-blindness as a
/// generated REST SDK. Excludes the bare `graphql` package, since a GraphQL server imports that too and
/// would be misframed as SDK-driven. Segment-anchored so e.g. `sdkim` never matches.
const SDK_SPECIFIER_PATTERN: &str =
    r"(?i)(^|[/@-])(sdk|openapi|api-client|apollo|urql|graphql-request)([/-]|$)";

pub fn sdk_import_no_visible_consume_findings(
    package_imports: &[PackageImportSite],
    http_consume_totals: &[(String, usize)],
) -> Vec<Finding> {
    let sdk_re = regex::Regex::new(SDK_SPECIFIER_PATTERN).unwrap();
    let totals: BTreeMap<&str, usize> = http_consume_totals
        .iter()
        .map(|(s, n)| (s.as_str(), *n))
        .collect();

    // source -> SDK-shaped packages imported widely enough to count.
    let mut sdk_by_source: BTreeMap<&str, Vec<&PackageImportSite>> = BTreeMap::new();
    for p in package_imports {
        if p.file_count >= MIN_SDK_IMPORTING_FILES && sdk_re.is_match(&p.specifier) {
            sdk_by_source.entry(p.source.as_str()).or_default().push(p);
        }
    }

    let mut out = Vec::new();
    for (source, mut packages) in sdk_by_source {
        let visible = totals.get(source).copied().unwrap_or(0);
        // At or above the ratio rule's floor, `unresolved-consume-ratio` owns the blind-spot report.
        if visible >= MIN_TOTAL_CONSUMES {
            continue;
        }
        packages.sort_by(|a, b| a.specifier.cmp(&b.specifier));
        let names: Vec<&str> = packages.iter().map(|p| p.specifier.as_str()).collect();
        let first = packages[0];
        let message = format!(
            "source `{source}` imports the SDK-shaped package{} {} from {} file{} but has only {visible} \
             statically visible http consume{} — API consumption likely flows through a generated/vendor \
             SDK client the egress extractor cannot see, so the cross-layer join is blind for this source \
             and join-based findings (`cross-layer/unconsumed-endpoint`, `cross-layer/unprovided-mutation-call`, \
             ...) are structurally weak here. Prefer literal paths at call sites where practical, or feed \
             this source through a Normalized AST adapter that projects the SDK calls as `IoConsume` facts. \
             Disable via rule config `disabled_rules: [\"cross-layer/sdk-import-no-visible-consume\"]` if \
             the source is intentionally SDK-driven and the join blindness is accepted.",
            if names.len() == 1 { "" } else { "s" },
            names
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", "),
            packages.iter().map(|p| p.file_count).sum::<usize>(),
            if packages.len() == 1 && packages[0].file_count == 1 {
                ""
            } else {
                "s"
            },
            if visible == 1 { "" } else { "s" },
        );
        out.push(Finding {
            rule_id: "cross-layer/sdk-import-no-visible-consume".to_string(),
            severity: Severity::Info,
            file: first.example_file.clone(),
            line: 1,
            message,
            data: Some(serde_json::json!({
                "source": source,
                "sdkPackages": packages
                    .iter()
                    .map(|p| serde_json::json!({
                        "specifier": p.specifier,
                        "fileCount": p.file_count,
                        "exampleFile": p.example_file,
                    }))
                    .collect::<Vec<_>>(),
                "visibleHttpConsumes": visible,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(
        source: &str,
        specifier: &str,
        file_count: usize,
        example_file: &str,
    ) -> PackageImportSite {
        PackageImportSite {
            source: source.to_string(),
            specifier: specifier.to_string(),
            file_count,
            example_file: example_file.to_string(),
        }
    }

    #[test]
    fn sdk_import_with_no_visible_consumes_fires_once_per_tree() {
        let imports = vec![
            pkg("web", "@acme/sdk", 40, "src/lib/api.ts"),
            pkg("web", "svelte", 200, "src/App.svelte"),
        ];
        let totals = vec![("web".to_string(), 0)];
        let out = sdk_import_no_visible_consume_findings(&imports, &totals);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/sdk-import-no-visible-consume");
        assert_eq!(out[0].file, "src/lib/api.ts");
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["visibleHttpConsumes"], 0);
        assert_eq!(data["sdkPackages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn visible_consumes_at_the_ratio_rule_floor_hand_off_instead_of_firing() {
        let imports = vec![pkg("web", "@acme/sdk", 40, "src/lib/api.ts")];
        let totals = vec![("web".to_string(), MIN_TOTAL_CONSUMES)];
        assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
    }

    #[test]
    fn sdk_imported_from_too_few_files_does_not_fire() {
        let imports = vec![pkg("web", "@acme/sdk", 2, "src/lib/api.ts")];
        let totals = vec![("web".to_string(), 0)];
        assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
    }

    #[test]
    fn non_sdk_specifiers_do_not_fire_and_segment_anchoring_holds() {
        let imports = vec![
            pkg("web", "react", 100, "src/App.tsx"),
            pkg("web", "sdkim-utils", 10, "src/x.ts"), // "sdk" not a whole segment
        ];
        let totals = vec![("web".to_string(), 0)];
        assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
    }

    #[test]
    fn openapi_and_api_client_shapes_match() {
        let imports = vec![
            pkg("a", "openapi-fetch", 5, "src/a.ts"),
            pkg("b", "@acme/api-client", 6, "src/b.ts"),
        ];
        let totals = vec![("a".to_string(), 0), ("b".to_string(), 1)];
        let out = sdk_import_no_visible_consume_findings(&imports, &totals);
        assert_eq!(out.len(), 2);
        // Deterministic: sorted by anchor file.
        assert_eq!(out[0].file, "src/a.ts");
        assert_eq!(out[1].file, "src/b.ts");
    }

    #[test]
    fn graphql_client_libraries_match_but_the_bare_graphql_server_package_does_not() {
        let imports = vec![
            pkg("fe", "@apollo/client", 50, "src/apollo.ts"),
            pkg("fe2", "@urql/core", 12, "src/urql.ts"),
            pkg("be", "graphql", 30, "src/schema.ts"), // server-side schema package: no claim
        ];
        let totals = vec![];
        let out = sdk_import_no_visible_consume_findings(&imports, &totals);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file, "src/apollo.ts");
        assert_eq!(out[1].file, "src/urql.ts");
    }

    #[test]
    fn tree_missing_from_totals_counts_as_zero_visible() {
        let imports = vec![pkg("web", "foo-sdk", 3, "src/api.ts")];
        let out = sdk_import_no_visible_consume_findings(&imports, &[]);
        assert_eq!(out.len(), 1);
    }
}
