use crate::{scan, TempDir};

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
