use crate::{assert_disqualifier_summary_precedes_imperative, hits, scan, TempDir};

// --- open-redirect ---

#[test]
fn redirect_of_a_request_derived_target_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\nexport function handleRedirect(req: any) {\n  const target = req.query.next;\n  res.redirect(target);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "open-redirect");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn redirect_to_a_hardcoded_path_with_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\nexport function goHome() {\n  res.redirect(\"/home\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

#[test]
fn redirect_ok_marker_above_the_redirect_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\nexport function handleRedirect(req: any) {\n  const target = req.query.next;\n  // zzop-open-redirect-ok: target validated against an internal allow-list above\n  res.redirect(target);\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "open-redirect").is_empty(), "{:?}", out.findings);
}

// --- ssrf-user-url ---

#[test]
fn fetch_of_a_request_derived_url_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/proxy.ts",
        "declare const fetch: any;\nexport async function proxy(req: any) {\n  const url = req.query.url;\n  return fetch(url);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "ssrf-user-url");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

/// §27 pin (2026-08-29). This is the most honest message of the five repaired here and it was arranged
/// the worst: it states outright that "Validating or allow-listing the target does NOT clear the finding
/// either" -- and put that 350 characters BEHIND "Validate the target against an allow-list of hosts."
/// A reader who acts on the first instruction does the allow-listing, re-runs, and sees the identical
/// finding, which is how a channel gets switched off instead of followed.
///
/// WHAT MOVED: the imperative, not the clause. Moving the clause forward would have split "The `if` is
/// load-bearing" from its antecedent -- the "if that value becomes part of the URL" in the opening
/// sentence -- and stranded "for exactly that gap" in the closing one. Sending the imperative to the
/// tail leaves every antecedent adjacent to its referent and moves one sentence. 986 characters before
/// and after, character multiset identical (the separating space travels with it).
///
/// The clause pinned is the LATER of this rule's two disqualifiers, so the assertion also holds the
/// earlier one -- "a handler that reads a query parameter for something else while calling a hard-coded
/// endpoint fires identically" -- in front of the imperative.
///
/// INVALIDATION PROBE: put the imperative back after the opening sentence. Every token stays present and
/// spelled exactly once, a `contains` pin stays green, and this assertion alone goes red.
#[test]
fn ssrf_user_url_remedy_does_not_clear_it_clause_precedes_the_validate_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/proxy.ts",
        "declare const fetch: any;\nexport async function proxy(req: any) {\n  const url = req.query.url;\n  return fetch(url);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "ssrf-user-url");
    // Sentence order only -- the finding itself is unchanged.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
    assert_disqualifier_summary_precedes_imperative(
        "ssrf-user-url",
        &h[0].message,
        "Validating or allow-listing the target does NOT clear the finding either",
        "Validate the target against an allow-list of hosts.",
        "fires identically",
    );
}

#[test]
fn fetch_of_a_hardcoded_url_with_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/proxy.ts",
        "declare const fetch: any;\nexport async function ping() {\n  return fetch(\"https://internal.example.com/health\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "ssrf-user-url").is_empty(), "{:?}", out.findings);
}

// --- path-traversal ---

#[test]
fn fs_read_of_a_path_joined_request_param_in_the_same_function_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "import * as fs from \"fs\";\nimport * as path from \"path\";\ndeclare const baseDir: string;\nexport async function readUserFile(req: any) {\n  const p = path.join(baseDir, req.params.filename);\n  return fs.readFileSync(p);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "path-traversal");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

#[test]
fn fs_read_of_a_fixed_path_with_no_request_input_or_path_join_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "import * as fs from \"fs\";\nexport function readConfig() {\n  return fs.readFileSync(\"/etc/app/config.json\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "path-traversal").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- sendfile-from-request ---

#[test]
fn send_file_of_a_request_derived_path_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function download(req: any) {\n  res.sendFile(req.params.filename);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sendfile-from-request");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn download_of_a_request_derived_path_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function getFile(req: any) {\n  res.download(req.query.path);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "sendfile-from-request").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn send_file_of_a_fixed_path_with_no_request_input_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function downloadReport() {\n  res.sendFile(\"/reports/summary.pdf\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sendfile-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn send_file_wrapped_in_path_basename_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\ndeclare const path: any;\nexport function download(req: any) {\n  res.sendFile(path.basename(req.params.filename));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sendfile-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn sendfile_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/files.ts",
        "declare const res: any;\nexport function download(req: any) {\n  // zzop-sendfile-from-request-ok: filename validated against an internal allow-list above\n  res.sendFile(req.params.filename);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sendfile-from-request").is_empty(),
        "{:?}",
        out.findings
    );
}
