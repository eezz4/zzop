//! The PER-FILE half of the comment-leader table — which leaders the file at a given path can carry.
//! The sibling [`super::channel`] is the per-MATCHER-KIND half; both are consulted for a suppression
//! decision, and neither answers for the other.
//!
//! # The per-file table, and why it is TWO functions
//!
//! This used to be one lookup answering both of the questions below, and its own doc recorded that the
//! `#`-leader family stayed unwired because "one row would decide two unrelated things at once ... which
//! of the two those authors meant is the RULES layer's call". That call has now been made, and the
//! answer is **both, in opposite directions** — which is exactly why one row could never carry it:
//!
//! - [`marker_leaders_for_path`] — *"is this line a comment that can carry my suppress marker?"* For a
//!   `.env`/`.yml`/`.toml`/`.ini`/`.conf`/`.cfg`/`.properties` file, `#` **must** count. `//` is not a
//!   comment in any of those formats; a dotenv reader or YAML parser sees a stray line. So a `//`-only
//!   marker table means such a file has NO working marker at all.
//! - [`leaders_for_path`] — *"is this line commentary I should ignore?"*, the `skip_comment_lines` gate.
//!   For a secret scanner, `#` must **not** count. A commented-out secret is still in the file, still in
//!   git history, and still one keystroke from being restored. **A secret is not less committed for
//!   having a `#` in front of it.**
//!
//! Measured, which is why the split is here rather than a wider one-line fix: with `#` added to the SKIP
//! set, a planted `# DATABASE_PASSWORD=<28 chars>` in `.env` goes silent — a detection loss on the
//! highest-severity pack, and `security/private-key-committed` / `security/vendor-token-committed` pair
//! the same `skip_comment_lines: true` with the same `#`-comment file types, so they would lose it too.
//!
//! What made this concrete: `security/config-file-secret` ends its message with
//! "``Suppress a vetted case with `# zzop-config-file-secret-ok`.``" and the engine read `//` only, so
//! the marker the finding named did nothing. Measured on a synthetic tree whose only matching file was
//! `.env`: the secret alone → 1 finding; with `# zzop-config-file-secret-ok` above it → still 1.
//! The author was right and the engine was wrong — `#` is what a comment IS in every file type that rule
//! matches, so the fix belongs here and not in the message.
//!
//! Split out of `markers/mod.rs` only for the repo per-file line cap
//! (`scripts/check-max-file-lines.sh`); one logical table across the three files.

use super::Leaders;

/// The SKIP axis — which leaders make a line "commentary to ignore" (`skip_comment_lines`), and the
/// leader set `zzop-engine`'s generated-banner detector reads. Only `.sql` is named; the `#` family is
/// deliberately NOT here (see the section above — that is the measured detection loss, not an oversight).
pub fn leaders_for_path(rel: &str) -> Leaders {
    match std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some(e) if e.eq_ignore_ascii_case("sql") => Leaders::SlashOrSql,
        _ => Leaders::Slash,
    }
}

/// Extensions whose line comment is `#`, and in which `//` is not a comment at all — the set
/// `security/config-file-secret`'s `file_pattern` accepts, which is the rule that proved the marker axis
/// needed them. Deliberately does NOT include `.py`/`.sh`/`.rb`: `#` is their comment leader too, but no
/// shipped rule directs a reader to a `#` marker there, and widening the set changes near-miss disclosure
/// for every Python line-scan finding. That is a real gap, recorded rather than silently closed — see
/// [`marker_leaders_for_path`].
const HASH_COMMENT_EXTENSIONS: [&str; 8] = [
    "properties",
    "yaml",
    "yml",
    "toml",
    "ini",
    "conf",
    "cfg",
    "env",
];

/// The per-file marker widenings, spelled for a human, DERIVED from the tables above rather than
/// written out — `zzop explain` prints this and it is the roster `docs/getting-started.md` tells a
/// reader to trust, so a hand-written copy would be one extension away from lying. It named `--` and
/// omitted `#` for exactly as long as it was hand-written.
///
/// Rule-independent by construction: it describes the FILE axis, and the caller supplies no file
/// (`explain` answers about a rule, which can match many). So it enumerates rather than resolves.
pub fn marker_widening_prose() -> String {
    let exts = HASH_COMMENT_EXTENSIONS
        .iter()
        .map(|e| format!("`.{e}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("also `--` inside a .sql file, and `#` inside a config file ({exts})")
}

/// The MARKER axis — which leaders may carry a suppress marker for the file at `rel`, case-insensitive.
///
/// `.sql` gets `--` (unchanged), the [`HASH_COMMENT_EXTENSIONS`] family gets `#`, everything else is
/// `//` only. Both widenings are ADDITIVE: `//` keeps working everywhere, so no marker that suppressed
/// before stops suppressing.
///
/// Near-miss disclosure reads this function, not [`leaders_for_path`], because near-miss must MIRROR
/// suppression exactly — a leader that can suppress must be blamable for failing to, and one that can
/// never suppress must never be blamed. That parity is the invariant; the two axes above are not.
///
/// KNOWN GAP, deliberately open: `.py` / `.sh` / `.rb` are `#`-comment languages that are not in the
/// family, so a line-scan finding in a `.py` file still honors `//` only — and `// zzop-x-ok` is a
/// SyntaxError in Python, meaning those findings have no writable marker. Nothing lies about it today
/// (no shipped rule names a `#` marker for them), which is why this is recorded instead of fixed inside
/// a change whose acceptance criterion was byte-identical messages. Closing it is one entry here plus a
/// re-judged `the_hash_leader_is_not_recognized_for_a_line_scan_finding`, whose current rationale
/// ("`#` never suppresses a line-scan finding") would become false.
pub fn marker_leaders_for_path(rel: &str) -> Leaders {
    let path = std::path::Path::new(rel);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if ext.eq_ignore_ascii_case("sql") {
            return Leaders::SlashOrSql;
        }
        if HASH_COMMENT_EXTENSIONS
            .iter()
            .any(|h| ext.eq_ignore_ascii_case(h))
        {
            return Leaders::SlashOrHash;
        }
    }
    // `.env` and its `.env.local` / `.env.production` siblings: `Path::extension` reads `.env` as a
    // hidden file with NO extension, and `.env.local` as extension `local`, so neither is reachable
    // through the branch above. The file NAME is the discriminator, matching the `(^|/)\.env(\.[\w.-]+)?$`
    // half of `security/config-file-secret`'s own `file_pattern`.
    //
    // EXACTLY `.env` or a `.env.`-prefixed sibling, never a `.env` PREFIX: a byte-slice test
    // (`name[..4]`) would take `.environment.ts` for a dotenv file — and would panic outright on a name
    // whose 4th byte is not a char boundary, which a UTF-8 filename can be.
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name == ".env" || name.starts_with(".env.") {
        return Leaders::SlashOrHash;
    }
    Leaders::Slash
}
