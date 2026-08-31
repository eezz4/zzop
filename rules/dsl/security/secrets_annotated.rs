//! `hardcoded-secret`'s third arm (`annotated-assignment`) and the value-POSITION scoping of the
//! value-shape veto. The two ship together because they are the same defect seen from both ends: the
//! rule could not read a binding whose TYPE sits between the name and the value, and its veto could
//! not tell a quoted KEY from a quoted VALUE. Each test below pins one half against the shape that
//! motivated it, and the negative twins are the ones the veto must keep killing.

use crate::{hits, label_of, scan, TempDir};

// --- the `annotated-assignment` arm -------------------------------------------------------------

/// TypeScript's typed constant puts the type between the name and the value, exactly as Rust's
/// `const API_KEY: &str = "…"` does — the shape the `assignment` arm structurally cannot reach.
/// The rule disclosed this as a known miss until the arm landed.
#[test]
fn a_typescript_typed_string_const_secret_is_flagged_by_the_annotated_assignment_arm() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/config.ts",
        // Deliberately NOT a vendor-token shape: this pins the arm, not a prefix, and a contiguous
        // vendor literal in tracked source trips GitHub push protection for the whole repo.
        "const token: string = \"Pr0d!Mailer#2024\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
    assert_eq!(label_of(h[0]), Some("annotated-assignment"));
}

/// Python spells the same shape with `str` rather than `string`, and its annotated assignment is
/// idiomatic at module scope and in dataclasses. One arm covers both spellings; this pins that the
/// Python half is not riding on the TypeScript one by accident.
#[test]
fn a_python_annotated_str_assignment_secret_is_flagged_by_the_same_arm() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/config.py",
        "api_key: str = \"hunter2-prod-mailer9f8a\"\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("annotated-assignment"));
}

/// The arm requires a quoted LITERAL after the `=`. A typed binding initialised from a call is the
/// correct handling of a secret, not a finding — and it is the exact shape that got the "widen the
/// `assignment` arm to allow any tokens before the `=`" design rejected, so it is pinned here.
#[test]
fn a_typed_binding_initialised_from_a_call_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/config.ts",
        "const apiKey: string = getFromVault(\"prod/mailer\");\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// An arrow function's RETURN type is `): string =>`, one character away from the arm's `: string =`.
/// It cannot match, because `=>` puts a `>` where the arm requires whitespace or a quote — but that
/// is a property of the regex rather than an intention, so it is pinned rather than trusted.
#[test]
fn an_arrow_function_returning_a_string_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/config.ts",
        "const token = (f: TFunc): string => \"some-literal-value\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The new arm does not get its own veto: an identifier-shaped VALUE is still killed, the same way
/// it is on the untyped arm. Without this the arm would be a hole in the value-shape gate.
#[test]
fn an_identifier_shaped_value_on_the_annotated_arm_is_still_vetoed() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/config.ts",
        "const secret: string = \"refresh-token-name\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- value-POSITION scoping of the value-shape veto ---------------------------------------------

/// The veto exists to reject identifier-shaped VALUES, but `exclude_pattern` is evaluated against the
/// whole LINE. A multi-word quoted KEY is itself identifier-shaped, so it used to trip the veto meant
/// for the value beside it — silencing the single most ordinary Python secret layout there is. The
/// veto now only judges a string sitting in value position (after a `:` or `=`).
#[test]
fn a_secret_under_a_multi_word_quoted_key_is_no_longer_killed_by_its_own_key() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/settings.py",
        "CONFIG = {\"api_key\": \"sk-live-a1B2c3D4e5F6g7H8i9J0\"}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "hardcoded-secret");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(label_of(h[0]), Some("assignment"));
}

/// The other side of that scoping, so it cannot quietly become a blanket removal of the veto: the
/// same layout with an identifier-shaped VALUE stays silent. Key and value are now judged
/// differently, which is the whole point — the key names the secret, the value IS one or is not.
#[test]
fn the_same_layout_with_an_identifier_shaped_value_is_still_vetoed() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/settings.py",
        "CSS = {\"api_key\": \"mantine-DatePickerInput-input\"}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The documented anchors of the veto's measured drop set, re-asserted AFTER the scoping change.
/// Each sits in value position, so narrowing the veto to that position must leave every one of them
/// dead — this is what makes the change a re-scoping rather than a weakening.
#[test]
fn every_documented_veto_anchor_survives_the_value_position_scoping() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write(
        "src/ui.ts",
        "const a = { apiKey: \"Mantine_DatePicker_Input\" };\n\
         const b = { apiKey: \"MY_API_KEY_NAME\" };\n\
         const c = { token: \"mantine-DatePickerInput-input\" };\n\
         const d = { secret: \"jwt.token.here\" };\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "hardcoded-secret").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The seventh measured finding — a separator-free single word — deliberately still fires, and the
/// scoping change must not have quietly adopted it into the drop set. Its twin in
/// `secrets_vetoes.rs` pins the same fact before the change; this one pins it after.
#[test]
fn the_separator_free_word_value_still_fires_after_the_scoping_change() {
    let dir = TempDir::new("zzop-sec-annot");
    dir.write("src/ads.ts", "const ads = { token: \"adsbygoogle\" };\n");
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "hardcoded-secret").len(),
        1,
        "{:?}",
        out.findings
    );
}
