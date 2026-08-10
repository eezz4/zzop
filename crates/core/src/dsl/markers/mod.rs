//! Shared helpers for suppress-marker compilation and matching, used by every matcher whose findings
//! carry an anchor line (`callers of RuleDef::suppress_marker` is the roster — do not re-list it here).
//! Author-written pattern fields compile through `diagnostics::RuleDiag` instead, so their failures are
//! reported rather than silently skipped; the markers here are DERIVED from the rule id, so they have no
//! author-facing field name to report and stay plain `Option` returns.
//!
//! ONE leader table, asked three different questions — which is why this is a directory:
//!
//! - [`path`] — per FILE. [`marker_leaders_for_path`] (which leaders may CARRY a marker: `//`, plus
//!   `--` in `.sql` and `#` in the config-file family) and [`leaders_for_path`] (which leaders make a
//!   line commentary to IGNORE, for `skip_comment_lines`). Those two deliberately DISAGREE about `#`;
//!   that module's own header carries the measurement and the reason.
//! - [`channel`] — per MATCHER KIND. [`MarkerChannel`] answers what a rule's suppression channel reads
//!   at all, and [`suppress_hint`] turns that into the sentence the engine appends to its findings.
//! - this file — the regex primitives both use, plus [`MarkerRegexes`], the one place that combines a
//!   file's marker leaders with the compiled regexes to answer "did a marker suppress this finding?".
//!
//! `line_scan` and `method_scan` used to answer that last question with their own local booleans, one
//! copy each. They no longer can.

mod channel;
mod path;

pub use channel::{marker_channel, suppress_hint, MarkerChannel};
pub use path::{leaders_for_path, marker_leaders_for_path, marker_widening_prose};

/// Builds the regex for the derived `RuleDef::suppress_marker()` (`zzop-<id>-ok`) — matches a `//` comment
/// naming the marker (regex-escaped; derived markers are `zzop-<kebab-id>-ok` with no metacharacters, so the
/// escape is defensive), optionally followed by `:` and free text.
pub(super) fn compile_marker(marker: &str, cache: &crate::dsl::RegexCache) -> Option<regex::Regex> {
    cache.compile(&format!(r"//\s*{}\b", regex::escape(marker)))
}

/// Builds the SQL-comment counterpart of `compile_marker` — matches a `--` comment naming the marker,
/// same escaping/suffix rules. Only ever consulted for `.sql` files (see `marker_leaders_for_path`, the
/// MARKER axis — the skip axis names `.sql` too, but this regex serves suppression); `--` is not a
/// comment marker in JS/TS (`--x` is a decrement there), so this regex must never be applied outside SQL.
pub(super) fn compile_marker_sql(
    marker: &str,
    cache: &crate::dsl::RegexCache,
) -> Option<regex::Regex> {
    cache.compile(&format!(r"--\s*{}\b", regex::escape(marker)))
}

/// Line-comment-neutral marker for the multi-language channels — those whose anchor lines can come from
/// any language at once, so a `//`-only regex would silently ignore half of them (callers of this fn are
/// the roster). Accepts `//` (TS/JS/Java/Go/C#) AND `#` (Python) comment leaders —
/// a `# zzop-protected-path-no-auth-evidence-ok` on a FastAPI route line suppresses exactly like `// zzop-protected-path-no-auth-evidence-ok` on an Express
/// one. `--` is deliberately NOT included (no `.sql` file produces route provides; see
/// `compile_marker_sql`'s isolation note). `#` cannot false-fire in JS/TS: a marker contains `-`, which
/// no `#private` field or hex literal continues with.
pub(super) fn compile_marker_line_comment(
    marker: &str,
    cache: &crate::dsl::RegexCache,
) -> Option<regex::Regex> {
    cache.compile(&format!(r"(?://|#)\s*{}\b", regex::escape(marker)))
}

