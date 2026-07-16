//! Shared helpers for line-scan and method-scan evaluation: suppress-marker compilation/matching and
//! `require_file_all`/`require_file_absent` compilation.

/// Builds the regex for `RuleDef::suppress_marker` — matches a `//` comment naming the marker (regex-escaped
/// so metacharacters like `n+1-ok`'s `+` match literally), optionally followed by `:` and free text.
pub(super) fn compile_marker(marker: &str) -> Option<regex::Regex> {
    regex::Regex::new(&format!(r"//\s*{}\b", regex::escape(marker))).ok()
}

/// Builds the SQL-comment counterpart of `compile_marker` — matches a `--` comment naming the marker,
/// same escaping/suffix rules. Only ever consulted for `.sql` files (see `is_sql_file`); `--` is not a
/// comment marker in JS/TS (`--x` is a decrement there), so this regex must never be applied outside SQL.
pub(super) fn compile_marker_sql(marker: &str) -> Option<regex::Regex> {
    regex::Regex::new(&format!(r"--\s*{}\b", regex::escape(marker))).ok()
}

/// Whether `--`-comment suppress markers should be recognized for this file — gated on the `.sql`
/// extension (case-insensitive) so `//`-only recognition stays byte-identical for every other extension.
pub(super) fn is_sql_file(rel: &str) -> bool {
    std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sql"))
}

/// Compiles `require_file_all`. `None` when any pattern fails to compile, skipping the whole rule.
pub(super) fn compile_require_all(patterns: &[String]) -> Option<Vec<regex::Regex>> {
    patterns.iter().map(|p| regex::Regex::new(p).ok()).collect()
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
