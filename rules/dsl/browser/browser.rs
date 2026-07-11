//! Exercises `rules/dsl/browser/browser.json`'s browser-footgun rules end-to-end via
//! `zzop_engine::analyze_tree`, routing through real engine dispatch rather than calling the line-scan
//! matcher directly.
//!
//! Bare `confirm()`/`alert()`/`prompt()` trip `no-system-dialogs`; `document.write`/`writeln` trip
//! `no-document-write`. `Matcher::LineScan` reports at most one finding per line per rule, so fixtures
//! spread one call per line to exercise each independently. `window.confirm`/`globalThis.alert` are
//! flagged with the receiver kept in the snippet; a call on an unrelated receiver, or a clean file,
//! produces no findings.
//!
//! `// browser-ok` (on the finding's line or the 3 lines above) suppresses `no-system-dialogs`;
//! `// document-write-ok` does the same for `no-document-write` — distinct markers, since a shared one
//! would let suppressing one rule silently suppress the other (`rule_contracts.rs` checks this).
//!
//! `postmessage-wildcard` flags `postMessage(..., '*')`; `// postmessage-target-ok` suppresses it.
//! `unsafe-html-sink` flags a non-literal `.innerHTML`/`.outerHTML` assignment (`innerhtml-assign`), a
//! backtick template assignment that interpolates (`innerhtml-template`), `insertAdjacentHTML(...)` whose
//! html argument isn't a plain literal (`insert-adjacent`), or JSX `dangerouslySetInnerHTML={{ __html: ... }}`
//! whose value isn't a plain literal (`dangerously-set`); `// unsafe-html-ok` suppresses it. Both rules are
//! source-free (unlike `security/taint-flow`, which needs a request-derived source in the same function and
//! only looks at `.ts`/`.tsx`) and cover `.js`/`.jsx` too.
//!
//! `javascript-url` flags a literal `javascript:`-scheme URL in a link/src position: a JSX/HTML attribute
//! literal (`jsx-href-literal`), a `.href`/`.src` property assignment (`href-assign`), or a `setAttribute`
//! call (`setattr-js`); `// javascript-url-ok` suppresses it. Scope-limited to the literal form — a dynamic
//! `href={x}` is not attempted.
//!
//! `location-assign-dynamic` flags a non-literal assignment to the client navigation sink (`location`/
//! `window.location`/`location.href` = ..., or `location.assign(...)`/`location.replace(...)`);
//! `// location-assign-ok` suppresses it. Receiver-aware like `no-document-write` (an arbitrary object's
//! own `.location` field, e.g. `user.location = x`, is never matched — only the bare global/`window.`
//! form is); a literal string/`/`-prefixed path stays silent; a `const location = ...` declaration
//! (the React Router `useLocation()` shape) is excluded via `exclude_pattern`.
//!
//! `jquery-html-sink` flags a jQuery HTML-insertion method (`.html`/`.append`/`.prepend`/`.after`/
//! `.before`/`.wrapAll`) called with a non-literal argument, gated on the file mentioning `jquery` or a
//! `$(` call; `// jquery-html-ok` suppresses it.
//!
//! `vue-v-html` flags Vue's `v-html` directive (`.vue` files, plus `.ts`/`.tsx`/`.js`/`.jsx`);
//! `// vue-v-html-ok` suppresses it.
//!
//! `unsanitized-markdown-html` (method-scan) flags a markdown-render call (`marked(`/`markdownit(`/
//! `md.render(`/`remark(`/`showdown(`) co-occurring with an HTML sink (`innerHTML`/`outerHTML`/
//! `dangerouslySetInnerHTML`/`v-html`) in the same function span, with no `DOMPurify`/`sanitize`/
//! `sanitizeHtml`/`xss` token anywhere in that span; `// markdown-html-ok` suppresses it. `.vue` is in its
//! file pattern for forward-compatibility only — this engine has no symbol/span parser for `.vue` today
//! (see `docs/rules/dsl-reference.md`'s method-scan doc), so a `.vue` SFC's `<script>`+`<template>`
//! pairing never actually co-fires; only same-file `.ts`/`.tsx`/`.js`/`.jsx` co-occurrence does.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Loads the real `browser.json` pack from the repo, one directory down from this test file.
fn browser_pack() -> RulePackDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl/browser/browser.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("parse browser.json")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "browser-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs: vec![browser_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

