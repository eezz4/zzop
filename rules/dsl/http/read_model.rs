use super::{hits, scan, TempDir};

// --- read-model-path ---

#[test]
fn get_endpoint_with_no_cache_marker_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items\", api.itemList);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "read-model-path").len(), 1, "{:?}", out.findings);
}

#[test]
fn cache_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items\", api.itemList); // cache: getCachedList (cache:list:items)\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn no_cache_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items/:id\", api.itemDetail); // no-cache: per-user state\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn post_endpoint_is_not_inspected() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.post(\"/api/items\", api.itemCreate);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn read_model_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\napiRoutes.get(\"/api/items\", api.itemList); // read-model-ok: legacy endpoint, cache handled at CDN layer\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn read_model_ok_marker_one_line_before_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\n// read-model-ok: static data served at edge, no server cache needed\napiRoutes.get(\"/api/items/:id\", api.itemDetail);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "read-model-path").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn read_model_ok_marker_two_lines_before_does_not_suppress() {
    // `MARKER_LOOKBACK_LINES` = 1 (see the const's doc in `zzop_core::dsl`): a marker 2 lines above the
    // reported line is out of range and does not suppress the finding.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\n// read-model-ok: static data, no cache needed\n// line 2\napiRoutes.get(\"/api/feed\", api.feed);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "read-model-path").len(), 1, "{:?}", out.findings);
}

#[test]
fn read_model_ok_marker_four_lines_before_does_not_suppress() {
    // Further out-of-range boundary check, same contract as the 2-lines-above case above.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "import { Hono } from \"hono\";\nconst apiRoutes = new Hono();\ndeclare const api: any;\n// read-model-ok: this is too far above\n// line 2\n// line 3\n// line 4\napiRoutes.get(\"/api/feed\", api.feed);\nexport { apiRoutes };\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "read-model-path").len(), 1, "{:?}", out.findings);
}
