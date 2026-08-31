//! The two rules added 2026-08-17 to carry SQL/command interpolation into Python and Go, pinned here
//! because nothing else does. Both were verified by an invalidation drill at authoring time, but a drill
//! proves the rule fired ONCE — it does not survive the next edit to a shared regex. The dogfood corpus
//! cannot stand in for these tests either: measured 2026-08-18 over `corpus/oss`'s 17 trees, both rules
//! fire ZERO times, because every backend there reaches its database through an ORM. A rule with no
//! fixture and no corpus hit is a rule whose breakage is silent, which is the one failure this repo
//! treats as cardinal.
//!
//! Each arm gets its own fixture rather than one fixture per rule: the arms are separate alternatives
//! pinned to ONE quote character each (a deliberate choice, so `"… ' …"` cannot straddle two literals),
//! and a single fixture would leave four of the five free to rot.

use crate::{hits, scan, TempDir};

// --- sql-interpolated-statement (line-scan, `.py`/`.go` only) ---

#[test]
fn python_fstring_select_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/repo.py",
        "def find(cur, uid):\n    cur.execute(f\"SELECT * FROM users WHERE id = {uid}\")\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-interpolated-statement");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn python_percent_format_delete_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/repo.py",
        "def purge(cur, uid):\n    cur.execute(\"DELETE FROM sessions WHERE id = %s\" % uid)\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "sql-interpolated-statement").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn python_str_format_update_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/repo.py",
        "def rename(cur, name, uid):\n    cur.execute(\"UPDATE users SET name = '{}' WHERE id = 1\".format(name))\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "sql-interpolated-statement").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn go_sprintf_select_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "internal/store.go",
        "package store\n\nimport \"fmt\"\n\nfunc Find(uid string) string {\n\treturn fmt.Sprintf(\"SELECT * FROM users WHERE id = %s\", uid)\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sql-interpolated-statement");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
}

/// The concat arm's Go alternative is keyed on the RAW-STRING backtick, which is the spelling Go
/// reaches for when a query contains quotes. A separate arm from `fmt.Sprintf` because the two share
/// no syntax.
#[test]
fn go_raw_string_concatenation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "internal/store.go",
        "package store\n\nfunc Find(uid string) string {\n\treturn `SELECT * FROM users WHERE id = ` + uid\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "sql-interpolated-statement").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// The parameterized form is the REMEDY the message prescribes, so it must clear the rule — a finding a
/// user cannot clear by following its own advice is one they turn off, taking the true positives with
/// it.
#[test]
fn a_parameterized_query_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/repo.py",
        "def find(cur, uid):\n    cur.execute(\"SELECT * FROM users WHERE id = %s\", (uid,))\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-interpolated-statement").is_empty(),
        "{:?}",
        out.findings
    );
}

/// Interpolation into something that is not a statement is out of scope: the arms all require a SQL
/// verb INSIDE the literal, so an f-string with no query in it must stay silent.
#[test]
fn interpolation_without_a_sql_verb_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/repo.py",
        "def label(cur, uid):\n    return f\"user {uid} selected from the list\"\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-interpolated-statement").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The rule is `.py`/`.go` only, by §22: its message names Python and Go constructs, so it cannot ship
/// on a language whose readers those sentences do not fit. The same concat shape in TypeScript belongs
/// to the sibling TS rules, and this one must not double-report it.
#[test]
fn the_same_shape_in_typescript_is_not_this_rules_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/repo.ts",
        "declare const db: any;\nexport function find(uid: string) {\n  return db.query(\"SELECT * FROM users WHERE id = \" + uid);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "sql-interpolated-statement").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- command-interpolated-string (method-scan, gated on `require_call_kind: process-exec`) ---

#[test]
fn python_subprocess_with_an_fstring_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/tools.py",
        "import subprocess\n\ndef run(name):\n    subprocess.run(f\"ls {name}\", shell=True)\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "command-interpolated-string").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn go_exec_command_with_sprintf_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "internal/tools.go",
        "package tools\n\nimport (\n\t\"fmt\"\n\t\"os/exec\"\n)\n\nfunc Run(name string) error {\n\treturn exec.Command(\"sh\", \"-c\", fmt.Sprintf(\"ls %s\", name)).Run()\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "command-interpolated-string").len(),
        1,
        "{:?}",
        out.findings
    );
}

/// `require_call_kind: "process-exec"` is the load-bearing half of this rule — without it the same
/// interpolation regex fires on every formatted string in the file. This fixture holds an interpolated
/// literal in a file that DOES import `subprocess` (so `require_file` passes), with no exec call in the
/// function: the gate, not the file filter, is what has to keep it silent.
#[test]
fn an_interpolated_string_with_no_exec_call_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/tools.py",
        "import subprocess\n\ndef describe(name):\n    return f\"about to list {name}\"\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "command-interpolated-string").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The remedy again: an argument LIST passes no shell and interpolates nothing, so it must clear.
#[test]
fn an_argument_list_without_interpolation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/tools.py",
        "import subprocess\n\ndef run(name):\n    subprocess.run([\"ls\", name])\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "command-interpolated-string").is_empty(),
        "{:?}",
        out.findings
    );
}
