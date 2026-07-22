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

// --- stream-open-no-close-in-loop ---

#[test]
fn create_read_stream_inside_for_of_loop_with_no_close_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/copy.ts",
        "declare const paths: string[];\ndeclare const dst: any;\ndeclare const fs: any;\nexport function copyAll() {\n  for (const p of paths) {\n    const s = fs.createReadStream(p);\n    s.pipe(dst);\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "stream-open-no-close-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

/// FP-adversarial (nearest harmless lookalike): same loop, but the stream feeds `pipeline(...)` instead
/// of `.pipe(...)` — `pipeline` closes both ends automatically, and the `pipeline\s*\(` text anywhere in
/// the function satisfies the `closed` absent-veto.
#[test]
fn create_read_stream_piped_via_pipeline_helper_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/copy.ts",
        "declare const paths: string[];\ndeclare const dst: any;\ndeclare const fs: any;\ndeclare function pipeline(...args: any[]): Promise<void>;\nexport async function copyAllPipeline() {\n  for (const p of paths) {\n    const s = fs.createReadStream(p);\n    await pipeline(s, dst);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "stream-open-no-close-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn create_read_stream_explicitly_closed_in_the_same_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/copy.ts",
        "declare const paths: string[];\ndeclare const dst: any;\ndeclare const fs: any;\nexport function copyAllClosed() {\n  for (const p of paths) {\n    const s = fs.createReadStream(p);\n    s.pipe(dst);\n    s.close();\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "stream-open-no-close-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn stream_in_loop_ok_marker_directly_above_the_open_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/copy.ts",
        "declare const paths: string[];\ndeclare const dst: any;\ndeclare const fs: any;\nexport function copyAllMarked() {\n  for (const p of paths) {\n    // stream-in-loop-ok: bounded fixture list, process exits right after\n    const s = fs.createReadStream(p);\n    s.pipe(dst);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "stream-open-no-close-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- listener-subscribe-in-loop ---

#[test]
fn emitter_on_inside_for_of_loop_with_no_removal_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/subscribe.ts",
        "import { EventEmitter } from \"events\";\ndeclare const channels: string[];\ndeclare const emitter: EventEmitter;\ndeclare function handler(msg: any): void;\nexport function subscribeAll() {\n  for (const ch of channels) {\n    emitter.on(\"message\", handler);\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "listener-subscribe-in-loop");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 7);
}

#[test]
fn emitter_on_inside_loop_followed_by_remove_listener_in_same_function_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/subscribe.ts",
        "import { EventEmitter } from \"events\";\ndeclare const channels: string[];\ndeclare const emitter: EventEmitter;\ndeclare function handler(msg: any): void;\nexport function subscribeAllThenRemove() {\n  for (const ch of channels) {\n    emitter.on(\"message\", handler);\n  }\n  emitter.removeListener(\"message\", handler);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "listener-subscribe-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

/// FP-adversarial (nearest harmless lookalike): the identical `.on("message", handler)` subscribe call,
/// but standing alone outside any loop — `trigger_in_loop` never satisfies, so a normal one-time
/// subscription (the common case) never fires.
#[test]
fn single_emitter_on_outside_any_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/subscribe.ts",
        "import { EventEmitter } from \"events\";\ndeclare const emitter: EventEmitter;\ndeclare function handler(msg: any): void;\nexport function subscribeOnce() {\n  emitter.on(\"message\", handler);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "listener-subscribe-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn listener_in_loop_ok_marker_directly_above_the_subscribe_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/subscribe.ts",
        "import { EventEmitter } from \"events\";\ndeclare const channels: string[];\ndeclare const emitter: EventEmitter;\ndeclare function handler(msg: any): void;\nexport function subscribeAllMarked() {\n  for (const ch of channels) {\n    // listener-in-loop-ok: bounded fixture channel list, process exits right after\n    emitter.on(\"message\", handler);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "listener-subscribe-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}

/// FP-adversarial (over-match guard): a frontend `$(el).on("click", ...)` binding inside a loop has the
/// exact same `.on("string", ...)` shape the trigger matches, but the file imports no Node event-emitter/
/// server library, so the rule's `require_file` gate is not satisfied and the file is never scanned. Pins
/// the DOM/jQuery/knex/D3 over-match class the method-name+string anchor would otherwise hit.
#[test]
fn dom_style_on_click_in_loop_in_a_file_with_no_node_event_library_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/dom.ts",
        "declare function $(sel: any): any;\ndeclare const els: any[];\ndeclare function onClick(): void;\nexport function bindAll() {\n  for (const el of els) {\n    $(el).on(\"click\", onClick);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "listener-subscribe-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}