/// Every marker regex a PER-FILE matcher may need, compiled once per rule — and the one place that
/// decides which of them a given file's leaders actually license.
///
/// `line_scan` and `method_scan` each used to compile the set themselves and then re-derive the choice
/// with their own `is_sql`/`is_hash` booleans. Two copies of "which leader applies here" is the defect
/// class this module exists to hold, and it was about to become three lines longer in each file when the
/// `#` family landed. Now the choice is made once and neither matcher can drift from the other.
pub(super) struct MarkerRegexes {
    slash: regex::Regex,
    sql: regex::Regex,
    hash: regex::Regex,
}

/// Compiles the trio. `None` means the marker itself is unusable as a regex — structural (it is derived
/// from the rule id), not an author-written pattern, so callers report it as a malformed RULE.
pub(super) fn compile_markers(
    marker: &str,
    cache: &crate::dsl::RegexCache,
) -> Option<MarkerRegexes> {
    Some(MarkerRegexes {
        slash: compile_marker(marker, cache)?,
        sql: compile_marker_sql(marker, cache)?,
        hash: compile_marker_line_comment(marker, cache)?,
    })
}

impl MarkerRegexes {
    /// Whether a marker comment on the finding's own line (or one above) suppresses it, under the
    /// file's MARKER leaders — [`marker_leaders_for_path`], never the `skip_comment_lines` set.
    ///
    /// `//` is always consulted, so both widenings are ADDITIVE: no marker that suppressed before this
    /// table grew a row stops suppressing because of one.
    pub(super) fn suppresses(&self, leaders: Leaders, lines: &[&str], line_idx: usize) -> bool {
        if marker_suppresses(&self.slash, lines, line_idx) {
            return true;
        }
        match leaders {
            Leaders::Slash => false,
            Leaders::SlashOrSql => marker_suppresses(&self.sql, lines, line_idx),
            Leaders::SlashOrHash => marker_suppresses(&self.hash, lines, line_idx),
        }
    }
}

/// How far above a finding a `// <marker>-ok` comment still suppresses it, one uniform window across every
/// rule. Set to 1: a wider window risks a marker aimed at one call silently suppressing unrelated sibling
/// findings a few lines below it. No fingerprint to bump: `zzop-engine`'s build script hashes this
/// crate into `DSL_INTERPRETER_FINGERPRINT`, so editing this file moves the cache key by itself.
const MARKER_LOOKBACK_LINES: usize = 1;

/// Whether the marker comment appears on the finding's own line or within `MARKER_LOOKBACK_LINES` above it.
pub(super) fn marker_suppresses(re: &regex::Regex, lines: &[&str], line_idx: usize) -> bool {
    (line_idx.saturating_sub(MARKER_LOOKBACK_LINES)..=line_idx)
        .any(|i| lines.get(i).is_some_and(|l| re.is_match(l)))
}

/// THE comment-leader table for these crates: which comment leaders a surface can carry. One owner,
/// three consumers — near-miss disclosure ([`near_miss_re`]), the `skip_comment_lines` line gate in
/// `line_scan`/`method_scan` (via [`is_comment_line`]), and `zzop-engine`'s generated-banner detector
/// (via [`strip_comment_leader`]). Each of those used to hard-code its own `//`/`*`/`/*` triple, and the
/// drift that produced was measured, not hypothetical: `sql/destructive-migration` pairs a `.sql`
/// `file_pattern` with `skip_comment_lines`, and a commented-out `-- DROP TABLE users;` fired as a
/// destructive migration.
///
/// Keyed by EXTENSION, not by language: `Language` lives in `zzop-engine`'s `dispatch`, which `zzop-core`
/// cannot see — and the extension is the right axis anyway, since a comment leader is a property of the
/// file's syntax rather than of the rule reading it.
///
/// Two axes feed this enum, and they are different questions:
/// - PER FILE — TWO functions, because the file axis answers two different questions:
///   [`marker_leaders_for_path`] (which leaders may carry a suppress marker) and [`leaders_for_path`]
///   (which make a line commentary to skip). They differ on the `#` family and that difference is
///   load-bearing — see `path.rs`'s header.
/// - PER CHANNEL — [`Leaders::SlashOrHash`], passed explicitly by the multi-language matchers
///   (`ir_scan`/`call_scan`/`literal_scan`), whose anchor line can come from any language at once, so no
///   single extension answers for it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Leaders {
    /// `//` only — every extension neither per-file lookup widens (see `compile_marker`).
    Slash,
    /// `//` or `--` — `.sql` (see `compile_marker_sql`). `--` must never leak outside `.sql`: `--x` is a
    /// decrement operator in JS/TS.
    SlashOrSql,
    /// `//` or `#` — the multi-language channels (see `compile_marker_line_comment`). `#` cannot
    /// false-fire in JS/TS: a marker carries a `-`, which no `#private` field or hex literal continues
    /// with.
    SlashOrHash,
}

