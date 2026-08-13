//! `no-system-dialogs` — moved here with the rule from `rules/dsl/browser/dialogs.rs`, which keeps the
//! `no-document-write` half it was sharing a file with.
//!
//! Two of the fixtures below judged BOTH rules at once (a clean-receiver negative, and a suppression
//! fixture carrying one marker per rule). The fixture source is duplicated rather than cut, exactly as
//! `sql_preferences`'s `language_scope.rs` split did it: trimming a shared fixture down to one rule's
//! lines changes what that rule is measured AGAINST, and the two halves are only equivalent while both
//! sources stay whole. `rules/dsl/browser/dialogs.rs` holds the mirror image of each.

use crate::{hits, scan, TempDir};

#[test]
fn bare_confirm_alert_prompt_each_flagged_no_system_dialogs() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "ui.ts",
        "export function ask() {\n  const ok = confirm(\"sure?\");\n  if (ok) {\n    alert(\"done\");\n  }\n  return prompt(\"name\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "no-system-dialogs");
    assert_eq!(h.len(), 3, "expected 3 hits, got: {:?}", out.findings);
    assert!(h.iter().all(|f| f.file == "ui.ts"));
}

#[test]
fn window_confirm_and_globalthis_alert_flagged_with_receiver() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "g.ts",
        "export function f() {\n  window.confirm(\"a\");\n  globalThis.alert(\"b\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "no-system-dialogs");
    assert_eq!(h.len(), 2, "expected 2 hits, got: {:?}", out.findings);
    assert!(h.iter().any(|f| f.line == 2));
    assert!(h.iter().any(|f| f.line == 3));
}

/// The receiver-awareness negative. Same fixture as `dialogs.rs`'s copy; that one asserts no
/// `browser/`-prefixed finding, this one asserts the dialog rule specifically declined it.
#[test]
fn member_call_on_unrelated_object_is_not_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "ok.ts",
        "declare const logger: any;\ndeclare const db: any;\nexport function f() { logger.alert(\"x\"); db.prompt(\"y\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "no-system-dialogs").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The suppression fixture carries one marker per rule, and the two markers are deliberately distinct —
/// a shared one would let suppressing either rule silently suppress the other. Now that the rules live
/// in different packs that separation matters more, not less: this half pins that the dialog marker
/// still lands on the dialog line with the pack renamed underneath it.
#[test]
fn dialog_ok_comment_on_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "exempt.ts",
        "export function f() {\n  // zzop-no-document-write-ok: legacy print path\n  document.write(\"x\");\n  alert(\"y\"); // zzop-no-system-dialogs-ok: deliberate\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "no-system-dialogs").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Interface/type-literal method signatures shaped like dialogs (`prompt(input: string): Promise<...>;`)
/// are declarations, not calls, and are never flagged.
#[test]
fn dialog_shaped_interface_signatures_are_not_calls() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "api.ts",
        "export interface NanoSession {\n  prompt(input: string): Promise<string>;\n  alert(msg: string): void;\n}\nexport function ask(s: NanoSession) {\n  return s.prompt(\"hi\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "no-system-dialogs").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The signature-exclude pattern must not swallow a one-line method that has a body — the `{` keeps the
/// line eligible, so a genuine `alert(` call inside it still fires.
#[test]
fn one_line_method_body_with_a_dialog_call_still_fires() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "cls.ts",
        "export class Notifier {\n  warn(msg: string): void { alert(msg); }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "no-system-dialogs");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

/// The shared test-path `file_exclude_pattern` from the dialog rule's side. `dialogs.rs` keeps the
/// `no-document-write` copy; both rules carry `${test-paths-stories}` and neither should fire on a
/// fixture path, but after the export they carry it from two different pack files, so each side has to
/// prove its own.
#[test]
fn a_dialog_call_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "__tests__/legacy.ts",
        "export function ask() {\n  return confirm(\"sure?\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "no-system-dialogs").is_empty(),
        "{:?}",
        out.findings
    );
}
