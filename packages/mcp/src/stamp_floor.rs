//! The `SOURCE_DATE_EPOCH` plausibility floor — the pure half of a decision `build.rs` makes at
//! build time. It lives in the lib tree because a build script has no test harness of its own:
//! `build.rs` pulls this file in with `#[path]`, and the tests below pin it from the lib's harness,
//! beside `staleness.rs`'s.
//!
//! ## The failure this closes
//! `build.rs` honors `SOURCE_DATE_EPOCH` as the authoritative source date, but several build
//! environments export it as a generic *determinism placeholder*, not as a statement about the
//! source: nix stdenv sets `315532800` (1980-01-01, the ZIP-format minimum) in every derivation.
//! Parsing alone accepts that, and the binary then self-reports as a ~17,000-day-old build — an age
//! nobody measured, in exactly the false-"you are behind" direction `staleness.rs` says is the
//! fastest way to make its one channel ignorable.
//!
//! ## The floor's two rungs
//! A stamp older than this project's FIRST commit cannot be this source's date. So rung one: the
//! floor is DERIVED — `git log --max-parents=0 --format=%ct`, parsed by [`earliest_root_epoch`] —
//! never a hardcoded project birthdate, which would be one more copy of a fact git already owns.
//! When that derivation fails (no `.git`, no `git` on `PATH`), the check is NOT skipped: the
//! environments that inject placeholder epochs (nix sandboxes) are exactly the ones that strip
//! `.git`, so "no git, no check" would leave the motivating bug unfixed precisely where it occurs.
//! Rung two is [`FLOOR_WHEN_HISTORY_UNKNOWN`] — a date before git itself existed. Unlike a project
//! birthdate it can never drift (git's beginning is immortal), and it still rejects every known
//! placeholder value (`0`, `1`, `315532800`).
//!
//! ## Why over-rejection is the safe direction
//! In a shallow checkout the derived "first commit" is the shallow BOUNDARY commit — newer than the
//! true root — so the derived floor errs only high, never low. A genuine stamp rejected by a
//! too-high floor degrades to `build.rs`'s git fallback (`HEAD`'s committer date, the value a
//! genuine stamp carries anyway) or, with no git at all, to `None` and therefore silence. Every rung
//! ends somewhere honest; accepting a placeholder is the one outcome with no honest reading, and no
//! rung can produce it.

/// The fallback floor when the project's own first commit could not be derived: 2005-04-07, the
/// committer date of git's initial commit. No genuine committer date in ANY git repository can
/// precede the existence of git, so a stamp below this is a placeholder in every project — which is
/// what lets this rung survive being a constant where a project birthdate could not (see the module
/// doc's "two rungs").
pub(crate) const FLOOR_WHEN_HISTORY_UNKNOWN: i64 = 1_112_911_993;

/// Whether a parsed `SOURCE_DATE_EPOCH` is plausible as THIS source's date: at or after the
/// project's first commit when history could be read, at or after git's own beginning when it could
/// not. Equality is accepted on both rungs — building the first commit itself is legitimate, and the
/// derived floor IS a real commit's date.
pub(crate) fn source_date_epoch_is_plausible(stamp: i64, first_commit: Option<i64>) -> bool {
    stamp >= first_commit.unwrap_or(FLOOR_WHEN_HISTORY_UNKNOWN)
}

/// The earliest root-commit date out of `git log --max-parents=0 --format=%ct` output. Multiple
/// lines are real — a repo holds several root commits after merging unrelated histories — and the
/// EARLIEST is the only choice that can reject nothing genuine. Unparseable lines are skipped rather
/// than fatal: `None` (nothing parsed at all) sends the caller to the static rung, and a floor
/// derived from garbage would be worse than no derivation.
pub(crate) fn earliest_root_epoch(git_stdout: &str) -> Option<i64> {
    git_stdout
        .lines()
        .filter_map(|line| line.trim().parse::<i64>().ok())
        .min()
}

#[cfg(test)]
mod tests {
    use super::{earliest_root_epoch, source_date_epoch_is_plausible, FLOOR_WHEN_HISTORY_UNKNOWN};

    /// nix stdenv's stamp: 1980-01-01, the ZIP-format minimum — the observed placeholder that
    /// motivated the floor.
    const NIX_PLACEHOLDER: i64 = 315_532_800;

    /// A stand-in for this project's derived first-commit date. Any post-2005 value exercises the
    /// same branches; a fixed one keeps every case reproducible.
    const FIRST_COMMIT: i64 = 1_780_000_000;

    /// The motivating bug, pinned: nix's 1980 placeholder must be rejected so `build.rs` enters its
    /// git-fallback path instead of baking a ~17,000-day age.
    #[test]
    fn the_nix_placeholder_is_rejected_when_history_is_known() {
        assert!(!source_date_epoch_is_plausible(
            NIX_PLACEHOLDER,
            Some(FIRST_COMMIT)
        ));
    }

    /// The same rejection WITHOUT git — the rung that matters most, because nix sandboxes strip
    /// `.git`, so the placeholder and the derivation failure arrive together. Skipping the check on
    /// derivation failure would un-fix the motivating bug in its most common environment.
    #[test]
    fn the_nix_placeholder_is_rejected_when_history_is_unknown_too() {
        assert!(!source_date_epoch_is_plausible(NIX_PLACEHOLDER, None));
        assert!(!source_date_epoch_is_plausible(0, None));
    }

    /// The positive lane: a packager who states the source date genuinely (at or after the first
    /// commit) is honored. Equality included — the derived floor is itself a real commit's date.
    #[test]
    fn a_stamp_at_or_after_the_first_commit_is_accepted() {
        assert!(source_date_epoch_is_plausible(
            FIRST_COMMIT,
            Some(FIRST_COMMIT)
        ));
        assert!(source_date_epoch_is_plausible(
            FIRST_COMMIT + 86_400,
            Some(FIRST_COMMIT)
        ));
    }

    /// Pins the comparison direction at the boundary: one second before the first commit is out.
    #[test]
    fn a_stamp_one_second_before_the_first_commit_is_rejected() {
        assert!(!source_date_epoch_is_plausible(
            FIRST_COMMIT - 1,
            Some(FIRST_COMMIT)
        ));
    }

    /// With no derived history, any modern stamp passes the static rung — a tarball build whose
    /// packager set a genuine date must not be pushed into silence by a floor meant for 1980.
    #[test]
    fn an_unknown_history_accepts_any_modern_stamp() {
        assert!(source_date_epoch_is_plausible(FIRST_COMMIT, None));
        assert!(source_date_epoch_is_plausible(
            FLOOR_WHEN_HISTORY_UNKNOWN,
            None
        ));
    }

    /// Several root commits (merged unrelated histories): the EARLIEST is the floor, because any
    /// later root would reject stamps that predate it but postdate the true beginning.
    #[test]
    fn the_earliest_of_several_roots_wins() {
        assert_eq!(
            earliest_root_epoch("1780000000\n1700000000\n"),
            Some(1_700_000_000)
        );
        assert_eq!(earliest_root_epoch("1783253851\n"), Some(1_783_253_851));
    }

    /// Output that parses nowhere yields no floor at all — the caller's next rung is the static
    /// constant, never a floor invented from garbage.
    #[test]
    fn garbage_git_output_yields_no_floor() {
        assert_eq!(earliest_root_epoch(""), None);
        assert_eq!(earliest_root_epoch("fatal: not a git repository\n"), None);
    }
}
