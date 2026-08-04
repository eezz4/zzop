//! Bakes ONE fact into the `zzop-mcp` binary that it cannot compute at run time: **when the source
//! this binary was built from was committed**. `src/staleness.rs` turns it into the "this build is
//! old" self-report — the only update-notification channel the manually installed Claude Desktop
//! (`.mcpb`) lane has, under two constraints the self-report must never break: the binary makes no
//! network call, and it never claims a newer release exists. `src/staleness.rs` owns both.
//!
//! ## The SOURCE's date, not the BUILD's — and the honest choice is the reproducible one
//! The question the self-report answers is "how old is the RELEASE I am running", and a fresh
//! recompile of a year-old tree is a year-old release. A wall-clock `SystemTime::now()` stamp answers a
//! different question ("when did the compiler run") and answers THIS one wrong in the direction that
//! matters: it reports fresh for exactly the build that most deserves the notice. It is also
//! nondeterministic — identical source, a different value on every build — which is the property that
//! makes a value dangerous anywhere near a cache key.
//!
//! Rejected beside it:
//! - **the release TAG's date** — the same instant as the commit's in practice (the auto-tag lane in
//!   `.github/workflows/prebuild.yml` creates `vX.Y.Z` from the version-bump commit), but it does not
//!   exist at all on a non-release build and the release checkout is depth-1 without tags. It would be
//!   absent exactly where the commit date is identical to it, and add a lookup to gain nothing.
//! - **the commit HASH** — an identity with no time axis. "How old" is not computable from it.
//!
//! ## Two sources, in this order
//! 1. `SOURCE_DATE_EPOCH` — the cross-ecosystem reproducible-builds convention. A packager who sets it
//!    is stating the source date authoritatively, so honoring it is correct on its own terms; it is
//!    also what makes the self-report verifiable end to end without fabricating git history. Honored
//!    WITH a plausibility floor: a stamp from before this project's first commit (nix stdenv exports
//!    1980 into every derivation as a determinism placeholder) is not a statement about THIS source,
//!    and falls through to source 2. The floor's derivation, its no-git fallback rung, and why
//!    over-rejection is the safe direction all live in `src/stamp_floor.rs` — shared with the lib so
//!    tests can pin the decision.
//! 2. `git log -1 --format=%ct` over the workspace — `HEAD`'s committer date. Spawning `git` at build
//!    time adds no dependency and no license inventory entry; `crates/git` already reads this repo's
//!    history the same way at RUN time.
//!
//! Neither available (a source tarball with no `.git`, or no `git` on `PATH`): the generated constant
//! is `None` and the self-report stays SILENT. It never guesses an age it cannot support.
//!
//! Known and bounded: an uncommitted working tree still reports `HEAD`'s date, so a developer's local
//! build reads as old as its last commit rather than as old as its files. Against a threshold measured
//! in months (`staleness::STALE_AFTER_DAYS`) that difference is noise.
//!
//! ## Why it is baked in THIS package, and why nothing here can reach a cache key
//! `crates/engine/build.rs` hashes `Cargo.lock` into `FP_ENGINE`, and `FP_ENGINE` suffixes EVERY arm of
//! `cache::parser_fingerprint` — so adding a dependency EDGE anywhere in this workspace invalidates
//! every cache entry once. Reading a stamp baked in `crates/facade` from here would need exactly that
//! edge (`zzop-mcp` depends on `zzop-summary` and `serde_json`, nothing else). Baking it here adds no
//! dependency, so `Cargo.lock` does not move and no fingerprint does either. Nothing under
//! `packages/` is a subject of any fingerprint to begin with: `crates/engine/build.rs`'s subjects are
//! the eight parser crates, `rules-schema`, `crates/core`, `crates/cache`, `crates/core/src/dsl`, and
//! `crates/engine`'s own `src/`.
//!
//! The other candidate home, `zzop_facade::version_string()`, is closed for a second measured reason:
//! that string is copied VERBATIM into analysis output's `tool` field, so a per-build value there would
//! make the engine's own output differ between two builds of the same source.

use std::path::{Path, PathBuf};
use std::process::Command;

// The pure half of the SOURCE_DATE_EPOCH plausibility decision, shared with the lib (which compiles
// it only so the tests beside `staleness.rs` can pin it — a build script has no test harness).
#[path = "src/stamp_floor.rs"]
mod stamp_floor;

