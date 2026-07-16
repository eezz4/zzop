use crate::{hits, scan, TempDir};

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

#[test]
fn shell_exec_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/tools.ts",
        "import { exec } from \"child_process\";\nexport function run(name: string) {\n  // shell-exec-ok: name is validated against an internal allow-list above\n  exec(`ls ${name}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "shell-exec-interpolation").is_empty(),
        "{:?}",
        out.findings
    );
}
