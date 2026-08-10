//! Git process invocation — exactly one `std::process::Command` call: `git log --numstat` for the
//! whole history (`parse.rs` does all the parsing/aggregation). Never per-file or per-commit git
//! calls.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, OnceLock};

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

/// Spawns `git <-c overrides> <args>` in `repo`. Always passes two `-c` overrides ahead of the
/// subcommand, both pinning output paths against user config: `core.quotepath=false` (without it, git
/// octal-escapes and double-quotes any path containing non-ASCII bytes — e.g. `"\355\225\234..."`
/// instead of the real UTF-8 name — corrupting every downstream path key derived from
/// `git log --numstat`) and `diff.relative=false` (a user-level `diff.relative=true` makes numstat
/// paths cwd-relative and drops files outside the cwd, poisoning the run-shared memo — see the inline
/// comment). Applying them here — the single git-spawn choke point — rather than at the call site
/// keeps any future git invocation covered by default.
fn spawn_git(repo: &Path, args: &[String]) -> Result<Output, GitError> {
    if !repo.is_dir() {
        return Err(GitError::NotAGitRepository {
            path: repo.display().to_string(),
            message: "path does not exist or is not a directory".to_string(),
        });
    }
    record_spawn(repo);
    Command::new("git")
        .arg("-c")
        .arg("core.quotepath=false")
        // A user-level `diff.relative=true` makes `--numstat` emit cwd-relative paths AND silently
        // drop files outside the cwd — and the engine's git memo shares one collection across a
        // run's trees keyed by repo root, so one cwd-sensitive collection would poison trees 2..N.
        .arg("-c")
        .arg("diff.relative=false")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| GitError::GitUnavailable(e.to_string()))
}

/// Every `repo` argument this process has ever spawned git in, in call order. Append-only for the life
/// of the process — it exists so a test can assert an EQUALITY ("N trees sharing one repo collect
/// once") rather than a timing threshold, which is the only shape of perf regression gate this repo
/// accepts (no hand-picked numbers, no flakes).
///
/// Placed inside [`spawn_git`] rather than at any call site on purpose: this module's own doc asserts
/// it holds *exactly one* `std::process::Command` call, and `scripts/check-git-spawn-isolation.sh`
/// machine-checks that assertion — so the counter sitting at that single door counts BY
/// CONSTRUCTION. A future git invocation added anywhere in this crate is either routed through here
/// (and counted) or fails the guard.
fn spawn_record() -> &'static Mutex<Vec<PathBuf>> {
    static LOG: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Vec::new()))
}

fn record_spawn(repo: &Path) {
    if let Ok(mut log) = spawn_record().lock() {
        log.push(repo.to_path_buf());
    }
}

/// The repos git was spawned in so far, in call order. PROCESS-GLOBAL and never reset — a test
/// reading it must therefore be the only test in its binary (`cargo` runs tests in one file
/// concurrently), which is why the caller lives in a `tests/*.rs` of its own. Same constraint, same
/// reason as `crates/engine/tests/analyze_parse_census.rs`.
pub fn spawn_log() -> Vec<PathBuf> {
    spawn_record()
        .lock()
        .map(|log| log.clone())
        .unwrap_or_default()
}

/// The directory owning the git repository that `start` belongs to: the nearest ancestor (starting at
/// `start` itself) containing a `.git` entry, or `None` when there is none.
///
/// IN-PROCESS on purpose. The obvious spelling is `git rev-parse --show-toplevel`, but this function's
/// whole job is to let callers AVOID spawning git — resolving the key by spawning git would spend the
/// process it is meant to save, and would make the spawn-count gate above count its own denominator.
///
/// `.git` is accepted as a FILE as well as a directory (linked worktrees and submodules spell it as a
/// `gitdir:` pointer file). The pointer is deliberately NOT followed: two worktrees of one repository
/// can sit on different branches and therefore produce different `git log` output, so the directory
/// holding `.git` — not the shared object store it points at — is the correct identity for "whose
/// history is this".
pub fn repo_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
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
