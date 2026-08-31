//! The rules of this file that stayed: `body-limit-missing`, `interval-no-clear`,
//! `stream-open-no-close-in-loop`, `listener-subscribe-in-loop`, `fs-in-loop-serial`. `console-in-be`
//! was exported to `examples/packs/code-hygiene.json` on 2026-08-12 (`axis: opinion`); its tests live at
//! `examples/packs/tests/console_in_be.rs`. It shared no fixture with any rule here, so nothing had to
//! be duplicated.

use crate::assert_disqualifier_clause_precedes_imperative;
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

/// §27 pin (2026-08-29). The disqualifier here is the RESIDUAL the rule declares about itself: the
/// `limit:` veto reads exactly one line in each direction, so a limit written two or more lines below
/// the call — after another option, say — still reports. A reader who acts on "Set an explicit `limit`
/// ..." having already written one two lines down edits nothing and learns nothing; the sentence that
/// tells him the answer here is the suppression marker sat three sentences further on.
///
/// The whole disclosure block moved ahead of the imperative rather than only its last sentence, because
/// "exactly one line in each direction" is a SUMMARY of the WHERE-the-limit-may-sit sentence and "that
/// second spelling" points back into it — lifting the residual alone would have stranded both
/// antecedents. Equivalently: the imperative moved to the end, where it is self-contained (no pronoun,
/// its own example), so the move needed no bridging sentence. 1353 chars before and after, multiset
/// identical.
#[test]
fn body_limit_missing_message_puts_the_two_line_residual_before_the_set_a_limit_imperative() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write("src/app.ts", "app.use(express.json());\n");
    let out = scan(&dir);
    let h = hits(&out, "body-limit-missing");
    // Sentence repair only — the finding itself is unchanged.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);

    assert_disqualifier_clause_precedes_imperative(
        "body-limit-missing",
        &h[0].message,
        "A limit written two or more lines below the call",
        "Set an explicit `limit`",
    );
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

/// The spelling a formatter actually produces, and the one this rule could not see until 2026-08-21:
/// the trigger matches the line that OPENS the options object and the limit sits one line down. Measured
/// on directus `06027c83` (`api/src/app.ts:247-248`) — the single place across directus and 38 corpus
/// trees where the advice had genuinely been taken was the one place the rule fired.
#[test]
fn express_json_with_the_limit_on_the_next_line_is_not_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/app.ts",
        "app.use(express.json({\n  limit: process.env.MAX_PAYLOAD_SIZE as string,\n}));\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "body-limit-missing").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The contract is EXACTLY one line in each direction, so a limit two lines down is still reported. Pinned
/// because it is a residual the rule message states, not an oversight — a span knob is what this field
/// deliberately is not.
#[test]
fn a_limit_two_lines_below_the_trigger_is_still_flagged() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/app.ts",
        "app.use(express.json({\n  strict: true,\n  limit: '1mb',\n}));\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "body-limit-missing").len(),
        1,
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

/// **An `.unref()`'d interval cannot cause the harm this rule names.** The finding is about a timer that
/// keeps running with nobody able to stop it; `.unref()` removes the handle from Node's event-loop
/// refcount, so the process exits on schedule whether or not anyone ever calls `clearInterval`. Measured
/// on nocodb `3a5cbd5`, where `packages/nocodb/src/helpers/tele.ts:33` ends in `.unref()` on the flagged
/// line itself and was reported anyway.
///
/// The veto is per-LINE, not per-file, and that is the load-bearing part: a file holding one unref'd
/// timer and one genuinely leaked timer must still report the leaked one. Both ride here in one call.
///
/// Stated miss: `.unref()` chained on the NEXT line is not seen. A line-scan sees one line, and widening
/// to a window would need evidence this shape occurs — it did not in the tree that produced the finding.
#[test]
fn an_unrefd_interval_is_vetoed_on_its_own_line_and_a_leaked_sibling_still_fires() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/telemetry.ts",
        "export function start() {\n\
        \x20 setInterval(() => send(), 60_000).unref();\n\
        \x20 setInterval(() => leak(), 1000);\n\
        }\n\
        declare function send(): void;\n\
        declare function leak(): void;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "interval-no-clear");
    let lines: Vec<u32> = h.iter().map(|f| f.line).collect();
    assert_eq!(
        lines,
        vec![3],
        "only the un-unref'd timer on line 3 may fire: {:?}",
        out.findings
    );
}
