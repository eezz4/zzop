//! Structural-score policy — the request-side shape of the config's `scores` object.
//!
//! A roof of its own rather than a `vocabulary` key: every key under that one names something the
//! PROJECT calls its own (a guard, a segment, a directory), and this one names a POLICY about which
//! files a score may count. The same line `parsers` is kept on the other side of.

use serde::Deserialize;

/// Structural-score policy, the request-side shape of the config's `scores` object.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScoresRequest {
    /// Drop test files from the population every structural score is computed over.
    ///
    /// `false` (the default, and what an absent `scores` object means) counts them — the population
    /// every score has always used, so an existing config's numbers do not move on upgrade. `true`
    /// removes every path the shared test-path vocabulary matches, widened by this tree's own
    /// `vocabulary.extraTestPathPatterns`.
    ///
    /// SCORES ONLY. No rule reads this, so no finding appears or disappears — measured on
    /// `corpus/frameworks/express`, key off then on: `findings.total` 58 -> 58, `bySeverity` and
    /// `byRule` byte-identical, while `architecture.pain` went 30.0 -> 11.5.
    ///
    /// 📏 Re-measured 2026-09-12 (review ledger V158), and again 2026-09-23 (V270): the INVARIANT held
    /// both times (the key moves no finding) but the absolute count drifted 56 -> 58, so re-run rather
    /// than trusting the digits. Recount --
    /// `zzop analyze corpus/frameworks/express` for the OFF side, then copy that tree's
    /// `zzop.config.jsonc` NEXT TO IT (roots resolve against the config's own directory, so a copy
    /// parked elsewhere silently analyses nothing), append
    /// `"scores": { "excludeTestFilesFromFileMetrics": true }`, and run
    /// `zzop analyze --config <the copy>` for the ON side. A measurement with neither a date nor a
    /// recount is the defect class this repo keeps finding in its own prose — and until 2026-09-23
    /// this recount named the RETIRED spelling `excludeTestFilesFromPopulation`, which is reported and
    /// never honored (`config/declared.rs`). 📏 Re-measured 2026-09-23 on the same tree: OFF pain 30,
    /// ON with that retired key pain **30 — it does not move**, ON with the key above pain **11.5**.
    /// So the command could not reproduce the number sitting three lines above it. A recount that is
    /// MISSING gets doubted; one that is WRONG gets believed, which is why this note stays.
    ///
    /// It moves the metrics whose subject is a FILE (`godFile`, `fileSizeCompliance`, `busFactor`,
    /// `publicApi`, `hierarchy`, `siblingCross`, `diamond`, `coupling`, `featureSlicedDesign`,
    /// `renameInstability`, `fixRatio`) and the `health.pain` rollup over them. It does NOT move the
    /// four whose subject is a directory rollup (`cohesion`, `sdp`, `mainSequence`, `modularity`), and
    /// it does not reach `critical`/`recommendations` at all — those are computed outside the scores
    /// subsystem, which is why `architecture.criticalTop` can still name a test module on a run whose
    /// `pain` excluded them (measured on express: `criticalTop` byte-identical across the two runs,
    /// slot 2 = `test/support/utils.js`).
    pub exclude_test_files_from_file_metrics: bool,

    /// The RETIRED spelling, kept only so a config carrying it gets TOLD rather than silently ignored.
    ///
    /// 🔴 Renamed 2026-09-14 (review ledger V182, user judgment). The old name said "from Population",
    /// which reads as every score's population, and the key never did that: the four directory-rollup
    /// metrics and the `criticalTop`/`topRecommendation` rankings keep counting the whole tree. The
    /// runtime disclosure said so on every narrowed run, but a name is read at CONFIG-WRITING time,
    /// long before any reply exists — and `1.0` freezes config key names, so a rename after the tag is
    /// a MAJOR bump.
    ///
    /// NOT honored. An `Option` rather than a `bool` purely so "absent" and "present and false" stay
    /// distinguishable — both spellings of the old key earn the warning, because a reader who wrote
    /// `false` explicitly still believes this key exists. Serde drops unknown fields silently, so
    /// without this field an upgraded config would change its own numbers with nothing said; the whole
    /// point of keeping the name here is to make that impossible.
    pub exclude_test_files_from_population: Option<bool>,
}
