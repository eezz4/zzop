//! Git process invocation — exactly two `std::process::Command` calls total: one `git log --numstat`
//! for the whole history (`parse.rs` does all the parsing/aggregation), one `git rev-parse HEAD` for
//! the cache-key hash. Never per-file or per-commit git calls.

use std::path::Path;
use std::process::{Command, Output};

use crate::error::GitError;
use crate::CollectOptions;

/// Field separator between header components (`sha`, `date`, `author`, `subject`) — the ASCII Unit
/// Separator (0x1f) is vanishingly unlikely to appear in a commit subject.
pub(crate) const FIELD_SEP: char = '\u{1f}';
/// Prefix marking a commit-header line among numstat lines (must be distinct from any real path).
pub(crate) const COMMIT_MARKER: &str = "__C__";

/// THE decode boundary of this crate: every byte git writes to stdout becomes a Rust `String` here
/// and nowhere else. `from_utf8_lossy` is deliberate and stays — it can never fail, so a repo whose
/// output is not valid UTF-8 still gets collected instead of erroring the whole analysis. The price
/// is named rather than hidden: **each non-UTF-8 byte is replaced by U+FFFD**, so what reaches
/// `parse::parse_git_log` is the lossily-decoded text, not git's bytes. This is reachable — git
/// re-encodes a commit message only when the commit object carries an `encoding` header, so legacy
/// history written in latin-1 / Shift-JIS without one is emitted raw by `git log %s` and lands here
/// with its high bytes replaced. Everything downstream (subject preservation, declared-pattern
/// label matching) therefore operates on post-replacement text; `crate::tests` pins that boundary
/// directly, and the engine's inert-pattern warning discloses an observed U+FFFD to the user.
pub(crate) fn decode_git_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `git log --numstat` over the whole repo (no path/branch scoping — see lib.rs module doc for
/// why this crate always collects the full repo) and returns its raw stdout for `parse::parse_git_log`.
pub(crate) fn run_git_log(repo: &Path, opts: &CollectOptions) -> Result<String, GitError> {
    let format =
        format!("--pretty=format:{COMMIT_MARKER}%H{FIELD_SEP}%cI{FIELD_SEP}%ae{FIELD_SEP}%s");
    let mut args: Vec<String> = vec![
        "log".into(),
        "--no-merges".into(),
        "-M".into(),
        "--reverse".into(),
        "--numstat".into(),
        "--date=iso-strict".into(),
        format,
    ];
    if let Some(since) = &opts.since {
        args.push(format!("--since={since}"));
    }
    let output = spawn_git(repo, &args)?;
    if output.status.success() {
        return Ok(decode_git_output(&output.stdout));
    }
    let stderr = decode_git_output(&output.stderr);
    // A brand-new repo with no commits yet is a valid, empty history — not an error.
    if stderr.to_lowercase().contains("does not have any commits") {
        return Ok(String::new());
    }
    Err(classify_failure(repo, "git log", &args, &stderr))
}

/// Runs `git rev-parse HEAD` — the cache-key input (unchanged HEAD => history is unchanged, so a
/// consumer can skip re-collecting).
pub(crate) fn head_hash_impl(repo: &Path) -> Result<String, GitError> {
    let args: Vec<String> = vec!["rev-parse".into(), "HEAD".into()];
    let output = spawn_git(repo, &args)?;
    if output.status.success() {
        return Ok(decode_git_output(&output.stdout).trim().to_string());
    }
    let stderr = decode_git_output(&output.stderr);
    Err(classify_failure(repo, "git rev-parse HEAD", &args, &stderr))
}

/// Spawns `git <-c overrides> <args>` in `repo`. Always passes `-c core.quotepath=false` ahead of the
/// subcommand: without it, git octal-escapes and double-quotes any path containing non-ASCII bytes
/// (e.g. `"\355\225\234..."` instead of the real UTF-8 name) in its output, corrupting every downstream
/// path key derived from `git log --numstat`. `git rev-parse HEAD` never emits a path, so the flag is a
/// no-op there, but applying it uniformly here keeps this the single git-spawn choke point instead of
/// asking each call site to remember it.
fn spawn_git(repo: &Path, args: &[String]) -> Result<Output, GitError> {
    if !repo.is_dir() {
        return Err(GitError::NotAGitRepository {
            path: repo.display().to_string(),
            message: "path does not exist or is not a directory".to_string(),
        });
    }
    Command::new("git")
        .arg("-c")
        .arg("core.quotepath=false")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| GitError::GitUnavailable(e.to_string()))
}

fn classify_failure(repo: &Path, command: &str, args: &[String], stderr: &str) -> GitError {
    let lower = stderr.to_lowercase();
    if lower.contains("not a git repository") || lower.contains("outside repository") {
        GitError::NotAGitRepository {
            path: repo.display().to_string(),
            message: stderr.trim().to_string(),
        }
    } else {
        GitError::CommandFailed {
            command: format!("{command} {}", args.join(" ")),
            message: stderr.trim().to_string(),
        }
    }
}
