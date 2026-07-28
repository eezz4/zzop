//! The per-commit half of the streaming parse: the in-flight commit accumulator ([`CommitCtx`]), the
//! `__C__`-prefixed header line's field split, and the flush that turns one finished commit into a
//! `CommitFileSet`. Split out of `parse.rs` (which keeps the per-FILE aggregation) when the subject
//! and its declared-pattern labels joined the commit record.
//!
//! **What "the subject" means here**: this module carries the header's subject field WHOLE — it never
//! shortens, normalizes or re-assembles it. It is not git's bytes, though: `parse_git_log` takes a
//! `&str`, and the only producer of that `&str` is `process::decode_git_output`, whose
//! `String::from_utf8_lossy` has already replaced every non-UTF-8 byte with U+FFFD (reachable on legacy
//! history whose commit objects carry no `encoding` header). So this module preserves what the decode
//! boundary handed it, not what `git log %s` wrote.
//!
//! **Memory shape**: one `String` per commit that reached a `CommitFileSet`. That is bounded by what the
//! parse ALREADY holds — `parse_git_log`'s `&str` input is the whole `git log --numstat` stdout, in which
//! every subject is already present once and the numstat lines dominate the byte count several times
//! over. So no cap is imposed here: a silent truncation would make "the subject, whole" a lie for exactly
//! the long subjects a declared pattern is most likely to be about, and this crate has no honest place to
//! disclose the cut (the collection carries no warning channel).

use zzop_core::CommitFileSet;

use crate::process::FIELD_SEP;
use crate::subject::SubjectMatchers;
use crate::tags::{extract_tags, CommitClassifiers};

/// The commit currently being accumulated — reset by each header line, drained by [`flush_commit`].
#[derive(Debug, Default)]
pub(super) struct CommitCtx {
    pub(super) sha: String,
    pub(super) date: String,
    pub(super) author: String,
    pub(super) tags: Vec<String>,
    pub(super) files: Vec<String>,
    /// The header's subject field (`%s`), kept whole and unmodified by this crate — `tags`/`labels` are
    /// both lossy derivations of it. Post-decode text, not git's bytes (see the module doc).
    subject: String,
    labels: Vec<String>,
}

/// Splits one `__C__<sha><SEP><date><SEP><author><SEP><subject>` header (the marker already stripped)
/// into `ctx`, deriving both subject-derived axes as it goes.
pub(super) fn parse_commit_header(
    rest: &str,
    classifiers: &CommitClassifiers,
    matchers: &SubjectMatchers,
    ctx: &mut CommitCtx,
) {
    let mut parts = rest.splitn(4, FIELD_SEP);
    ctx.sha = parts.next().unwrap_or("").to_string();
    ctx.date = parts.next().unwrap_or("").to_string();
    ctx.author = parts.next().unwrap_or("").to_string();
    let subject = parts.next().unwrap_or("");
    ctx.tags = extract_tags(subject, classifiers);
    // Two independent readings of the SAME bytes: `tags` is the commit-TYPE axis (bracket grammar, then
    // the caller's classifier table), `labels` is the DECLARED-pattern axis (empty unless the caller
    // declared a table). Neither suppresses the other, and neither can reconstruct the subject — which
    // is why the subject itself is now carried instead of being read and discarded here.
    ctx.labels = if matchers.is_empty() {
        Vec::new()
    } else {
        matchers.labels(subject)
    };
    ctx.subject = subject.to_string();
    ctx.files.clear();
}

/// Emits the accumulated commit (dropping one that never reached a parseable numstat line — a
/// binary-only or empty commit has no file set to contribute) and leaves `ctx` empty for the next one.
pub(super) fn flush_commit(ctx: &mut CommitCtx, commits: &mut Vec<CommitFileSet>) {
    if ctx.sha.is_empty() || ctx.files.is_empty() {
        return;
    }
    let date = std::mem::take(&mut ctx.date);
    let subject = std::mem::take(&mut ctx.subject);
    commits.push(CommitFileSet {
        sha: std::mem::take(&mut ctx.sha),
        files: std::mem::take(&mut ctx.files),
        tags: std::mem::take(&mut ctx.tags),
        date: if date.is_empty() { None } else { Some(date) },
        subject: if subject.is_empty() {
            None
        } else {
            Some(subject)
        },
        labels: std::mem::take(&mut ctx.labels),
    });
}