impl Leaders {
    /// The LINE-comment leaders themselves — the single source both [`near_miss_re`]'s alternation and
    /// [`strip_comment_leader`] read, so a leader honored by one can never be missed by the other. Every
    /// entry is regex-metacharacter-free, which is what lets `near_miss_re` join them into an alternation
    /// with no escaping step.
    fn line_leaders(self) -> &'static [&'static str] {
        match self {
            Leaders::Slash => &["//"],
            Leaders::SlashOrSql => &["//", "--"],
            Leaders::SlashOrHash => &["//", "#"],
        }
    }
}

/// The comment BODY when `line`'s trimmed start opens a comment under `leaders`, else `None` — the one
/// primitive every comment-line consumer shares. `/*` is tried before `*` so the body is the text after
/// the opener rather than a stray `/`.
///
/// Block-comment forms (`/*`, and the ` * ` continuation this codebase, C-family sources and SQL all
/// write) are accepted for EVERY leader set, which is byte-for-byte what the C-family consumers did
/// before this table existed; the per-leader part is the LINE comment. Line-local by design: no
/// block-comment START/end state is tracked, so a `/* ... */` continuation line with no leading `*` is
/// not recognized — a documented residual gap (`rules/dsl/egress/localhost_egress.rs`), unchanged here.
pub fn strip_comment_leader(leaders: Leaders, line: &str) -> Option<&str> {
    let t = line.trim_start();
    leaders
        .line_leaders()
        .iter()
        .find_map(|p| t.strip_prefix(*p))
        .or_else(|| t.strip_prefix("/*"))
        .or_else(|| t.strip_prefix('*'))
}

/// Whether `line` is a comment line under `leaders` — the `skip_comment_lines` gate of `line_scan` and
/// `method_scan`. ONE function because the two matchers' semantics are identical: same trimmed-start,
/// line-local, no-block-state test, applied per line before any pattern runs. The only thing that ever
/// differed between them was nothing at all — they were two copies of the same three prefixes.
pub(super) fn is_comment_line(leaders: Leaders, line: &str) -> bool {
    strip_comment_leader(leaders, line).is_some()
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
/// have suppressed this finding must never be blamed for failing to — so `#` is recognized on the
/// multi-language channels AND, per file, inside the config-file family, and `--` only inside a `.sql`
/// file. Callers get the set from [`marker_leaders_for_path`], never from the skip axis.
fn near_miss_re(leaders: Leaders) -> &'static regex::Regex {
    static SLASH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SQL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static HASH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let cell = match leaders {
        Leaders::Slash => &SLASH,
        Leaders::SlashOrSql => &SQL,
        Leaders::SlashOrHash => &HASH,
    };
    cell.get_or_init(|| {
        // The alternation is DERIVED from `Leaders::line_leaders()` rather than spelled again here — that is
        // what makes "near-miss mirrors suppression" structural instead of a promise. No escaping step:
        // every leader in that table is regex-metacharacter-free (see its doc).
        let alt = leaders.line_leaders().join("|");
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
