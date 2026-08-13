//! The `#`-comment marker half of the suppression suite — the files where `//` is not a comment at all,
//! so a `//`-only marker table leaves them with no working marker. Two groups: the config formats (where
//! `//` is stray data) and `.py` (where `//` is a SyntaxError, added 2026-08-12).
//!
//! Its own file because the two axes it holds apart (`markers::marker_leaders_for_path` vs
//! `markers::leaders_for_path`) want OPPOSITE answers for the same `#`, and a reader who meets these
//! cases interleaved with the `//` and `--` ones has to reconstruct that distinction from scratch.

use super::super::test_support::{rule_pack, scan_pack};
use super::super::RulePackDef;

// --- `#`-comment marker recognition, gated to the hash-comment config-file family ---
//
// The two axes this section holds apart are the whole point, and they want OPPOSITE answers for the
// same `#` in the same file:
//   * MARKER — "is this line a comment carrying my marker?" `#` must count, or a rule matching `.env`
//     has no working marker at all (`//` is not a comment in dotenv; it is stray data).
//   * skip_comment_lines — "is this line commentary I can ignore?" For a secret scanner, NO. A
//     commented-out secret is still committed, still in git history, still copy-paste-restorable.
// `security/config-file-secret` is the rule that made this concrete: its message told readers to write
// `# zzop-config-file-secret-ok`, and the engine read `//` only, so the marker it named did nothing.

fn marker_line_pack_env() -> RulePackDef {
    rule_pack(
        r#"{"id":"config-secret","severity":"critical","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.env$","line_pattern":"TOKEN","skip_comment_lines":true}}"#,
    )
}