#[test]
fn bare_confirm_alert_prompt_each_flagged_no_system_dialogs() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "ui.ts",
        "export function ask() {\n  const ok = confirm(\"sure?\");\n  if (ok) {\n    alert(\"done\");\n  }\n  return prompt(\"name\");\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/no-system-dialogs")
        .collect();
    assert_eq!(hits.len(), 3, "expected 3 hits, got: {:?}", out.findings);
    assert!(hits.iter().all(|f| f.file == "ui.ts"));
}

#[test]
fn window_confirm_and_globalthis_alert_flagged_with_receiver() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "g.ts",
        "export function f() {\n  window.confirm(\"a\");\n  globalThis.alert(\"b\");\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/no-system-dialogs")
        .collect();
    assert_eq!(hits.len(), 2, "expected 2 hits, got: {:?}", out.findings);
    assert!(hits.iter().any(|f| f.line == 2));
    assert!(hits.iter().any(|f| f.line == 3));
}

#[test]
fn document_write_and_writeln_each_flagged_no_document_write() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "w.ts",
        "export function f() {\n  document.write(\"<b>x</b>\");\n  document.writeln(\"y\");\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/no-document-write")
        .collect();
    assert_eq!(hits.len(), 2, "expected 2 hits, got: {:?}", out.findings);
}

#[test]
fn member_call_on_unrelated_object_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "ok.ts",
        "declare const logger: any;\ndeclare const db: any;\nexport function f() { logger.alert(\"x\"); db.prompt(\"y\"); }\n",
    );
    let out = scan(&dir);
    assert!(out
        .findings
        .iter()
        .all(|f| !f.rule_id.starts_with("browser/")));
}

#[test]
fn clean_frontend_file_has_zero_findings() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "clean.ts",
        "export const greet = (n: string) => \"hi \" + n;\n",
    );
    let out = scan(&dir);
    assert!(out
        .findings
        .iter()
        .all(|f| !f.rule_id.starts_with("browser/")));
}

#[test]
fn browser_ok_comment_on_or_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "exempt.ts",
        "export function f() {\n  // document-write-ok: legacy print path\n  document.write(\"x\");\n  alert(\"y\"); // browser-ok: deliberate\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| !f.rule_id.starts_with("browser/")),
        "{:?}",
        out.findings
    );
}

/// `win.document.write(...)` on a popup handle must NOT fire — the scanner is receiver-aware and only
/// flags the bare global `document`, not an arbitrary variable named `document`.
#[test]
fn document_write_on_a_window_handle_receiver_is_skipped() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "export.ts",
        "export function printGrid(html: string) {\n  const win = window.open(\"\", \"_blank\");\n  if (!win) return;\n  win.document.write(html);\n  win.document.close();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "browser/no-document-write"),
        "{:?}",
        out.findings
    );
}

#[test]
fn bare_document_write_still_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "legacy.ts",
        "export function inject(html: string) {\n  document.write(html);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/no-document-write")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

/// Interface/type-literal method signatures shaped like dialogs (`prompt(input: string): Promise<...>;`)
/// are declarations, not calls, and are never flagged.
#[test]
fn dialog_shaped_interface_signatures_are_not_calls() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "api.ts",
        "export interface NanoSession {\n  prompt(input: string): Promise<string>;\n  alert(msg: string): void;\n}\nexport function ask(s: NanoSession) {\n  return s.prompt(\"hi\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "browser/no-system-dialogs"),
        "{:?}",
        out.findings
    );
}

/// The signature-exclude pattern must not swallow a one-line method that has a body — the `{` keeps the
/// line eligible, so a genuine `alert(` call inside it still fires.
#[test]
fn one_line_method_body_with_a_dialog_call_still_fires() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "cls.ts",
        "export class Notifier {\n  warn(msg: string): void { alert(msg); }\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/no-system-dialogs")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

// Both rules use `skip_comment_lines` plus a shared test-path `file_exclude_pattern`: `document.write`
// in a test fixture path is not shipped browser code.

#[test]
fn document_write_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/legacy.ts",
        "export function inject(html: string) {\n  document.write(html);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/no-document-write"),
        "{:?}",
        out.findings
    );
}

// --- postmessage-wildcard ---

