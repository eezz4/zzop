use crate::{
    assert_disqualifier_clause_precedes_imperative, assert_landing_precedes_imperative, hits, scan,
    TempDir,
};

/// The ARGV landing, spliced ahead of the `execFile`/`spawn` imperative.
///
/// WHY THIS RULE NEEDED ONE (`1.architecture/rules/rule-quality.md` §27 leg 3, §33, §37). The
/// disqualifying clause this rule already carried answers "might this finding be wrong" — it says the
/// rule cannot prove the dynamic part is request-derived. It says nothing about the OTHER question,
/// which is what the reader's own CORRECT edit costs, and for this remedy the answer is large: `exec`
/// and `execSync` hand their whole string to a shell, so the argv form silently unmakes every command
/// that was relying on the shell to do something. Two different axes, and until this landing the
/// message was exhaustive on one and silent on the other.
///
/// NOT A DISQUALIFIER, which is why it is pinned with the landing helper rather than the clause one.
/// A reader whose command contains a pipe still has a real finding: the interpolation is still going
/// to a shell, and that is still injection. What changes is that the one-line swap is not available to
/// them, and the message now says so before it hands them the swap.
///
/// WHY THE EXIT NAMES `shell: true` AS A TRAP RATHER THAN AS AN OPTION. It is the move a reader makes
/// when the argv rewrite breaks their pipeline, it makes the finding go quiet (this rule matches on
/// `exec`/`execSync` spellings, not on `spawn`), and it restores the exact substrate the finding is
/// about. An exit that reads as an option here would be an exit back into the defect.
///
/// POSITION, not presence. The invalidation probe for the test below is to move this constant to the
/// tail of the message: every token stays present and spelled exactly once, and the pin must go red on
/// ORDER alone.
const ARGV_REWRITE_LANDING: &str = "THE ARGV REWRITE IS NOT A DROP-IN, AND WHAT IT BREAKS FAILS QUIETLY: `exec`/`execSync` hand the whole string to a shell, so every command that RELIES on the shell — a `|` pipe, a `>` redirect, `&&`/`;` sequencing, a `*` glob, `$VAR` expansion, a `~` home reference — stops meaning what it meant the moment its parts become inert argv elements.";

// --- shell-exec-interpolation ---

#[test]
fn exec_with_template_literal_interpolation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function run(name: string) {\n  exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "shell-exec-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn exec_sync_with_string_concatenation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.js",
        "const { execSync } = require(\"child_process\");\nfunction run(name) {\n  execSync(\"ls \" + name);\n}\nmodule.exports = { run };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "shell-exec-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn cp_member_exec_with_template_interpolation_is_flagged() {
    // Member form: only the known child_process receiver aliases (`child_process`/`childProcess`/
    // `cp`) fire — the allowlist is what keeps RegExp's `.exec(` out (see the regexp fixture below).
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import * as cp from \"child_process\";\nexport function run(name: string) {\n  cp.exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "shell-exec-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn regexp_exec_with_dynamic_arg_is_not_flagged() {
    // Reviewer-verified FP shape: `pattern.exec(...)` is RegExp.prototype.exec, not a shell — a
    // plain `\b(?:exec|execSync)` boundary is satisfied at the `.`->`e` transition, so the matcher
    // instead requires a non-dot/word char before a bare `exec` (dot-guard idiom) and allows member
    // calls only on the known child_process receiver aliases. The file mentions child_process on
    // purpose, so this pins the dot-guard itself, not just the require_file gate.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/version.ts",
        "import { execFile } from \"child_process\";\ndeclare const pattern: RegExp;\ndeclare const version: string;\ndeclare const x: string;\nexport const m1 = pattern.exec(`v${version}`);\nexport const m2 = pattern.exec(\"pre\" + x);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn bare_exec_interpolation_without_a_child_process_mention_is_gate_skipped() {
    // require_file gate claim: a file that never mentions `child_process` cannot be shelling out
    // through it, so a same-named local `exec` helper with an interpolated arg stays silent.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/query.ts",
        "declare function exec(q: string): unknown;\nexport function run(table: string) {\n  return exec(`analyze ${table}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn exec_with_a_fixed_string_literal_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function cleanup() {\n  exec(\"rm -rf /tmp/cache\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn exec_file_with_argv_array_and_interpolated_arg_is_not_flagged() {
    // Documented boundary: execFile/spawn (argv-array APIs) are deliberately not matched, even
    // when one of their array elements is itself interpolated.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { execFile } from \"child_process\";\nexport function run(name: string) {\n  execFile(\"ls\", [`${name}`]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn spawn_with_argv_array_and_interpolated_arg_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { spawn } from \"child_process\";\nexport function run(name: string) {\n  spawn(\"ls\", [`${name}`]);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}

/// POSITION pin on a DELIVERED finding, carrying BOTH claims this rule now makes ahead of its remedy:
/// the disqualifier (this finding may be wrong) and the landing (the remedy costs something even when
/// the finding is right). Both must be reached before the imperative, because a reader who acts on the
/// first instruction never reaches anything placed behind it.
///
/// Two helpers rather than one because the two make different claims and the panic prose says which:
/// a landing that failed the disqualifier helper's message would tell the next author the finding is
/// unreliable, which is not what went wrong.
#[test]
fn the_argv_landing_and_the_disqualifier_both_precede_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function run(name: string) {\n  exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "shell-exec-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    let imperative = "Use `execFile`/`spawn` with an argv array instead";
    assert_disqualifier_clause_precedes_imperative(
        "shell-exec-interpolation",
        &h[0].message,
        "whether or not this rule can prove it's request-derived",
        imperative,
    );
    assert_landing_precedes_imperative(
        "shell-exec-interpolation",
        &h[0].message,
        ARGV_REWRITE_LANDING,
        imperative,
    );
    // The facts the landing exists to carry. Presence, unlike order, is what a rewrite loses — and the
    // worked example is the load-bearing half: a reader who is told only that "shell features break"
    // still has to be shown that the failure is a wrong RESULT, not an error at the call.
    for needle in [
        "`exec('ls *.log | wc -l > out')`",
        "counts no lines and writes no file",
        "Read the command string before you rewrite it",
        "`spawn`'s stdio",
        "expand the glob with a directory read",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/shell-exec-interpolation: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
    // The trap exit, pinned separately: `shell: true` is the move this landing exists to head off, and
    // it must stay named as the thing that RESTORES the defect rather than as a third way out.
    let trap = h[0]
        .message
        .find("reaching for `shell: true`")
        .expect("the shell: true trap left the message");
    assert!(
        h[0].message[trap..].contains("restores the exact injection this finding is about"),
        "security/shell-exec-interpolation: `shell: true` is named but no longer as a trap: {}",
        h[0].message
    );
}

#[test]
fn shell_exec_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function run(name: string) {\n  // zzop-shell-exec-interpolation-ok: name is validated against an internal allow-list above\n  exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}
