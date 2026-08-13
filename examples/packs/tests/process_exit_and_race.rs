//! `process-exit-in-lib` and `promise-race-no-cancel` — moved here with the rules from
//! `rules/dsl/reliability/fetch_and_process.rs`, which keeps the `fetch-no-timeout` and
//! `emitter-async-listener` halves it was sharing a file with. All four rules use distinct fixtures, so
//! this was a clean cut rather than a duplication.
//!
//! One cross-reference did not move with them: `sync-fs-in-handler` excludes `scripts/`/`tools/`/`bin/`
//! and its comment cites `process-exit-in-lib` as the rule it mirrors that exclusion from. That is a
//! shape citation, not a coverage handoff — `sync-fs-in-handler` excludes those paths because sync fs is
//! fine off the request path, not because another rule covers them — so nothing went silent when this
//! rule left the bundle. See `rules/dsl/reliability/routes_and_handlers.rs`.

use crate::{hits, scan, TempDir};

// --- process-exit-in-lib ---

#[test]
fn process_exit_inside_a_function_is_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/shutdown.ts",
        "export function shutdown(reason: string) {\n  console.error(reason);\n  process.exit(1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "process-exit-in-lib");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn process_exit_inside_a_string_literal_is_not_flagged() {
    // A code-generation template or example emits the TEXT `process.exit(2)` as a string literal — it is
    // not a real call in THIS file. With `strip_string_literals`, the masked line no longer matches the
    // `process.exit(` pattern, so the code-gen helper is not falsely flagged. (Regression: the raw
    // per-line regex used to fire on the string's contents.)
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/codegen.ts",
        "export function emitExit(): string {\n  return 'if (err) { process.exit(2); }';\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "a process.exit inside a string literal must not fire: {:?}",
        out.findings
    );
}

#[test]
fn a_real_call_on_the_same_line_as_a_string_literal_still_fires() {
    // The mask only blanks string INTERIORS — a genuine call outside the string on the same line is still
    // seen. Proves the masking doesn't over-suppress.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/shutdown.ts",
        "export function shutdown() {\n  logger.info('shutting down'); process.exit(1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "process-exit-in-lib");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn process_exit_inside_a_scripts_dir_cli_file_is_not_flagged() {
    // process.exit is the expected/idiomatic way for a CLI entrypoint to exit, so scripts/**.cjs files are excluded outright rather than flagged.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "scripts/build.cjs",
        "function main(code) {\n  console.error('build failed');\n  process.exit(code);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_exit_at_module_top_level_is_not_scanned() {
    // Same method-scan precision limit as `async-route-no-catch`: no enclosing function body -> no symbol span -> method-scan silently skips it.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/cli.ts",
        "declare const reason: string;\nprocess.exit(1);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_exit_inside_a_sigterm_handler_in_a_signal_handling_module_is_not_flagged() {
    // A canonical graceful-shutdown module — process.exit(...) called from inside a process.on('SIGTERM', ...) handler is the idiomatic pattern, not a library-code bug.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/shutdown.ts",
        "export function registerShutdown() {\n  process.on('SIGTERM', () => {\n    process.exit(0);\n  });\n  process.on('SIGINT', () => {\n    process.exit(0);\n  });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn process_exit_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/cli.ts",
        "export function main(code: number) {\n  // zzop-process-exit-in-lib-ok: this is the CLI entrypoint, exiting here is intentional\n  process.exit(code);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "process-exit-in-lib").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- promise-race-no-cancel ---

#[test]
fn promise_race_with_no_cancellation_is_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/fetchWithTimeout.ts",
        "declare const url: string;\ndeclare function timeout(ms: number): Promise<never>;\n\nexport async function fetchWithTimeout() {\n  return await Promise.race([fetch(url), timeout(5000)]);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "promise-race-no-cancel");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

#[test]
fn promise_race_with_an_abort_controller_signal_is_not_flagged() {
    // FP-adversarial pin: the losing `fetch` is wired to `ac.signal`, so once the race settles the loser is
    // actually aborted — the `AbortController`/`signal:` veto in `absent` matches and the finding is
    // suppressed.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/fetchWithTimeout.ts",
        "declare const url: string;\ndeclare function timeout(ms: number): Promise<never>;\n\nexport async function fetchWithTimeout() {\n  const ac = new AbortController();\n  return await Promise.race([fetch(url, { signal: ac.signal }), timeout(5000)]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-race-no-cancel").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn promise_race_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/fetchWithTimeout.ts",
        "declare const url: string;\ndeclare function timeout(ms: number): Promise<never>;\n\nexport async function fetchWithTimeout() {\n  // zzop-promise-race-no-cancel-ok: timeout() is a plain in-memory timer, nothing to cancel\n  return await Promise.race([fetch(url), timeout(5000)]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "promise-race-no-cancel").is_empty(),
        "{:?}",
        out.findings
    );
}
