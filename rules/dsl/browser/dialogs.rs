//! `no-document-write` — the half of this file that stayed. `no-system-dialogs` was exported to
//! `examples/packs/code-hygiene.json` on 2026-08-12 (`axis: opinion`); its tests live at
//! `examples/packs/tests/system_dialogs.rs`, and the two fixtures that judged BOTH rules at once are
//! duplicated there rather than cut, so neither side's negative lost what it was measured against.

use crate::{assert_landing_precedes_imperative, scan, TempDir};

/// §33/§37 LANDING for `no-document-write`, spliced ahead of "Build DOM nodes (or set textContent)".
///
/// WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). The
/// message already names what `document.write` does WRONG — it breaks parsing after load and can
/// clobber the page — and then names a substitute whose differences it does not name. Three of them,
/// and two fail without raising: the DOM calls need a parent that exists (a call reached from `<head>`
/// meets `document.body === null`) and a position the surrounding markup may never have named; a
/// `<script src>` written into the token stream is parser-blocking while an appended script element
/// loads asynchronously unless `async` is set false, so the consumer that used to find the library
/// defined now reads `undefined`; and `textContent`, the message's own second option, prints markup as
/// characters.
///
/// WHY NOT `SANITIZER_SUBTRACTION_LANDING`, the closest sibling in this batch and therefore the most
/// dangerous reuse (§37). That constant's noun is an ALLOW-LIST, and no sanitizer appears anywhere in
/// this remedy — splicing it here would ship a paragraph about `sanitize-html` defaults to a reader who
/// is replacing a parser call. The `textContent` half overlaps, which is exactly why this constant
/// states that half in its own bytes rather than borrowing them.
///
/// NOT A DISQUALIFIER. A reader calling `document.write` has a real finding either way; what changes is
/// that the substitute is three decisions rather than a rename.
///
/// POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
/// every token stays present and spelled exactly once, and the pin goes red on ORDER alone.
const DOCUMENT_WRITE_TIMING_LANDING: &str = "THE DOM CALLS DO NOT INSERT WHERE OR WHEN `document.write` DID, AND TWO OF THE THREE DIFFERENCES FAIL WITHOUT RAISING. `document.write` splices its string into the token stream at the parser's current position, so there is no node to append to yet: a call reached from `<head>` meets a `document.body` that is still `null`, and one that ran mid-body has to be given an explicit parent and a position that the markup around it may never have named. A `<script src=...>` written that way is parser-blocking — the parser stops, fetches it and runs it before the next tag — whereas a script element you create and append loads ASYNCHRONOUSLY unless `async` is set to false, so code that used to find the library defined now runs first and reads `undefined`. And `textContent` is not a substitute for markup at all: it prints tags as characters, so whatever this call was writing as HTML appears as text. Read what the call actually writes before replacing it — a plain string is a one-line change, a script tag is an ordering change, and markup aimed at the parse position needs a container that exists at the moment it runs.";

/// §33/§37 LANDING pin. This rule's axis-A verdict is a `NO_DISQUALIFIER` row (suppression boilerplate
/// plus, from this change on, a landing); this is the other axis, on a delivered finding.
#[test]
fn no_document_write_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "w.ts",
        "export function f() {\n  document.write(\"<b>x</b>\");\n}\n",
    );
    let out = scan(&dir);
    let hits: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "browser/no-document-write")
        .collect();
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "no-document-write",
        &hits[0].message,
        DOCUMENT_WRITE_TIMING_LANDING,
        "Build DOM nodes (or set textContent) instead",
    );
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

/// Receiver-awareness, from this pack's side: a dialog-shaped call on an unrelated object leaves the
/// WHOLE pack silent. The `code-hygiene` copy of this fixture asserts the exported dialog rule declined
/// it specifically; this one is the broader claim, and stays broad on purpose — it is also what catches
/// an unrelated browser rule firing on the shape.
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

/// The suppression fixture carries one marker per rule, and the markers are deliberately distinct — a
/// shared one would let suppressing one rule silently suppress the other (the `rule_contracts`
/// meta-test checks this). Kept whole here: the `zzop-no-system-dialogs-ok` line is what proves this
/// pack's marker does not swallow the other rule's finding, which is only visible while both are in the
/// file. `examples/packs/tests/system_dialogs.rs` holds the other half.
#[test]
fn browser_ok_comment_on_or_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-browser");
    dir.write(
        "exempt.ts",
        "export function f() {\n  // zzop-no-document-write-ok: legacy print path\n  document.write(\"x\");\n  alert(\"y\"); // zzop-no-system-dialogs-ok: deliberate\n}\n",
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

// `no-document-write` uses `skip_comment_lines` plus the shared test-path `file_exclude_pattern`:
// `document.write` in a test fixture path is not shipped browser code.

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
