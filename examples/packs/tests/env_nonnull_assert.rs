//! `env-nonnull-assert` — moved here with the rule from `rules/dsl/reliability/config_flags.rs`, which
//! keeps the `debug-true-committed` half it was sharing a file with. The two rules share no fixture, so
//! this was a clean cut rather than a duplication.
//!
//! The rule's own cross-check with `env-outside-config` (one `process.env.X!` line firing BOTH rules)
//! moved too and lives in `env_outside_config.rs` — both halves of that pair are in this pack, so the
//! assertion did not have to become a cross-pack one.

use crate::{hits, scan, TempDir};

#[test]
fn process_env_non_null_assertion_is_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/config.ts",
        "export const key = process.env.API_KEY!;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "env-nonnull-assert");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn process_env_strict_inequality_comparison_is_not_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/config.ts",
        "export function checkEnv(): boolean {\n  if (process.env.API_KEY !== undefined) {\n    return true;\n  }\n  return false;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-nonnull-assert").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn env_assert_ok_marker_above_the_assertion_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/config.ts",
        "// zzop-env-nonnull-assert-ok: validated at startup in bootstrap.ts\nexport const key = process.env.API_KEY!;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "env-nonnull-assert").is_empty(),
        "{:?}",
        out.findings
    );
}
