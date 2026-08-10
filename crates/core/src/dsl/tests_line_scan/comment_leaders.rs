//! Comment-leader behaviour of `skip_comment_lines`: the leader set is keyed by EXTENSION
//! (`markers::leaders_for_path`), so the same rule reads a `--` line as a comment in `.sql` and as
//! scannable code in `.ts`. Split out of the parent file to keep it under the 300-line cap.

use super::super::test_support::{rule_pack, scan_pack};
use super::super::RulePackDef;

// --- comment leaders are keyed by EXTENSION (`markers::leaders_for_path`) ---

/// The pack both leader tests scan with: one rule that matches BOTH extensions, so the only variable
/// between the two is the file's own comment syntax.
fn dash_dash_pack() -> RulePackDef {
    rule_pack(
        r#"{"id":"drop","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"(?i)[.](sql|ts)$","skip_comment_lines":true,"line_pattern":"(?i)DROP TABLE"}}"#,
    )
}

/// `skip_comment_lines` skips a `--` line in a `.sql` file. This is the defect the one comment-leader
/// table closed: while every matcher carried its own `//`/`*`/`/*` triple, a commented-out
/// `-- DROP TABLE users;` in a migration fired as a destructive migration.
#[test]
fn skip_comment_lines_skips_a_sql_comment_in_a_sql_file() {
    let f = scan_pack(
        &dash_dash_pack(),
        "migrations/0002_drop.sql",
        "-- DROP TABLE users;
DROP TABLE orders;
",
        vec![],
    );
    // The second assertion is the one that keeps this from being a rule-killing "fix": the STATEMENT
    // on line 2 must still fire, and only the comment on line 1 must stop.
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 2);
}

/// The other half of the extension key, and the reason the table is keyed at all: `--` opens no comment
/// in TS (`--x` is a decrement), so a `.ts` line starting with `--` stays scannable. A single
/// leader set shared by every file would have silenced this line too.
#[test]
fn skip_comment_lines_does_not_read_dash_dash_as_a_comment_in_ts() {
    let f = scan_pack(
        &dash_dash_pack(),
        "src/a.ts",
        "--DROP TABLE orders;
",
        vec![],
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].line, 1);
}