fn main() {
    // Without these declarations Cargo would keep serving the stamp from whenever this package last
    // happened to rebuild for an unrelated reason, and the reported age would drift OLDER than the
    // truth — the over-claiming direction.
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("packages/mcp sits two levels under the workspace root")
        .to_path_buf();
    register_head_rerun(&workspace);

    let stamp = match source_commit_epoch(&workspace) {
        Some(epoch) => format!("Some({epoch})"),
        None => "None".to_string(),
    };
    let out = format!(
        "// Generated by build.rs — do not edit. Unix seconds of the committer date of the source\n\
         // HEAD this binary was built from, or `None` when the build could not learn it (no `.git`,\n\
         // no `git` on PATH, and no USABLE SOURCE_DATE_EPOCH — a placeholder stamp from before this\n\
         // project's history is rejected). See build.rs for why it is the SOURCE's date.\n\
         const SOURCE_COMMIT_EPOCH: Option<i64> = {stamp};\n"
    );
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("build_stamp.rs");
    std::fs::write(&out_path, out).expect("write build_stamp.rs");
}

/// Declares the git files whose change moves `HEAD`'s commit date: `.git/HEAD`, the ref it names (an
/// ordinary branch checkout), and `.git/packed-refs` (where that ref's tip lives after a `git gc` or in
/// a fresh clone). Declaring a path that does not exist is legal — Cargo reads it as "rerun if it
/// appears". A `.git` FILE rather than a directory (a worktree or submodule) is not followed: no rerun
/// is declared and the stamp is whatever the previous build produced, so the degrade is a date that
/// lags, never a build that is wrong.
fn register_head_rerun(workspace: &Path) {
    let git_dir = workspace.join(".git");
    if !git_dir.is_dir() {
        return;
    }
    let head = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );
    let Ok(text) = std::fs::read_to_string(&head) else {
        return;
    };
    if let Some(reference) = text.trim().strip_prefix("ref: ") {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(reference).display()
        );
    }
}

/// `SOURCE_DATE_EPOCH` if set AND plausible, else `HEAD`'s committer date, else `None`. See the
/// module doc for the ordering's rationale and for what `None` costs (silence, never a guess).
fn source_commit_epoch(workspace: &Path) -> Option<i64> {
    if let Ok(raw) = std::env::var("SOURCE_DATE_EPOCH") {
        // An unparseable override is a mistake in the build environment, and falling back to git here
        // would answer a question nobody asked while looking like it honored the one they did.
        let parsed = raw.trim().parse::<i64>().unwrap_or_else(|e| {
            panic!("SOURCE_DATE_EPOCH is {raw:?}, which is not an integer count of seconds: {e}")
        });
        // A stamp from before this project began is a different animal from an unparseable one: not
        // a mistake in this build's configuration but an environment-wide determinism placeholder
        // (nix stdenv exports 1980-01-01 into every derivation). It answers "give me any fixed
        // date", not "when was this source committed" — so the honest move is decline-and-fall-back,
        // not panic (the packager configured nothing wrong) and not acceptance (the self-report
        // would claim a ~17,000-day age nobody measured). The floor itself, its no-git rung, and
        // why over-rejection is the safe direction live in `src/stamp_floor.rs`.
        if stamp_floor::source_date_epoch_is_plausible(parsed, first_commit_epoch(workspace)) {
            return Some(parsed);
        }
        println!(
            "cargo:warning=SOURCE_DATE_EPOCH ({parsed}) predates this project's history; treating \
             it as a reproducible-build placeholder, not a source date — using git instead"
        );
    }
    head_commit_epoch(workspace)
}

/// One git invocation, stdout as UTF-8 on success. `None` for a missing `git`, a failed command (not
/// a repo, shallow oddities), or undecodable output — every caller's next rung is "git could not
/// answer this", never a guess.
fn git_stdout(workspace: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// The floor's derived rung: the project's first commit — `--max-parents=0`, taking the earliest of
/// several roots if history was ever stitched. In a shallow checkout the parentless commit git sees
/// is the shallow BOUNDARY, newer than the true root, so this rung errs only toward over-rejection —
/// and a genuine stamp it over-rejects degrades to [`head_commit_epoch`], the value a genuine stamp
/// carries anyway. `None` (no git at all) does NOT skip the check: `src/stamp_floor.rs` owns why the
/// no-git environments are exactly the placeholder-injecting ones.
fn first_commit_epoch(workspace: &Path) -> Option<i64> {
    let stdout = git_stdout(workspace, &["log", "--max-parents=0", "--format=%ct"])?;
    stamp_floor::earliest_root_epoch(&stdout)
}

/// Source 2 of the module doc: `HEAD`'s committer date.
fn head_commit_epoch(workspace: &Path) -> Option<i64> {
    git_stdout(workspace, &["log", "-1", "--format=%ct"])?
        .trim()
        .parse()
        .ok()
}
