use crate::{hits, scan, TempDir};

// --- body-limit-missing ---

#[test]
fn express_json_without_limit_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write("src/app.ts", "app.use(express.json());\n");
    let out = scan(&dir);
    let h = hits(&out, "body-limit-missing");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn express_json_with_explicit_limit_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write("src/app.ts", "app.use(express.json({ limit: '1mb' }));\n");
    let out = scan(&dir);
    assert!(
        hits(&out, "body-limit-missing").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn body_limit_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/app.ts",
        "// body-limit-ok: internal admin endpoint, payload size bounded upstream by the LB\napp.use(bodyParser.json());\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "body-limit-missing").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- console-in-be ---

#[test]
fn console_log_under_api_directory_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write("src/api/handler.ts", "console.log(\"hit\");\n");
    let out = scan(&dir);
    let h = hits(&out, "console-in-be");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn console_log_outside_backend_directories_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write("src/utils/logger.ts", "console.log(\"hit\");\n");
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn console_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/api/handler.ts",
        "// console-ok: temporary trace, removed before merge\nconsole.log(\"hit\");\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

// --- interval-no-clear ---

#[test]
fn set_interval_without_any_clear_interval_in_file_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/poller.ts",
        "export function startPolling() {\n  const id = setInterval(() => {\n    console.log(\"tick\");\n  }, 1000);\n  return id;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "interval-no-clear");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn set_interval_with_a_clear_interval_elsewhere_in_the_same_file_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/poller.ts",
        "export function startPolling() {\n  const id = setInterval(() => {\n    console.log(\"tick\");\n  }, 1000);\n  return id;\n}\nexport function stopPolling(id: any) {\n  clearInterval(id);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "interval-no-clear").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn interval_ok_marker_above_the_set_interval_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/poller.ts",
        "export function startPolling() {\n  // interval-ok: cleared by the host process's own lifecycle hook\n  const id = setInterval(() => {\n    console.log(\"tick\");\n  }, 1000);\n  return id;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "interval-no-clear").is_empty(),
        "{:?}",
        out.findings
    );
}
