//! Pure `git log --numstat` output parser — the single source of both per-file `GitStats` (zzop_core)
//! and per-commit `CommitFileSet`s (zzop_core), built in one streaming pass: same commit-header shape,
//! numstat line shape, rename (`"old => new"` / `"{old => new}"`) handling, and `--reverse` traversal
//! (oldest-first) so a rename's pre-rename contributions — already accumulated under the old path —
//! can be folded into the new canonical path the moment the rename is seen.
//!
//! Notable design choices:
//! - No path-prefix stripping, extension filter, decay half-life, or branch selection — this crate
//!   collects the WHOLE repo's history in one pass; a caller narrows/filters downstream if needed.
//! - The recent window defaults to 30 days (`CollectOptions::recent_days`, shared with
//!   `zzop_core::node::DEFAULT_RECENT_THRESHOLD_DAYS`, the lifecycle classifier's own recency window)
//!   so a file's "recent" churn lines up with the same window the lifecycle classifier uses.
//! - `recentChurn`/`recentChangeCount` are always populated (`Some`), since this crate always computes
//!   them; `GitPathStats`'s `Option`-ness instead marks fields that may genuinely be absent when
//!   unknown (e.g. no commits at all for a path).
//! - `lastModified` and the `GitWindow` first/last dates use plain ISO-string comparison rather than
//!   parsing to a numeric timestamp: ISO-8601 date strings sort lexicographically in the same order as
//!   chronologically, as long as every compared string shares the same UTC-offset notation, so string
//!   min/max is a cheap correct comparison here. The *recency cutoff* instead parses to real epoch
//!   milliseconds (`iso_date.rs`, offset-aware) because it's a numeric threshold comparison against
//!   `now_ms - recent_days`, not a min/max over a fixed set of already-comparable strings.

use std::collections::BTreeMap;

use zzop_core::{CommitFileSet, GitPathStats, GitStats};

use crate::iso_date::parse_iso_to_ms;
use crate::process::{COMMIT_MARKER, FIELD_SEP};
use crate::tags::{extract_tags, CommitClassifiers};
use crate::{CollectOptions, GitCollection, GitWindow};

const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

