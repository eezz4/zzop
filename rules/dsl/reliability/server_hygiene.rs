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
        "// zzop-body-limit-missing-ok: internal admin endpoint, payload size bounded upstream by the LB\napp.use(bodyParser.json());\n",
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
        "// zzop-console-in-be-ok: temporary trace, removed before merge\nconsole.log(\"hit\");\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

// --- console-in-be after the call-scan migration ---
//
// The three tests above are the PARITY set: they passed against the line-scan version and must keep
// passing, which is what makes "this migration changed nothing on TypeScript" a measured claim rather
// than an intention. The tests below pin what the migration DID change, in both directions.

#[test]
fn a_python_print_on_a_backend_path_now_fires() {
    // The gain, and the only intended one on this rule. One rule, two languages — `print` is a
    // `console-write` with the callee spelled as Python spells it, reached by adding `py` to the file
    // pattern rather than by writing a second regex.
    let dir = TempDir::new("zzop-be-rel");
    dir.write("src/api/handler.py", "def handle():\n    print(\"hit\")\n");
    let out = scan(&dir);
    let h = hits(&out, "console-in-be");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn a_python_hash_ok_marker_suppresses_the_finding() {
    // `#` is a real leader for this matcher (io-scan's set, not line-scan's `//`-only), so a Python
    // author has the same escape hatch a TypeScript one does. Without this the rule would be
    // unsuppressable in the language it just reached.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/api/handler.py",
        "def handle():\n    # zzop-console-in-be-ok: startup banner, vetted\n    print(\"hit\")\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn debug_and_trace_are_projected_but_stay_out_of_this_rule() {
    // The deliberate non-widening, pinned as a decision rather than left as an accident. The producer
    // emits all six `console` write methods (`CONSOLE_WRITE_METHODS`); this rule's callee pattern keeps
    // the four the text regex already flagged, so the migration's measured detection delta is
    // attributable to the Python reach alone. `console-in-loop` is where the other two are read.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/api/handler.ts",
        "console.debug(\"d\");\nconsole.trace(\"t\");\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn a_console_call_named_only_in_a_string_or_a_comment_is_not_a_site() {
    // The parse dividend. The line-scan version needed `skip_comment_lines` for half of this and had no
    // answer at all for the other half; here neither is a site, so neither reaches the rule.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/api/handler.ts",
        "// console.log(\"commented out\")\nexport const help = \"call console.log(x) to debug\";\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn a_structured_logger_is_never_a_console_write() {
    // The false fold the channel refuses at the producer, asserted from the rule side: a rule banning
    // console writes in a backend is not banning logging, in either language.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/api/handler.ts",
        "declare const logger: { info(m: string): void };\nexport function handle() {\n  logger.info(\"hit\");\n}\n",
    );
    dir.write(
        "src/api/svc.py",
        "import logging\n\n\ndef handle():\n    logging.info(\"hit\")\n",
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
        "export function startPolling() {\n  // zzop-interval-no-clear-ok: cleared by the host process's own lifecycle hook\n  const id = setInterval(() => {\n    console.log(\"tick\");\n  }, 1000);\n  return id;\n}\n",
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
        "declare const paths: string[];\ndeclare const dst: any;\ndeclare const fs: any;\nexport function copyAllMarked() {\n  for (const p of paths) {\n    // zzop-stream-open-no-close-in-loop-ok: bounded fixture list, process exits right after\n    const s = fs.createReadStream(p);\n    s.pipe(dst);\n  }\n}\n",
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
        "import { EventEmitter } from \"events\";\ndeclare const channels: string[];\ndeclare const emitter: EventEmitter;\ndeclare function handler(msg: any): void;\nexport function subscribeAllMarked() {\n  for (const ch of channels) {\n    // zzop-listener-subscribe-in-loop-ok: bounded fixture channel list, process exits right after\n    emitter.on(\"message\", handler);\n  }\n}\n",
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

// --- fs-in-loop-serial ---

#[test]
fn fs_promises_readfile_await_inside_for_of_loop_with_no_promise_all_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/ingest.ts",
        "import fs from \"fs\";\ndeclare const paths: string[];\nexport async function readAll() {\n  const out: string[] = [];\n  for (const p of paths) {\n    const content = await fs.promises.readFile(p, \"utf8\");\n    out.push(content);\n  }\n  return out;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fs-in-loop-serial");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn fs_writefile_await_inside_foreach_callback_is_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/dump.ts",
        "import fs from \"fs\";\ndeclare const records: { path: string; data: string }[];\nexport async function dumpAll() {\n  records.forEach(async (r) => {\n    await fs.writeFile(r.path, r.data);\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fs-in-loop-serial");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn fs_readfile_await_wrapped_in_promise_all_map_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/ingest.ts",
        "import fs from \"fs\";\ndeclare const paths: string[];\nexport async function readAllParallel() {\n  return Promise.all(\n    paths.map(async (p) => {\n      return await fs.promises.readFile(p, \"utf8\");\n    })\n  );\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-in-loop-serial").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn single_fs_readfile_await_outside_any_loop_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/load.ts",
        "import fs from \"fs\";\nexport async function readOne(p: string) {\n  return await fs.promises.readFile(p, \"utf8\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-in-loop-serial").is_empty(),
        "{:?}",
        out.findings
    );
}

/// FP-adversarial (nearest harmless lookalike): `open`/`createReadStream` are deliberately excluded from
/// this rule's call set — `stream-open-no-close-in-loop` already owns the resource-leak angle for those,
/// and doubling up here would just co-fire noise on the same line for a different assertion.
#[test]
fn create_read_stream_inside_loop_is_not_fs_in_loop_serial() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/copy.ts",
        "declare const paths: string[];\ndeclare const dst: any;\ndeclare const fs: any;\nexport function copyAll() {\n  for (const p of paths) {\n    const s = fs.createReadStream(p);\n    s.pipe(dst);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-in-loop-serial").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fs_loop_serial_ok_marker_directly_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/ingest.ts",
        "import fs from \"fs\";\ndeclare const paths: string[];\nexport async function readAllMarked() {\n  const out: string[] = [];\n  for (const p of paths) {\n    // zzop-fs-in-loop-serial-ok: deliberately sequential, must preserve on-disk read order\n    const content = await fs.promises.readFile(p, \"utf8\");\n    out.push(content);\n  }\n  return out;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-in-loop-serial").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn fs_in_loop_serial_require_file_gate_skips_a_file_with_no_fs_signal() {
    // The `require_file` gate (an `fs` word / fs import) must keep the scanner out of files with a
    // non-fs receiver that merely matches the awaited-call shape lexically — pins the gate's
    // contribution, which no other fs-in-loop-serial test exercises.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/ingest.ts",
        "declare const paths: string[];\ndeclare const helper: { readFile(p: string, e: string): Promise<string> };\nexport async function readAll() {\n  const out: string[] = [];\n  for (const p of paths) {\n    out.push(await helper.readFile(p, \"utf8\"));\n  }\n  return out;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "fs-in-loop-serial").is_empty(),
        "{:?}",
        out.findings
    );
}
