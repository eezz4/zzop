use crate::{hits, scan, TempDir};

// --- skip_comment_lines + test-path file_exclude_pattern ---
// Every deployed-surface rule in this pack enables `skip_comment_lines` (a commented-out example of a flagged shape must not fire) and shares the test-path `file_exclude_pattern` (same string as `debug-true-committed`, already exercised above).

#[test]
fn async_route_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/routes.ts",
        "export function registerRoutes(app: any) {\n  // app.get(\"/items\", async (req, res) => { ... }) -- old handler, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "async-route-no-catch").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn json_parse_of_request_body_in_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/__tests__/handler.test.ts",
        "export function handleBody(req: any) {\n  const parsed = JSON.parse(req.body);\n  return parsed;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "json-parse-no-try").is_empty(),
        "{:?}",
        out.findings
    );
}
