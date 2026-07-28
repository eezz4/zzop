//! Shared helpers for line-scan and method-scan evaluation: suppress-marker compilation and matching.
//! Author-written pattern fields compile through `diagnostics::RuleDiag` instead, so their failures are
//! reported rather than silently skipped; the markers here are DERIVED from the rule id, so they have no
//! author-facing field name to report and stay plain `Option` returns.

/// Builds the regex for the derived `RuleDef::suppress_marker()` (`zzop-<id>-ok`) — matches a `//` comment
/// naming the marker (regex-escaped; derived markers are `zzop-<kebab-id>-ok` with no metacharacters, so the
/// escape is defensive), optionally followed by `:` and free text.
pub(super) fn compile_marker(marker: &str) -> Option<regex::Regex> {
    regex::Regex::new(&format!(r"//\s*{}\b", regex::escape(marker))).ok()
}

/// Builds the SQL-comment counterpart of `compile_marker` — matches a `--` comment naming the marker,
/// same escaping/suffix rules. Only ever consulted for `.sql` files (see `is_sql_file`); `--` is not a
/// comment marker in JS/TS (`--x` is a decrement there), so this regex must never be applied outside SQL.
pub(super) fn compile_marker_sql(marker: &str) -> Option<regex::Regex> {
    regex::Regex::new(&format!(r"--\s*{}\b", regex::escape(marker))).ok()
}

/// Line-comment-neutral marker for the whole-tree io-scan pass, whose anchor lines span every language
/// an `http` provide can come from: accepts `//` (TS/JS/Java/Go/C#) AND `#` (Python) comment leaders —
/// a `# zzop-protected-path-no-auth-evidence-ok` on a FastAPI route line suppresses exactly like `// zzop-protected-path-no-auth-evidence-ok` on an Express
/// one. `--` is deliberately NOT included (no `.sql` file produces route provides; see
/// `compile_marker_sql`'s isolation note). `#` cannot false-fire in JS/TS: a marker contains `-`, which
/// no `#private` field or hex literal continues with.
pub(super) fn compile_marker_line_comment(marker: &str) -> Option<regex::Regex> {
    regex::Regex::new(&format!(r"(?://|#)\s*{}\b", regex::escape(marker))).ok()
}

/// Whether `--`-comment suppress markers should be recognized for this file — gated on the `.sql`
/// extension (case-insensitive) so `//`-only recognition stays byte-identical for every other extension.
pub(super) fn is_sql_file(rel: &str) -> bool {
    std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sql"))
}

/// How far above a finding a `// <marker>-ok` comment still suppresses it, one uniform window across every
/// rule. Set to 1: a wider window risks a marker aimed at one call silently suppressing unrelated sibling
/// findings a few lines below it. Bump `zzop-engine`'s `DSL_INTERPRETER_FINGERPRINT` when changing this.
const MARKER_LOOKBACK_LINES: usize = 1;

/// Whether the marker comment appears on the finding's own line or within `MARKER_LOOKBACK_LINES` above it.
pub(super) fn marker_suppresses(re: &regex::Regex, lines: &[&str], line_idx: usize) -> bool {
    (line_idx.saturating_sub(MARKER_LOOKBACK_LINES)..=line_idx)
        .any(|i| lines.get(i).is_some_and(|l| re.is_match(l)))
}

/// Which comment leaders a near-miss scan recognizes — mirrors whichever `compile_marker*` variant the
/// caller compiled its honored marker with, so disclosure and suppression read the same comment syntax.
#[derive(Clone, Copy)]
pub(super) enum Leaders {
    /// `//` only — line/method-scan outside `.sql` (see `compile_marker`).
    Slash,
    /// `//` or `--` — line/method-scan inside a `.sql` file (see `compile_marker_sql`).
    SlashOrSql,
    /// `//` or `#` — the whole-tree io-scan pass (see `compile_marker_line_comment`).
    SlashOrHash,
}

