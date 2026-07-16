//! Glob expansion + matching engine for `trees: "auto"` — pure move from `workspaces.rs` (300-line
//! source cap). Resolves positive workspace patterns segment-by-segment against real directories and
//! applies `!`-negations as whole-path filters; the census-pinned `SKIP_DIRS`/`MAX_GLOB_DEPTH`
//! constants stay in the parent module. See the `workspaces` module doc for the JS-parity mandate.

use std::collections::BTreeSet;
use std::path::Path;

use super::{MAX_GLOB_DEPTH, SKIP_DIRS};

/// Resolve workspace-package glob patterns to the set of relative directories that both match a
/// positive pattern, survive every `!`-negated pattern, and contain a `package.json`.
/// Deterministically sorted.
pub(super) fn resolve_workspace_dirs(base_dir: &Path, patterns: &[String]) -> Vec<String> {
    let mut positives: Vec<String> = Vec::new();
    let mut negatives: Vec<String> = Vec::new();
    for raw in patterns {
        let p = raw.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(neg) = p.strip_prefix('!') {
            negatives.push(neg.to_string());
        } else {
            positives.push(p.to_string());
        }
    }

    let mut matched: BTreeSet<String> = BTreeSet::new();
    for pattern in &positives {
        let segments = split_pattern(pattern);
        for rel in expand_segments(base_dir, "", &segments, 0) {
            if !rel.is_empty() {
                matched.insert(rel);
            }
        }
    }

    let neg_ops: Vec<Vec<GlobOp>> = negatives
        .iter()
        .map(|p| compile_negation_pattern(p))
        .collect();
    let mut kept: Vec<String> = Vec::new();
    for rel in &matched {
        if neg_ops.iter().any(|ops| glob_ops_match(ops, rel)) {
            continue;
        }
        if base_dir.join(rel).join("package.json").exists() {
            kept.push(rel.clone());
        }
    }
    kept.sort();
    kept
}

fn split_pattern(pattern: &str) -> Vec<String> {
    pattern
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .map(str::to_string)
        .collect()
}

