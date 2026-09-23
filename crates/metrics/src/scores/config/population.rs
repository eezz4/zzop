//! Which files the structural scores are allowed to COUNT — the scored population, held as config
//! rather than decided in code.
//!
//! ## The measurement this exists for
//! Every score in `scores/*` is a ratio or an average over "the files in this tree", and until this
//! type existed that population was whatever the dep graph and the node list happened to contain —
//! test files included. A repo whose tests outnumber its sources therefore had its structural answers
//! computed largely about code that never ships: measured over the 21-tree `corpus/frameworks` set,
//! turning this on takes `godFile`'s own published population from 3603 to 583 files on typeorm and
//! from 16884 to 11807 on grafana.
//!
//! ## WHICH scores it reaches, measured — not all of them
//! It reaches the ELEVEN metrics that take the per-file subject gate
//! (`crate::scores::compute::ScoresInput::is_scored`), and the `health.pain` rollup over them. Moved
//! on the 21-tree corpus, key off then on: `health.pain` 21 trees, `fileSizeCompliance` 20, `godFile`
//! 20, `siblingCross` 13, `hierarchy` 11, `publicApi` 9, `diamond` 6, `coupling` 4,
//! `featureSlicedDesign` 2. `busFactor` moved 0 there for a CORPUS reason rather than a wiring one —
//! every one of those checkouts is a depth-1 clone (`git rev-parse --is-shallow-repository` -> true),
//! so no file reaches its churn gate at all. On a tree with real history its published population
//! (`busFactor.total`) does move — verified on this repo, whose figures are deliberately not quoted
//! here because they drift with every commit; recount by diffing `busFactor.total` across two runs of
//! the same tree with the key off and on.
//!
//! The four FOLDER/SLICE-keyed metrics (`cohesion`, `sdp`, `main_sequence`, `modularity`) do NOT move
//! — 0 of 21 trees each — because a per-file subject gate has no per-file subject to remove from a
//! directory rollup. `compute_scores`' own doc carries why narrowing them was tried, measured and
//! reverted (2026-09-11 user ruling).
//!
//! ## Why a config knob and not a behavior change
//! "Test file" is a PROJECT convention, not an ecosystem fact — `zzop_core::is_test_file` reads the
//! shared `${test-paths}` vocabulary, and a project widens it with
//! `vocabulary.extraTestPathPatterns`. A build that simply stopped counting those paths would move
//! every score of every existing user on upgrade, on an assumption they never declared. So the
//! default here is [`PopulationFilter::default`] — EXCLUDES NOTHING, byte-identical to the behavior
//! that shipped before this type — and the exclusion is opened by one config key,
//! `scores.excludeTestFilesFromFileMetrics`.
//!
//! ## This is a SCORING axis, never a detection one
//! Nothing here reaches rule evaluation: a test file excluded from the scored population is still
//! parsed, still a node with all its edges, and still reported on by every rule that did not itself
//! decline test paths. What changes is only whether it counts as a SUBJECT of a score — the same
//! boundary `crate::scores::compute::ScoresInput::is_scored` already draws for the top-level
//! `exclude` key.

/// The scored-population gate carried on [`super::ScoresConfig`].
///
/// Two independent arms, both off by default:
/// * `exclude_test_paths` — the shared, ecosystem-fixed test-path vocabulary
///   (`zzop_core::is_test_file`, which reads `crates/core/src/dsl/shared_fragments.json`'s
///   `test-paths` fragment: `*.test.*`/`*.spec.*`, `__tests__/`, `tests/`, `fixtures/`, `_test.go`,
///   `test_*.py`, `*Tests.cs`, `*Test.java`, ...).
/// * `extra_test_paths` — the project's own additions, already compiled and joined into ONE
///   alternation by the caller that owns `vocabulary.extraTestPathPatterns`. Held as an
///   already-validated regex because the arm-by-arm validation (and the warning naming an arm that
///   does not compile) belongs to that caller, not here.
///
/// `extra_test_paths` alone never excludes anything: it widens the test-path vocabulary, so it is
/// read only when `exclude_test_paths` is on. That keeps one question with one answer — "is the test
/// population excluded" — instead of two knobs that can disagree.
#[derive(Debug, Clone, Default)]
pub struct PopulationFilter {
    exclude_test_paths: bool,
    extra_test_paths: Option<regex::Regex>,
}