#[test]
fn hash_marker_on_the_same_line_suppresses_line_scan_finding_in_a_dotenv_file() {
    let f = scan_pack(
        &marker_line_pack_env(),
        ".env",
        "SVC_TOKEN=abc123 # zzop-config-secret-ok: rotated weekly\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn hash_marker_on_the_line_above_suppresses_line_scan_finding_in_a_dotenv_file() {
    let f = scan_pack(
        &marker_line_pack_env(),
        ".env",
        "# zzop-config-secret-ok: rotated weekly\nSVC_TOKEN=abc123\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

/// THE property the split exists to protect, and the reason `skip_comment_lines` must NOT learn `#`
/// here. A commented-out secret is still in the file and still in history; silencing it would be a
/// detection loss on the highest-severity pack, and `security/private-key-committed` /
/// `security/vendor-token-committed` pair the same `skip_comment_lines: true` with the same
/// `#`-comment file types, so they would lose it too.
#[test]
fn a_hash_commented_secret_still_fires_in_a_dotenv_file() {
    let f = scan_pack(
        &marker_line_pack_env(),
        ".env",
        "# SVC_TOKEN=abc123\n",
        vec![],
    );
    assert_eq!(
        f.len(),
        1,
        "a `#` in front of a secret does not un-commit it: {f:?}"
    );
}

#[test]
fn hash_marker_is_not_recognized_outside_the_hash_comment_family() {
    // Same rule text, a `.ts` file. `#` is not a comment leader there (`#field` is a private field),
    // so the `#`-marker recognizer must never activate for it — leader parity with `.sql`'s `--`.
    let pack = rule_pack(
        r#"{"id":"config-secret","severity":"critical","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.(ts|env)$","line_pattern":"TOKEN"}}"#,
    );
    let f = scan_pack(
        &pack,
        "f.ts",
        "const SVC_TOKEN = 1; # zzop-config-secret-ok\n",
        vec![],
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn slash_slash_marker_still_suppresses_in_a_dotenv_file() {
    // The `#` recognizer is ADDITIVE, exactly like `--` in `.sql`: the `//` form keeps working.
    let f = scan_pack(
        &marker_line_pack_env(),
        ".env",
        "SVC_TOKEN=abc123 // zzop-config-secret-ok\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn the_hash_family_covers_every_extension_config_file_secret_matches() {
    // Derived from the rule that motivated the split, not a hand-picked sample: these are the
    // extensions `security/config-file-secret`'s `file_pattern` accepts. One of them silently missing
    // would leave that rule's own message lying for that extension only — the hardest kind to notice.
    for rel in [
        "app.properties",
        "app.yaml",
        "app.yml",
        "app.toml",
        "app.ini",
        "app.conf",
        "app.cfg",
        ".env",
        // The `.env.<suffix>` siblings the same `file_pattern` accepts. `Path::extension` reads these
        // as extension `local`/`production`, so they are only reachable through the file-NAME branch.
        ".env.local",
        ".env.production",
        "config/.env.staging",
    ] {
        let pack = rule_pack(
            r#"{"id":"config-secret","severity":"critical","message":"m","matcher":{"type":"line-scan","file_pattern":".","line_pattern":"TOKEN"}}"#,
        );
        let f = scan_pack(
            &pack,
            rel,
            "SVC_TOKEN=abc # zzop-config-secret-ok\n",
            vec![],
        );
        assert!(f.is_empty(), "`#` marker must suppress in {rel}: {f:?}");
    }
}

// --- `.py`, the second group (2026-08-12) ---
//
// The subjects are the shipped line-scan rules whose `file_pattern` admits a `.py` path — the `sql`
// pack's whole-statement family in the bundle, plus what the exported packs carry. Until this widening
// every one of them offered a marker (`// zzop-<id>-ok`) that is a SyntaxError in Python, so the only
// lever a Python author had was a whole-FILE `rules`/`exclude` path in config for a one-LINE judgment.
// They say so in their own message rather than promise something that did nothing; those sentences now
// promise `#`.
//
// Neither the roster nor its size is retyped here. `markers/path.rs`'s `.py` entry owns both and carries
// the recount command, and every hand-written copy of this roster has gone stale so far without
// exception — this one named `security/private-key-committed`, which is patterned
// (`(?i)\.(ts|…|pem|rs)$`) and does not admit `.py` at all.

fn marker_line_pack_py() -> RulePackDef {
    rule_pack(
        r#"{"id":"pysql","severity":"critical","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.py$","line_pattern":"DELETE FROM"}}"#,
    )
}

#[test]
fn hash_marker_on_the_same_line_suppresses_line_scan_finding_in_a_python_file() {
    let f = scan_pack(
        &marker_line_pack_py(),
        "svc.py",
        "cur.execute('DELETE FROM sessions')  # zzop-pysql-ok: nightly reaper\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn hash_marker_on_the_line_above_suppresses_line_scan_finding_in_a_python_file() {
    let f = scan_pack(
        &marker_line_pack_py(),
        "svc.py",
        "# zzop-pysql-ok: nightly reaper\ncur.execute('DELETE FROM sessions')\n",
        vec![],
    );
    assert!(f.is_empty(), "{f:?}");
}

/// The SKIP axis is deliberately NOT widened with the marker axis, and this is the property that says
/// so. A `#`-commented-out statement is still text in the file; treating `#` as "commentary to ignore"
/// is the measured detection loss the module header records for secrets, and there is no reason to buy
/// it here either. Same `#`, same file, opposite answers — which is exactly why the two axes are two
/// functions.
#[test]
fn a_hash_commented_statement_still_fires_in_a_python_file() {
    let f = scan_pack(
        &marker_line_pack_py(),
        "svc.py",
        "# cur.execute('DELETE FROM sessions')\n",
        vec![],
    );
    assert_eq!(
        f.len(),
        1,
        "the marker axis learned `#`; the skip axis must not have: {f:?}"
    );
}

/// The `.env` branch matches a NAME, and a name test is where an over-eager prefix check hides. These
/// are `//`-comment files whose names merely START with `.env` — a `name[..4]` test took them for
/// dotenv files (and would have panicked on a name whose 4th byte split a UTF-8 char).
#[test]
fn a_name_merely_starting_with_dot_env_is_not_a_dotenv_file() {
    for rel in [".environment.ts", ".env-overrides.ts"] {
        let pack = rule_pack(
            r#"{"id":"config-secret","severity":"critical","message":"m","matcher":{"type":"line-scan","file_pattern":".","line_pattern":"TOKEN"}}"#,
        );
        let f = scan_pack(
            &pack,
            rel,
            "const SVC_TOKEN = 1; # zzop-config-secret-ok\n",
            vec![],
        );
        assert_eq!(
            f.len(),
            1,
            "`#` is not a comment leader in {rel}, so the marker must not suppress: {f:?}"
        );
    }
}