#[test]
fn postmessage_with_single_quoted_wildcard_target_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge.ts",
        "export function broadcast(data: unknown) {\n  window.postMessage(data, '*');\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/postmessage-wildcard")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

#[test]
fn postmessage_with_double_quoted_wildcard_target_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge2.ts",
        "export function relay(payload: unknown) {\n  parent.postMessage(payload, \"*\");\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/postmessage-wildcard")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

#[test]
fn postmessage_with_explicit_origin_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge3.ts",
        "export function broadcast(data: unknown) {\n  window.postMessage(data, 'https://example.com');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}

#[test]
fn postmessage_wildcard_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge4.ts",
        "export function broadcast(data: unknown) {\n  // window.postMessage(data, '*'); -- old behavior, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}

#[test]
fn postmessage_target_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "bridge5.ts",
        "export function broadcast(data: unknown) {\n  // postmessage-target-ok: non-sensitive heartbeat ping\n  window.postMessage(data, '*');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}

#[test]
fn postmessage_wildcard_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/bridge.ts",
        "export function broadcast(data: unknown) {\n  window.postMessage(data, '*');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/postmessage-wildcard"),
        "{:?}",
        out.findings
    );
}

// --- unsafe-html-sink ---

#[test]
fn innerhtml_assign_with_a_variable_is_flagged_innerhtml_assign() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "render.ts",
        "declare const el: HTMLElement;\ndeclare const userInput: string;\nexport function render() {\n  el.innerHTML = userInput;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 4);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("innerhtml-assign")
    );
}

#[test]
fn outerhtml_plus_equals_with_a_call_is_flagged_innerhtml_assign() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "append.ts",
        "declare const el: HTMLElement;\ndeclare function getHtml(): string;\nexport function append() {\n  el.outerHTML += getHtml();\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn innerhtml_plain_string_literal_assignment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "safe.ts",
        "declare const el: HTMLElement;\nexport function render() {\n  el.innerHTML = \"<b>safe</b>\";\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_strict_equality_comparison_is_not_flagged() {
    // FP guard: `el.innerHTML === originalHtml` is a read + comparison, not an assignment — the `=` added
    // to the negative char class rejects the second `=` of `===`/`==` right after the assignment position.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "cmp.ts",
        "declare const el: HTMLElement;\ndeclare const originalHtml: string;\nexport function unchanged() {\n  return el.innerHTML === originalHtml;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_loose_equality_comparison_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "cmp2.ts",
        "declare const target: HTMLElement;\ndeclare const prev: string;\nexport function same() {\n  return target.innerHTML == prev;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_inequality_comparison_is_not_flagged() {
    // FP guard: `el.innerHTML != x` — the `!` sits where the pattern demands `[+]?=`, so the assignment
    // position never matches and the negative class never even gets consulted.
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "cmp3.ts",
        "declare const el: HTMLElement;\ndeclare const x: string;\nexport function changed() {\n  return el.innerHTML != x;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_plain_template_literal_with_no_interpolation_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "safe2.ts",
        "declare const el: HTMLElement;\nexport function render() {\n  el.innerHTML = `<b>safe</b>`;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn innerhtml_template_literal_with_interpolation_is_flagged_innerhtml_template() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "greet.ts",
        "declare const el: HTMLElement;\ndeclare const name: string;\nexport function render() {\n  el.innerHTML = `<b>${name}</b>`;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("innerhtml-template")
    );
}

#[test]
fn insert_adjacent_html_with_a_variable_argument_is_flagged_insert_adjacent() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "insert.ts",
        "declare const el: HTMLElement;\ndeclare const userHtml: string;\nexport function insert() {\n  el.insertAdjacentHTML('beforeend', userHtml);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("insert-adjacent")
    );
}

#[test]
fn insert_adjacent_html_with_a_literal_html_argument_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "insert2.ts",
        "declare const el: HTMLElement;\nexport function insert() {\n  el.insertAdjacentHTML('beforeend', '<b>safe</b>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn dangerously_set_inner_html_with_a_variable_is_flagged_dangerously_set() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Comp.tsx",
        "declare const data: { html: string };\nexport function Comp() {\n  return <div dangerouslySetInnerHTML={{ __html: data.html }} />;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsafe-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("dangerously-set")
    );
}

