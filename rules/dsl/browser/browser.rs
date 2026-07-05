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
