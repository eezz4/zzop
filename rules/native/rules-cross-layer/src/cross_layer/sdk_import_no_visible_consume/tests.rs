use super::*;

fn pkg(source: &str, specifier: &str, file_count: usize, example_file: &str) -> PackageImportSite {
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
    assert_eq!(data["sdkPackages"][0]["kind"], "sdk");
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

#[test]
fn superagent_from_a_single_file_with_zero_visible_consumes_fires() {
    let imports = vec![pkg("web", "superagent", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].file, "src/lib/client.ts");
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["sdkPackages"][0]["kind"], "opaqueClient");
}

#[test]
fn got_from_a_single_file_fires() {
    let imports = vec![pkg("web", "got", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["sdkPackages"][0]["kind"], "opaqueClient");
}

#[test]
fn non_client_bare_package_does_not_fire() {
    let imports = vec![pkg("web", "lodash", 1, "src/util.ts")];
    let totals = vec![("web".to_string(), 0)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

#[test]
fn requestly_does_not_match_the_request_anchor() {
    let imports = vec![pkg("web", "requestly", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

#[test]
fn oazapfts_from_a_single_file_with_zero_visible_consumes_fires() {
    // Native recognition of the oazapfts call family is gone (decision: generated SDKs are
    // injection adapters, not engine vocab); an unadapted oazapfts import is now an opaque client
    // just like superagent/got, so it must fire the disclosure rather than staying silent.
    let imports = vec![pkg("web", "oazapfts", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].file, "src/lib/client.ts");
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["sdkPackages"][0]["kind"], "opaqueClient");
}

#[test]
fn opaque_client_at_or_above_the_ratio_rule_floor_hands_off_instead_of_firing() {
    let imports = vec![pkg("web", "superagent", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), MIN_TOTAL_CONSUMES)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}