#[test]
fn dangerously_set_inner_html_with_a_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Comp2.tsx",
        "export function Comp() {\n  return <div dangerouslySetInnerHTML={{ __html: \"<b>safe</b>\" }} />;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsafe_html_sink_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented.ts",
        "declare const el: HTMLElement;\ndeclare const userInput: string;\nexport function render() {\n  // el.innerHTML = userInput; -- old implementation, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsafe_html_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted.ts",
        "declare const el: HTMLElement;\ndeclare const trusted: string;\nexport function render() {\n  // unsafe-html-ok: value is sanitized upstream via DOMPurify\n  el.innerHTML = trusted;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsafe_html_sink_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/render.ts",
        "declare const el: HTMLElement;\ndeclare const userInput: string;\nexport function render() {\n  el.innerHTML = userInput;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsafe-html-sink"),
        "{:?}",
        out.findings
    );
}

// --- javascript-url ---

#[test]
fn jsx_href_literal_javascript_url_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Link.tsx",
        "export function Link() {\n  return <a href=\"javascript:alert(1)\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/javascript-url")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("jsx-href-literal")
    );
}

#[test]
fn href_property_assignment_to_javascript_url_is_flagged_href_assign() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "nav.ts",
        "declare const a: HTMLAnchorElement;\nexport function wire() {\n  a.href = 'javascript:void(0)';\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/javascript-url")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("href-assign")
    );
}

#[test]
fn set_attribute_javascript_url_is_flagged_setattr_js() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "setattr.ts",
        "declare const a: HTMLAnchorElement;\nexport function wire() {\n  a.setAttribute('href', 'javascript:doIt()');\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/javascript-url")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("setattr-js")
    );
}

#[test]
fn https_href_is_not_flagged_javascript_url() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "safe-link.tsx",
        "export function Link() {\n  return <a href=\"https://example.com\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn relative_href_is_not_flagged_javascript_url() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "rel-link.tsx",
        "export function Link() {\n  return <a href=\"/dashboard\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

/// Scope-limit claim: a dynamic (non-literal) `href` is NOT caught — only the literal `javascript:` form.
#[test]
fn dynamic_href_expression_is_not_flagged_javascript_url() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "dyn-link.tsx",
        "declare const safeUrl: string;\nexport function Link() {\n  return <a href={safeUrl}>click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn javascript_url_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented-url.ts",
        "export function f() {\n  // a.href = 'javascript:alert(1)'; -- old, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn javascript_url_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-url.ts",
        "declare const a: HTMLAnchorElement;\nexport function wire() {\n  // javascript-url-ok: intentional no-op affordance\n  a.href = 'javascript:void(0)';\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

#[test]
fn javascript_url_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/link.tsx",
        "export function Link() {\n  return <a href=\"javascript:alert(1)\">click</a>;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/javascript-url"),
        "{:?}",
        out.findings
    );
}

// --- location-assign-dynamic ---

#[test]
fn bare_location_assigned_a_dynamic_value_is_flagged_location_href() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect.ts",
        "declare const target: string;\nexport function go() {\n  location = target;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

#[test]
fn window_location_href_assigned_a_dynamic_value_is_flagged_location_href() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect2.ts",
        "declare const target: string;\nexport function go() {\n  window.location.href = target;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

#[test]
fn location_assign_call_with_a_dynamic_value_is_flagged_location_assign_call() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect3.ts",
        "declare const target: string;\nexport function go() {\n  location.assign(target);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-assign-call")
    );
}

#[test]
fn location_replace_call_with_a_dynamic_value_is_flagged_location_assign_call() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect4.ts",
        "declare const target: string;\nexport function go() {\n  window.location.replace(target);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-assign-call")
    );
}

#[test]
fn location_href_relative_path_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "login.ts",
        "export function goLogin() {\n  location.href = '/login';\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

#[test]
fn location_href_absolute_url_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "external.ts",
        "export function goExternal() {\n  location.href = \"https://x.com\";\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

/// Calibration pin (opus-reviewer): the immich `getBaseUrl() + '/admin/…/' + filename` shape is NOT a
/// navigation sink — prepending a base pins the origin/path, so the trailing dynamic segment is a path
/// component, not a scheme/origin. The `exclude_pattern`'s `+ '/…'` path-literal-concat alternative vetoes
/// it. This is the false positive that motivated adding that alternative.
#[test]
fn base_plus_path_literal_concat_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "backup.ts",
        "declare function getBaseUrl(): string;\ndeclare const filename: string;\nexport function download() {\n  location.href = getBaseUrl() + '/admin/database-backups/' + filename;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