/// Regex for a token SHAPED like a zzop suppress marker, cached per leader set (the shape carries no rule
/// vocabulary at all — it is the same constant for every rule, which is why it can be a static).
///
/// Accepted shape, deliberately narrower than the honored-marker regex: a token over `[a-z0-9+]` with
/// `-`-joined segments (lowercase only, and `+` is in the alphabet so a `n+1`-style id is recognized)
/// ending in `-ok`, standing as the FIRST token of a line comment (leader-adjacent, whitespace only in
/// between), and terminated by an ATTACHED `:` or by the end of the line. That is exactly the documented
/// way a marker is written (`// <marker>-ok` or `// <marker>-ok: reason`).
///
/// What that buys and what it costs, precisely — the claim is NOT "prose is never accused":
/// - A `-ok` word INSIDE a sentence never fires (`// half-ok for now, revisit`): more words follow, so
///   neither terminator is reached. Nor does one with a capital (`// NOT-ok:`) or with any word before it
///   (`// TODO: not-ok`). Those are the realistic prose shapes, and they are pinned by tests.
/// - A comment that is ONLY a hyphenated lowercase `-ok` word (`// half-ok`) IS reported. By shape it is
///   indistinguishable from a bare marker, and a bare marker is a legal, documented spelling — there is
///   nothing left to discriminate on. Accepted cost.
/// - Conservative misses, both from the terminator: `// as-ok reason` (no colon) and `// as-ok : reason`
///   (detached colon) go unreported, even though the honored regex's `\b` would accept either spacing for
///   the RIGHT marker. Missing a disclosure is recoverable; accusing a correct comment is not.
///
/// Leaders are per-caller and mirror suppression EXACTLY (`Leaders`): a comment leader that could never
/// have suppressed this finding must never be blamed for failing to — so `#` is recognized only on the
/// io-scan path, and `--` only inside a `.sql` file.
fn near_miss_re(leaders: Leaders) -> &'static regex::Regex {
    static SLASH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SQL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static HASH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let (cell, alt) = match leaders {
        Leaders::Slash => (&SLASH, "//"),
        Leaders::SlashOrSql => (&SQL, "//|--"),
        Leaders::SlashOrHash => (&HASH, "//|#"),
    };
    cell.get_or_init(|| {
        regex::Regex::new(&format!(r"(?:{alt})\s*{NEAR_MISS_MARKER_TOKEN_PATTERN}"))
            .expect("near-miss marker shape is a compile-time constant regex")
    })
}

/// The marker-shaped TOKEN itself, leader-free — the capturing half of [`near_miss_re`], whose full
/// rationale (alphabet, `-ok` tail, attached-`:`-or-end-of-line terminator, and exactly which prose
/// shapes that accepts and rejects) is documented on that function.
///
/// Public because `rules-http`'s hand-authored `idempotent-ok` scanner is in another crate and must
/// report the SAME token shape, or one near-miss reads differently on the two surfaces. It used to
/// hand-copy this regex; exporting the shape closes that drift by construction. Callers prepend their
/// own leader alternation, which must mirror the leaders they actually honor (see [`Leaders`]) — a
/// leader that could never have suppressed must never be blamed for failing to.
pub const NEAR_MISS_MARKER_TOKEN_PATTERN: &str = r"([a-z0-9+]+(?:-[a-z0-9+]+)*-ok)(?::|\s*$)";

/// First marker-shaped token in the same lookback window `marker_suppresses` searches that is NOT this
/// rule's own honored marker. Deterministic: window lines in ascending order, matches left-to-right within
/// a line. Only ever consulted for a finding that is ABOUT TO BE EMITTED — the honored marker was already
/// checked and did not suppress — so this never sees a suppressed site.
fn near_miss_token(
    leaders: Leaders,
    honored: &str,
    lines: &[&str],
    line_idx: usize,
) -> Option<String> {
    let re = near_miss_re(leaders);
    (line_idx.saturating_sub(MARKER_LOOKBACK_LINES)..=line_idx)
        .filter_map(|i| lines.get(i).copied())
        .flat_map(|l| {
            re.captures_iter(l)
                .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
                .collect::<Vec<_>>()
        })
        .find(|t| t != honored)
}

/// `base` (the rule's own message) with one disclosure sentence appended when the finding's lookback
/// window carries a marker-shaped token that this rule does not honor — the author wrote a suppression
/// comment in good faith and it does nothing, so the finding says so instead of firing mutely. Naming BOTH
/// the token found and the marker actually honored is the whole point: neither alone lets the reader fix
/// the comment. Purely additive to the message; it changes no gate, so the set of findings is untouched.
pub(super) fn message_with_near_miss(
    leaders: Leaders,
    honored: &str,
    lines: &[&str],
    line_idx: usize,
    base: &str,
) -> String {
    match near_miss_token(leaders, honored, lines, line_idx) {
        Some(found) => format!(
            "{base} Note: a comment on this line (or the line directly above it) reads `{found}`, \
             which does not suppress this rule — the marker this rule honors is `{honored}`, so this \
             finding still fires."
        ),
        None => base.to_string(),
    }
}