impl PopulationFilter {
    /// The population every score counted before this type existed: every file the caller hands in.
    /// Identical to [`PopulationFilter::default`]; spelled as a named constructor so a call site can
    /// say "keep everything" out loud instead of relying on a reader to know what the default is.
    pub fn keep_everything() -> Self {
        PopulationFilter::default()
    }

    /// Drops the shared test-path vocabulary from the scored population, plus `extra_test_paths`
    /// when the project declared any (the joined alternation built from
    /// `vocabulary.extraTestPathPatterns` — pass `None` when the project declared none, or when
    /// nothing it declared compiled).
    ///
    /// An `extra_test_paths` source that does not compile is DROPPED, not propagated as an error and
    /// not panicked on: the caller that owns the key validates and warns arm by arm, so a value that
    /// reaches here broken has already been reported. Losing the tail silently narrows the exclusion
    /// (fewer files leave the population), which is the under-reach direction — the opposite of
    /// silently excluding files the project never named.
    pub fn excluding_test_paths(extra_test_paths: Option<&str>) -> Self {
        PopulationFilter {
            exclude_test_paths: true,
            extra_test_paths: extra_test_paths.and_then(|p| regex::Regex::new(p).ok()),
        }
    }

    /// True when this filter removes nothing — the default. Call sites use it to keep the untouched
    /// path literally untouched (no graph copy, no closure wrapping) rather than only arithmetically
    /// equivalent.
    pub fn keeps_everything(&self) -> bool {
        !self.exclude_test_paths
    }

    /// True when `path` is OUT of the scored population.
    pub fn excludes(&self, path: &str) -> bool {
        self.exclude_test_paths
            && (zzop_core::is_test_file(path)
                || self
                    .extra_test_paths
                    .as_ref()
                    .is_some_and(|re| re.is_match(path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default is the pre-existing behavior, stated as a test so a future edit that flips it has
    /// to delete this line rather than merely not notice.
    #[test]
    fn the_default_filter_excludes_nothing_at_all() {
        let f = PopulationFilter::default();
        assert!(f.keeps_everything());
        for p in [
            "src/index.ts",
            "src/index.test.ts",
            "__tests__/a.ts",
            "services/handler_test.go",
        ] {
            assert!(!f.excludes(p), "{p}");
        }
        assert_eq!(
            f.keeps_everything(),
            PopulationFilter::keep_everything().keeps_everything()
        );
    }

    #[test]
    fn the_test_path_arm_excludes_the_shared_vocabulary_and_nothing_else() {
        let f = PopulationFilter::excluding_test_paths(None);
        assert!(!f.keeps_everything());
        for p in [
            "src/index.test.ts",
            "src/__tests__/a.ts",
            "services/handler_test.go",
            "pkg/test_login.py",
        ] {
            assert!(f.excludes(p), "{p}");
        }
        for p in ["src/index.ts", "crates/metrics/src/scores/cohesion.rs"] {
            assert!(!f.excludes(p), "{p}");
        }
    }

    /// The declared tail widens the vocabulary — a path the shared fragment does not know leaves the
    /// population only because the project named it.
    #[test]
    fn a_declared_tail_widens_the_excluded_set() {
        let without = PopulationFilter::excluding_test_paths(None);
        let with = PopulationFilter::excluding_test_paths(Some("(?:(^|/)it/)"));
        assert!(!without.excludes("src/it/login.ts"));
        assert!(with.excludes("src/it/login.ts"));
        assert!(with.excludes("src/index.test.ts"), "the base arm survives");
        assert!(!with.excludes("src/index.ts"));
    }

    /// A tail that does not compile costs the tail, never the base arm and never a panic.
    #[test]
    fn an_uncompilable_tail_is_dropped_and_the_base_arm_survives() {
        let f = PopulationFilter::excluding_test_paths(Some("([unclosed"));
        assert!(f.excludes("src/index.test.ts"));
        assert!(!f.excludes("src/index.ts"));
    }
}