/// Literal-first concat (`'/admin/' + x`): the RHS opens with a string path literal, so the navigation
/// path is pinned by that leading literal. Silent for two independent reasons — the `line_pattern`'s
/// negative class rejects a leading quote, AND the `exclude_pattern` would veto a `+ '/…'` concat anyway —
/// this pins the literal-first form the previous suite only covered for a whole-literal `"https://x.com"`.
#[test]
fn literal_first_path_concat_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "adminnav.ts",
        "declare const x: string;\nexport function go() {\n  location.href = '/admin/' + x;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

/// TP preserved after the exclude: a bare dynamic value (`location.href = returnUrl`) is a classic open
/// redirect — nothing pins the destination, no `+ '/…'` concat to veto — must still fire.
#[test]
fn bare_dynamic_return_url_still_fires_after_the_concat_exclude() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "redirect5.ts",
        "declare const returnUrl: string;\nexport function go() {\n  location.href = returnUrl;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

/// TP preserved after the exclude: a query-suffix concat (`base + '?next=' + q`) does NOT pin the
/// destination origin — the `+ '?…'` literal is a query string, not a `+ '/…'` path literal, so the
/// exclude's path-concat alternative does not match it and the finding still fires.
#[test]
fn base_plus_query_suffix_concat_still_fires_after_the_concat_exclude() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "querysuffix.ts",
        "declare const base: string;\ndeclare const q: string;\nexport function go() {\n  location.href = base + '?next=' + q;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/location-assign-dynamic")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(
        hits[0]
            .data
            .as_ref()
            .and_then(|d| d.get("label"))
            .and_then(|v| v.as_str()),
        Some("location-href")
    );
}

/// Receiver-aware claim: an arbitrary domain object's own `.location` field is never matched — only the
/// bare global/`window.` form is.
#[test]
fn unrelated_object_location_field_assignment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "user.ts",
        "declare const user: { location: string };\ndeclare const newAddress: string;\nexport function move() {\n  user.location = newAddress;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

/// `const location = useLocation();` (React Router) is a declaration, not a navigation assignment.
#[test]
fn use_location_hook_declaration_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "route.tsx",
        "declare function useLocation(): { pathname: string };\nexport function Page() {\n  const location = useLocation();\n  return location.pathname;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

#[test]
fn location_assign_dynamic_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented-loc.ts",
        "declare const target: string;\nexport function go() {\n  // location.href = target; -- old, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

#[test]
fn location_assign_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-loc.ts",
        "declare const target: string;\nexport function go() {\n  // location-assign-ok: target is checked against an allowlist above\n  location.href = target;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

#[test]
fn location_assign_dynamic_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/redirect.ts",
        "declare const target: string;\nexport function go() {\n  location.href = target;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/location-assign-dynamic"),
        "{:?}",
        out.findings
    );
}

// --- jquery-html-sink ---

#[test]
fn jquery_html_call_with_a_variable_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  $('#box').html(userHtml);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/jquery-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn jquery_append_with_a_variable_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget2.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  $('#box').append(userHtml);\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/jquery-html-sink")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn jquery_text_call_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget3.js",
        "import $ from 'jquery';\nexport function render(userText) {\n  $('#box').text(userText);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_call_with_a_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "widget4.js",
        "import $ from 'jquery';\nexport function render() {\n  $('#box').html('<b>static</b>');\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

/// Non-jQuery `.append(` on an unrelated object in a file that never mentions jQuery/`$(` is not flagged —
/// the `require_file` gate keeps this rule honest outside jQuery codebases.
#[test]
fn append_call_in_a_non_jquery_file_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "list.js",
        "export function addItem(list, item) {\n  list.append(item);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_sink_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "commented-jq.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  // $('#box').html(userHtml); -- old, removed\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-jq.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  // jquery-html-ok: sanitized via DOMPurify above\n  $('#box').html(userHtml);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

#[test]
fn jquery_html_sink_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/widget.js",
        "import $ from 'jquery';\nexport function render(userHtml) {\n  $('#box').html(userHtml);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/jquery-html-sink"),
        "{:?}",
        out.findings
    );
}

