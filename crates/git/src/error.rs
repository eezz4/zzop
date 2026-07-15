//! Typed git-invocation errors — collection never panics on a missing `git`, a path outside any
//! repository, or an otherwise-failing git invocation; all three surface here instead.

use std::fmt;

#[derive(Debug)]
pub enum GitError {
    /// The `git` executable could not be launched (not installed / not on `PATH`), or `repo` does not
    /// exist as a directory at all.
    GitUnavailable(String),
    /// `repo` exists but is not inside a git repository (no `.git`, or the path is outside it).
    NotAGitRepository { path: String, message: String },
    /// `git` ran but exited non-zero for a reason other than "not a repository" (bad `--since`, no
    /// such revision, etc.).
    CommandFailed { command: String, message: String },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::GitUnavailable(message) => write!(f, "git executable unavailable: {message}"),
            GitError::NotAGitRepository { path, message } => {
                write!(f, "\"{path}\" is not a git repository: {message}")
            }
            GitError::CommandFailed { command, message } => {
                write!(f, "`{command}` failed: {message}")
            }
        }
    }
}

impl std::error::Error for GitError {}