/// List immediate subdirectory names of `abs_dir`, excluding `SKIP_DIRS`. Returns `[]` for a
/// missing/unreadable directory (a glob pattern pointing at a non-existent path simply matches
/// nothing). Does not follow symlinks (mirrors Node's `Dirent.isDirectory()`, which reflects the raw
/// directory-entry type, not the symlink target).
fn list_subdirs(abs_dir: &Path) -> Vec<String> {
    let entries = match std::fs::read_dir(abs_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        if !is_dir {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        dirs.push(name);
    }
    dirs
}

/// Expand one glob pattern's remaining `segments` from `current_rel` (a "/"-joined relative dir
/// under `base_dir`) into every matching relative directory path. `**` matches zero or more
/// directory levels. Depth-guarded by `MAX_GLOB_DEPTH`.
fn expand_segments(
    base_dir: &Path,
    current_rel: &str,
    segments: &[String],
    depth: u32,
) -> Vec<String> {
    if segments.is_empty() {
        return vec![current_rel.to_string()];
    }
    if depth > MAX_GLOB_DEPTH {
        return Vec::new();
    }

    let seg = &segments[0];
    let rest = &segments[1..];
    let current_abs = if current_rel.is_empty() {
        base_dir.to_path_buf()
    } else {
        base_dir.join(current_rel)
    };
    let mut results = Vec::new();

    if seg == "**" {
        // Zero levels: apply the rest right here. One-or-more: recurse into each subdir keeping `**`.
        results.extend(expand_segments(base_dir, current_rel, rest, depth));
        for sub in list_subdirs(&current_abs) {
            let next_rel = join_rel(current_rel, &sub);
            results.extend(expand_segments(base_dir, &next_rel, segments, depth + 1));
        }
        return results;
    }

    let is_wild = seg.contains('*') || seg.contains('?');
    for sub in list_subdirs(&current_abs) {
        let is_match = if is_wild {
            segment_matches(seg, &sub)
        } else {
            sub == seg.as_str()
        };
        if is_match {
            let next_rel = join_rel(current_rel, &sub);
            results.extend(expand_segments(base_dir, &next_rel, rest, depth + 1));
        }
    }
    results
}

fn join_rel(current_rel: &str, sub: &str) -> String {
    if current_rel.is_empty() {
        sub.to_string()
    } else {
        format!("{current_rel}/{sub}")
    }
}

// --- glob matching engine (shared by segment wildcards and whole-path negations) ------------
//
// A tiny hand-rolled matcher standing in for the JS source's `new RegExp(...)` calls: this crate has
// no regex dependency, and the grammar these patterns ever produce (literal chars, `[^/]*`, `[^/]`,
// and — for negations only — an optional `(?:/.*)?` group per interior `**` segment) is small enough
// to match directly without building real regex text. (The `.*` inside that optional group is
// modeled as "any char", not JS's "any char but a line terminator" — real directory names never
// contain newlines, so the two are equivalent for this use.)

enum GlobOp {
    Char(char),
    AnyNonSlash,
    AnyNonSlashRun,
    /// An interior `**` path segment: matches nothing, or a `/` followed by any run of characters
    /// (which may itself contain `/`). Mirrors `(?:/.*)?` in the JS source's whole-path regex.
    OptionalSlashAny,
}

fn compile_segment(seg: &str) -> Vec<GlobOp> {
    seg.chars()
        .map(|c| match c {
            '*' => GlobOp::AnyNonSlashRun,
            '?' => GlobOp::AnyNonSlash,
            other => GlobOp::Char(other),
        })
        .collect()
}

fn segment_matches(pattern: &str, name: &str) -> bool {
    glob_ops_match(&compile_segment(pattern), name)
}

/// Compiles a WHOLE glob path (may contain `/` and `**`) for negation matching. `**` matches across
/// directory separators only when it is an entire segment by itself; `*`/`?` do not cross `/`.
///
/// Faithfully reproduces a quirk of the ported JS `globToFullRegExp`: a **leading** `**` segment
/// (i.e. the pattern's very first segment) contributes nothing at all — including no separator — so
/// the segment that follows it still expects a literal leading `/` before its own content, which no
/// relative path ever has. A pattern like `!**/examples/**` therefore never matches anything; this
/// was verified against the JS source directly and is carried over as-is per the port mandate (see
/// module doc), not "fixed".
fn compile_negation_pattern(pattern: &str) -> Vec<GlobOp> {
    let segs = split_pattern(pattern);
    let mut ops: Vec<GlobOp> = Vec::new();
    for (i, seg) in segs.iter().enumerate() {
        if seg == "**" {
            if i > 0 {
                ops.push(GlobOp::OptionalSlashAny);
            }
        } else {
            if i > 0 {
                ops.push(GlobOp::Char('/'));
            }
            ops.extend(compile_segment(seg));
        }
    }
    ops
}

fn glob_ops_match(ops: &[GlobOp], candidate: &str) -> bool {
    let chars: Vec<char> = candidate.chars().collect();
    match_ops_at(ops, 0, &chars, 0)
}

fn match_ops_at(ops: &[GlobOp], oi: usize, s: &[char], si: usize) -> bool {
    if oi == ops.len() {
        return si == s.len();
    }
    match &ops[oi] {
        GlobOp::Char(c) => si < s.len() && s[si] == *c && match_ops_at(ops, oi + 1, s, si + 1),
        GlobOp::AnyNonSlash => si < s.len() && s[si] != '/' && match_ops_at(ops, oi + 1, s, si + 1),
        GlobOp::AnyNonSlashRun => {
            let mut end = si;
            while end < s.len() && s[end] != '/' {
                end += 1;
            }
            (si..=end).rev().any(|k| match_ops_at(ops, oi + 1, s, k))
        }
        GlobOp::OptionalSlashAny => {
            if match_ops_at(ops, oi + 1, s, si) {
                return true;
            }
            si < s.len()
                && s[si] == '/'
                && (si + 1..=s.len())
                    .rev()
                    .any(|k| match_ops_at(ops, oi + 1, s, k))
        }
    }
}