// --- vue-v-html ---

#[test]
fn v_html_directive_in_a_vue_file_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Article.vue",
        "<template>\n  <div v-html=\"renderedHtml\"></div>\n</template>\n<script setup>\nconst renderedHtml = article.body;\n</script>\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/vue-v-html")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].line, 2);
}

#[test]
fn interpolation_binding_in_a_vue_file_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Safe.vue",
        "<template>\n  <div>{{ plainText }}</div>\n</template>\n<script setup>\nconst plainText = 'hi';\n</script>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn v_html_mentioned_only_in_a_vue_template_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Commented.vue",
        "<template>\n  // v-html=\"x\" (not real Vue syntax, just exercising the JS-style comment skip)\n</template>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn vue_v_html_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Vetted.vue",
        "<template>\n  <!-- vue-v-html-ok: sanitized upstream via DOMPurify -->\n  // vue-v-html-ok: sanitized upstream via DOMPurify\n  <div v-html=\"trusted\"></div>\n</template>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn vue_v_html_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/Article.vue",
        "<template>\n  <div v-html=\"renderedHtml\"></div>\n</template>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/vue-v-html"),
        "{:?}",
        out.findings
    );
}

// --- unsanitized-markdown-html ---

#[test]
fn marked_output_into_inner_html_with_no_sanitizer_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Post.tsx",
        "import { marked } from 'marked';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  const html = marked(article.body);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsanitized-markdown-html")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn markdown_it_render_into_dangerously_set_inner_html_with_no_sanitizer_is_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Post2.tsx",
        "declare const md: { render(s: string): string };\ndeclare const article: { body: string };\nexport function Comp() {\n  const html = md.render(article.body);\n  return <div dangerouslySetInnerHTML={{ __html: html }} />;\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/unsanitized-markdown-html")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn marked_output_sanitized_with_dompurify_before_inner_html_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "SafePost.tsx",
        "import { marked } from 'marked';\nimport DOMPurify from 'dompurify';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  const raw = marked(article.body);\n  const html = DOMPurify.sanitize(raw);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

/// Bare-word claim: `marked`/`remark` are ordinary English words too, so the pattern requires call syntax
/// (`marked(`) — plain prose mentioning the word with no call form must not fire.
#[test]
fn marked_as_a_plain_english_word_in_a_string_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "prose.ts",
        "declare const el: HTMLElement;\nexport function render() {\n  const note = 'this field is marked as required';\n  el.innerHTML = note;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

/// Co-occurrence-limitation claim: a markdown render in one function and the HTML sink in a different
/// function (different spans) does not co-fire.
#[test]
fn marked_render_and_sink_in_different_functions_does_not_co_fire() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "split.ts",
        "import { marked } from 'marked';\ndeclare const article: { body: string };\nexport function toHtml() {\n  return marked(article.body);\n}\ndeclare const el: HTMLElement;\nexport function render(html: string) {\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

/// Documented `.vue` limitation: this engine has no symbol/span parser for `.vue`, so a `.vue` file's
/// `<script>`+`<template>` co-occurrence (the exact fe-vue corpus shape) does not co-fire even though
/// `.vue` is in the rule's `file_pattern`.
#[test]
fn marked_and_v_html_in_the_same_vue_sfc_does_not_co_fire_no_span_support() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "Article.vue",
        "<template>\n  <div v-html=\"renderedHtml\"></div>\n</template>\n<script setup>\nimport { marked } from 'marked';\ndeclare const article: { body: string };\nconst renderedHtml = marked(article.body);\n</script>\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn markdown_html_ok_marker_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "vetted-md.tsx",
        "import { marked } from 'marked';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  // markdown-html-ok: sanitize option enabled in marked config\n  const html = marked(article.body);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}

#[test]
fn unsanitized_markdown_html_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "__tests__/Post.tsx",
        "import { marked } from 'marked';\ndeclare const el: HTMLElement;\ndeclare const article: { body: string };\nexport function render() {\n  const html = marked(article.body);\n  el.innerHTML = html;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        out.findings
            .iter()
            .all(|f| f.rule_id != "browser/unsanitized-markdown-html"),
        "{:?}",
        out.findings
    );
}