/// Parses `git log --numstat` output (this crate's format — see `process::run_git_log`) into a
/// [`GitCollection`]. Pure: no git or filesystem access, so it is fully testable against canned text.
/// `now_ms` (epoch milliseconds) drives the recent-window boundary.
pub fn parse_git_log(output: &str, opts: &CollectOptions, now_ms: i64) -> GitCollection {
    let classifiers = CommitClassifiers::compile(&opts.commit_type_patterns);
    let recent_cutoff_ms = now_ms - i64::from(opts.recent_days) * MS_PER_DAY;

    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
    let mut commits: Vec<CommitFileSet> = Vec::new();
    let mut ctx = CommitCtx::default();

    for line in output.split('\n') {
        if let Some(rest) = line.strip_prefix(COMMIT_MARKER) {
            flush_commit(&mut ctx, &mut commits);
            parse_commit_header(rest, &classifiers, &mut ctx);
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        process_numstat_line(line, &mut ctx, &mut acc, recent_cutoff_ms);
    }
    flush_commit(&mut ctx, &mut commits);

    GitCollection {
        stats: build_stats(acc),
        window: build_window(&commits, opts.since.clone()),
        commits,
    }
}

/// Per-canonical-path accumulator, keyed by canonical path while a commit's numstat lines stream by.
#[derive(Debug, Default)]
struct Acc {
    change_count: u32,
    churn: u32,
    last_modified: String,
    authors: std::collections::BTreeSet<String>,
    author_commits: BTreeMap<String, u32>,
    /// (author -> commit count) within the recent window — for ownership-shift comparison.
    recent_author_commits: BTreeMap<String, u32>,
    tag_counts: BTreeMap<String, u32>,
    aliases: std::collections::BTreeSet<String>,
    recent_churn: u32,
    recent_change_count: u32,
}

impl Acc {
    fn new(date: String) -> Self {
        Acc {
            last_modified: date,
            ..Default::default()
        }
    }
}

#[derive(Debug, Default)]
struct CommitCtx {
    sha: String,
    date: String,
    author: String,
    tags: Vec<String>,
    files: Vec<String>,
}

fn parse_commit_header(rest: &str, classifiers: &CommitClassifiers, ctx: &mut CommitCtx) {
    let mut parts = rest.splitn(4, FIELD_SEP);
    ctx.sha = parts.next().unwrap_or("").to_string();
    ctx.date = parts.next().unwrap_or("").to_string();
    ctx.author = parts.next().unwrap_or("").to_string();
    let subject = parts.next().unwrap_or("");
    ctx.tags = extract_tags(subject, classifiers);
    ctx.files.clear();
}

fn flush_commit(ctx: &mut CommitCtx, commits: &mut Vec<CommitFileSet>) {
    if ctx.sha.is_empty() || ctx.files.is_empty() {
        return;
    }
    let date = std::mem::take(&mut ctx.date);
    commits.push(CommitFileSet {
        sha: std::mem::take(&mut ctx.sha),
        files: std::mem::take(&mut ctx.files),
        tags: std::mem::take(&mut ctx.tags),
        date: if date.is_empty() { None } else { Some(date) },
    });
}

fn process_numstat_line(
    line: &str,
    ctx: &mut CommitCtx,
    acc: &mut BTreeMap<String, Acc>,
    recent_cutoff_ms: i64,
) {
    let mut parts = line.splitn(3, '\t');
    let add_str = parts.next().unwrap_or("");
    let del_str = parts.next().unwrap_or("");
    let file_path = parts.next().unwrap_or("");
    let added: Option<u32> = add_str.parse().ok();
    let deleted: Option<u32> = del_str.parse().ok();
    // Binary files report `-\t-\tpath` — both unparseable. Skip entirely: no churn/changeCount signal,
    // and no membership in the commit's file set.
    if added.is_none() && deleted.is_none() {
        return;
    }
    let (canonical, alias) = parse_path(file_path);
    ctx.files.push(canonical.clone());
    let line_delta = added.unwrap_or(0) + deleted.unwrap_or(0);
    let recent = is_recent(&ctx.date, recent_cutoff_ms);

    acc.entry(canonical.clone())
        .or_insert_with(|| Acc::new(ctx.date.clone()));
    {
        let cur = acc.get_mut(&canonical).expect("entry just inserted above");
        cur.change_count += 1;
        cur.churn += line_delta;
        if recent {
            cur.recent_change_count += 1;
            cur.recent_churn += line_delta;
        }
        if ctx.date > cur.last_modified {
            cur.last_modified = ctx.date.clone();
        }
        apply_author(cur, &ctx.author, recent);
        for tag in &ctx.tags {
            *cur.tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
        if let Some(a) = &alias {
            cur.aliases.insert(a.clone());
        }
    }
    if let Some(alias) = alias {
        merge_alias(acc, &alias, &canonical);
    }
}

fn apply_author(cur: &mut Acc, author: &str, recent: bool) {
    if author.is_empty() {
        return;
    }
    cur.authors.insert(author.to_string());
    *cur.author_commits.entry(author.to_string()).or_insert(0) += 1;
    if recent {
        *cur.recent_author_commits
            .entry(author.to_string())
            .or_insert(0) += 1;
    }
}

/// Absorbs the old-path entry into the new-path (canonical) entry on a rename commit — requires
/// `--reverse` (oldest-first) traversal so the old path's stats are already accumulated by the time
/// the rename commit is reached.
fn merge_alias(acc: &mut BTreeMap<String, Acc>, alias: &str, canonical: &str) {
    let Some(old) = acc.remove(alias) else {
        return;
    };
    let cur = acc
        .get_mut(canonical)
        .expect("canonical entry inserted before merge_alias is called");
    cur.change_count += old.change_count;
    cur.churn += old.churn;
    for a in old.authors {
        cur.authors.insert(a);
    }
    for (author, n) in old.author_commits {
        *cur.author_commits.entry(author).or_insert(0) += n;
    }
    for (author, n) in old.recent_author_commits {
        *cur.recent_author_commits.entry(author).or_insert(0) += n;
    }
    for (tag, n) in old.tag_counts {
        *cur.tag_counts.entry(tag).or_insert(0) += n;
    }
    for old_alias in old.aliases {
        cur.aliases.insert(old_alias);
    }
    if old.last_modified > cur.last_modified {
        cur.last_modified = old.last_modified;
    }
}

fn is_recent(date: &str, cutoff_ms: i64) -> bool {
    match parse_iso_to_ms(date) {
        Some(ms) => ms >= cutoff_ms,
        None => false,
    }
}

/// Handles the `git --numstat -M` rename path spellings: a top-level `"old => new"` rename, and the
/// common-prefix-optimized `"{old => new}"` fragment inside an otherwise-shared path (e.g.
/// `"src/{old.ts => new.ts}"` or `"{a => b}/file.ts"`). Only ONE `{...}` fragment per path is
/// handled — git-numstat never emits more than one.
fn parse_path(raw: &str) -> (String, Option<String>) {
    if let Some(brace_start) = raw.rfind('{') {
        if let Some(arrow_rel) = raw[brace_start..].find(" => ") {
            let arrow_at = brace_start + arrow_rel;
            if let Some(close_rel) = raw[arrow_at..].find('}') {
                let close_at = arrow_at + close_rel;
                let pre = &raw[..brace_start];
                let old_mid = &raw[brace_start + 1..arrow_at];
                let new_mid = &raw[arrow_at + 4..close_at];
                let post = &raw[close_at + 1..];
                let old_p = collapse_slashes(&format!("{pre}{old_mid}{post}"));
                let new_p = collapse_slashes(&format!("{pre}{new_mid}{post}"));
                return (new_p, Some(old_p));
            }
        }
    }
    if let Some(idx) = raw.find(" => ") {
        let old_p = raw[..idx].to_string();
        let new_p = raw[idx + 4..].to_string();
        return (new_p, Some(old_p));
    }
    (raw.to_string(), None)
}

/// Mirrors `.replace(/\/{2,}/g, "/")` — the brace-splice can produce a doubled slash at the junction
/// (e.g. `pre` ending in `/` and `post` starting with `/` when the old/new segment is empty).
fn collapse_slashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_slash = false;
    for c in s.chars() {
        if c == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        out.push(c);
    }
    out
}

fn build_stats(acc: BTreeMap<String, Acc>) -> GitStats {
    let mut by_path = BTreeMap::new();
    let mut alias_to_canonical = BTreeMap::new();
    for (path, a) in acc {
        for alias in &a.aliases {
            alias_to_canonical.insert(alias.clone(), path.clone());
        }
        by_path.insert(
            path,
            GitPathStats {
                change_count: a.change_count,
                churn: a.churn,
                last_modified: if a.last_modified.is_empty() {
                    None
                } else {
                    Some(a.last_modified)
                },
                author_count: a.authors.len() as u32,
                tag_counts: a.tag_counts.into_iter().collect(),
                recent_churn: Some(a.recent_churn),
                recent_change_count: Some(a.recent_change_count),
                author_commits: Some(a.author_commits.into_iter().collect()),
                recent_author_commits: if a.recent_author_commits.is_empty() {
                    None
                } else {
                    Some(a.recent_author_commits.into_iter().collect())
                },
            },
        );
    }
    GitStats {
        by_path,
        alias_to_canonical,
    }
}

/// Derives the covered window from the parsed commits (dates are ISO, so string min/max is
/// chronological under a consistent UTC-offset notation).
fn build_window(commits: &[CommitFileSet], since: Option<String>) -> GitWindow {
    let dates: Vec<&str> = commits.iter().filter_map(|c| c.date.as_deref()).collect();
    if dates.is_empty() {
        return GitWindow {
            since,
            first: None,
            last: None,
            commits: commits.len(),
        };
    }
    let mut min = dates[0];
    let mut max = dates[0];
    for &d in &dates {
        if d < min {
            min = d;
        }
        if d > max {
            max = d;
        }
    }
    GitWindow {
        since,
        first: Some(min.to_string()),
        last: Some(max.to_string()),
        commits: commits.len(),
    }
}
