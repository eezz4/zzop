//! The PATH half of config-driven gating: does this config's `suppressions` / `global_excludes` cover
//! this file. Split out of `super` when that file crossed the 300-line cap, along the seam the surface
//! already has — `super` answers *"is this rule evaluated at all"* from ids alone, and this file answers
//! *"is this finding reported"* from paths. The two never call each other, which is why the cut is here
//! and not somewhere that would leave a helper stranded across it.
//!
//! Everything public is re-exported from `super`, so no caller learns the new path.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use regex::Regex;

use super::{GlobalExclude, RuleConfig, Suppression};

/// Shared substring-vs-glob path-filter semantics: `glob` takes precedence over `path` when both are set;
/// a filter with neither matches every file; an unparseable glob fails safe (matches nothing). Both
/// `suppression_matches_path` and `global_exclude_matches_path` are thin wrappers over this so the two
/// filter shapes (`Suppression`, `GlobalExclude`) never diverge in matching behavior.
fn path_filter_matches(glob: &Option<String>, path: &Option<String>, file: &str) -> bool {
    if let Some(glob) = glob {
        return glob_matches(glob, file);
    }
    match path {
        None => true,
        Some(path) => file.contains(path.as_str()),
    }
}

/// True if a finding for `rule` (optionally in `file`) is suppressed by `config.global_excludes` (a
/// rule-agnostic match drops the finding regardless of `rule`) OR `config.suppressions`: an entry matches
/// when its `rule` equals `rule` AND its path filter matches (`suppression_matches_path`). A
/// path/glob-qualified entry never matches a fileless finding. Multiple entries are OR-ed.
pub fn is_suppressed(config: &RuleConfig, rule: &str, file: Option<&str>) -> bool {
    if let Some(f) = file {
        if config
            .global_excludes
            .iter()
            .any(|entry| global_exclude_matches_path(entry, f))
        {
            return true;
        }
    }
    config.suppressions.iter().any(|entry| {
        if entry.rule != rule {
            return false;
        }
        match file {
            Some(f) => suppression_matches_path(entry, f),
            // A path/glob-qualified entry never matches a fileless finding; only a filter-less entry does.
            None => entry.glob.is_none() && entry.path.is_none(),
        }
    })
}

/// True when `suppression`'s path filter matches `file` (glob takes precedence over the substring
/// `path`; a suppression with neither filter matches every file). Shares the exact semantics
/// `is_suppressed` applies, exposed so a caller can detect a path/glob filter that matches no scanned
/// file (a likely typo — see the engine's unmatched-suppression warning).
pub fn suppression_matches_path(suppression: &Suppression, file: &str) -> bool {
    path_filter_matches(&suppression.glob, &suppression.path, file)
}

/// True when `exclude`'s path filter matches `file`. Same substring-vs-glob semantics as
/// `suppression_matches_path`, over a `GlobalExclude` instead of a `Suppression` — exposed so a caller can
/// detect a top-level `exclude` entry that matches no scanned file (the engine's unmatched-exclude
/// warning, mirroring `unmatched_suppression_warnings`).
///
/// One deliberate divergence from `Suppression`: a FILTER-LESS entry (`path`/`glob` both `None`) matches
/// NOTHING here, whereas a filter-less `Suppression` matches everything for its one rule. A filter-less
/// global exclude would silently drop EVERY finding of EVERY rule — a whole-run blast radius no one can
/// mean (the CLI never emits one; only a malformed raw addon request can). Treating it as match-nothing
/// also routes it into the unmatched-exclude warning instead of a silent total suppression.
pub fn global_exclude_matches_path(exclude: &GlobalExclude, file: &str) -> bool {
    if exclude.glob.is_none() && exclude.path.is_none() {
        return false;
    }
    path_filter_matches(&exclude.glob, &exclude.path, file)
}

/// Whether `file` matches shell-style `glob` (full-path anchored). Compiled globs are memoized — a config
/// has a handful of distinct patterns but `is_suppressed` runs per finding, so recompiling per call would
/// be wasteful. An unparseable pattern is cached as `None` and matches nothing.
///
/// The cache is process-lifetime and never evicted. That's bounded for a one-shot CLI/`analyze` call (a
/// config carries only a few globs); a long-lived addon host that analyzes many distinct configs over its
/// lifetime would accumulate distinct glob keys without bound — swap in an LRU/per-call cache if that ever
/// becomes a real embedding.
pub fn glob_matches(glob: &str, file: &str) -> bool {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Regex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    let compiled = map
        .entry(glob.to_string())
        .or_insert_with(|| Regex::new(&glob_to_regex(glob)).ok());
    compiled.as_ref().is_some_and(|re| re.is_match(file))
}

/// Translate a shell-style path glob to an anchored regex source. `**` spans `/` (a `**/` or `/**`
/// boundary also matches zero directories); `*` and `?` stay within a single path segment; `{a,b}`
/// alternates (nesting not supported); every other character is matched literally.
fn glob_to_regex(glob: &str) -> String {
    let bytes = glob.as_bytes();
    let mut re = String::from("^");
    let mut brace_depth: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'*' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    // `**` — spans path separators.
                    i += 1;
                    if bytes.get(i + 1) == Some(&b'/') {
                        // `**/` — also match zero leading directories.
                        re.push_str("(?:.*/)?");
                        i += 1;
                    } else if re.ends_with('/') {
                        // `/**` at end — also match zero trailing directories.
                        re.truncate(re.len() - 1);
                        re.push_str("(?:/.*)?");
                    } else {
                        re.push_str(".*");
                    }
                } else {
                    // `*` — within a single segment.
                    re.push_str("[^/]*");
                }
            }
            b'?' => re.push_str("[^/]"),
            b'{' => {
                brace_depth += 1;
                re.push_str("(?:");
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                re.push(')');
            }
            b',' if brace_depth > 0 => re.push('|'),
            // Escape every regex metacharacter so the remaining glob text is matched literally.
            c => {
                let ch = c as char;
                if "\\.+()|[]^$".contains(ch) {
                    re.push('\\');
                }
                re.push(ch);
            }
        }
        i += 1;
    }
    re.push('$');
    re
}
