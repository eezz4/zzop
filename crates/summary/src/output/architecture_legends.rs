//! The two `architecture`-object legends that FOLD, and the one that does not.
//!
//! # Why these two and not their sibling
//! All three legends on this object exist for the same reason — a word in the object is read as a word
//! from a different vocabulary in the same reply — but only two of them are RUN-INVARIANT, which is the
//! fold's entry condition (`super::legends`' own doc states it).
//!
//! 📏 Measured 2026-09-14 (ledger V237): `zzop analyze <tree> --limit 0` over eight framework corpus
//! trees in six languages (fastapi, django, express, gin, aspnetcore, guava, ripgrep, typeorm) —
//! `topRecommendationMeaning` is 1,742 bytes and `criticalTopMeaning` 1,190, each BYTE-IDENTICAL on
//! all eight. Neither interpolates anything. Recount both with one md5 per tree:
//! `zzop analyze <tree> --limit 0 | jq -r '.architecture.topRecommendationMeaning' | md5sum`.
//!
//! 🔴 `painMeaning` is NOT here, and the reason is a measurement rather than a preference. Varying the
//! TREE does not separate it from these two — it too is byte-identical across all eight of them.
//! Varying the CONFIG does: on `corpus/frameworks/express`, adding
//! `"scores": {"excludeTestFilesFromFileMetrics": true}` moves it 1,619 -> 1,847 bytes, because it
//! carries a POPULATION clause computed per run. Recount both halves in one run:
//! `zzop analyze <tree> --limit 0` against `zzop analyze --config <copy with the key on> --limit 0`,
//! and compare `architecture.painMeaning`.
//!
//! ⚠ That is the trap this module doc exists to name: the EIGHT-tree sweep above reports all three
//! legends invariant, and "invariant across trees" is not "invariant across runs" — the axis that
//! separates them is the config, and only one key on it.
//!
//! 🔴 Nothing enforces this yet. `crates/summary/tests/legend_fold.rs` walks the fold list from both
//! ends, but its population is that list — so a legend that qualifies and was never added is invisible
//! to it, which is how these two shipped
//! inline for the thirteen days between the fold landing (2026-09-01) and this commit. The guard that
//! derives its population from the REPLY instead, varying both axes, is ledger row V240 and is not
//! written. Until it is, adding a legend here is a thing a reviewer has to notice.
//!
//! `painMeaning` also has an older and independent disqualification that survives whatever the bytes
//! say — output principle §1.5, a 2026-08-12 user ruling: a SYNTHESIZED NUMBER ships with the
//! statement of what it measures. Folding it would reopen that decision rather than apply this one,
//! and `super::legends::tests` pins the document against ever growing a heading for it.

/// The full text of `architecture.topRecommendationMeaning`, moved here from the shaper on 2026-09-14
/// so the fold can serve it once. Interpolates nothing — see this module's doc for the measurement.
///
/// Every clause is here because a reader got it wrong: three auditors read `severity` as a finding
/// severity (2026-08-20, dotnet/eShop) and a fourth read `id` as a rule id (2026-09-13). What stays on
/// the wire is [`super::legends::top_recommendation_note`]; the rest is one lookup away.
///
/// # The four measured misreads — moved here with the text on 2026-09-14
///
/// The VOCABULARY of the field above, and it needed one because the field reuses a word that already
/// means something else in this very reply. `severity` here is the ROI ranker's own priority band
/// (`zzop_metrics::recommendations`), which happens to be spelled with the same three tokens as a
/// finding severity and is computed from something else entirely — import cycles, fan-out, churn.
/// Measured 2026-08-20 on dotnet/eShop: `findings.bySeverity` was `{"info":8,"warning":16}` — zero
/// criticals anywhere in the run — beside `topRecommendation: {"id":"circular","severity":
/// "critical","topItem":"tests/Ordering.UnitTests/Domain/OrderAggregateTest.cs"}`, a file `zzop file`
/// reports `total: 0` findings on, while the run's ONE `circular` finding was a warning on a
/// different file. Two independent auditors read that as the reply contradicting itself. The
/// ranking is right; the word is load-bearing and was undefined. Renaming the field would be a
/// major bump under VERSIONING.md (CLI JSON field names and types are the compatibility surface),
/// so the token stays and the legend ships beside it — the same device `painMeaning` and
/// `criticalTopMeaning` already use, for the same failure.
///
/// 2026-09-13 (ledger V196): a THIRD auditor read the same shape the same way, and this time the
/// load-bearing word was `id`, not `severity`. The legend said only that `id` is "one of a closed
/// set the `rule-catalog` contract document defines" — which points at the RULE catalog and invites
/// exactly the inference that was drawn: that disabling the rule `circular` should silence a
/// recommendation whose `id` is `circular`. Measured: with `rules: {"circular": "off"}` the reply
/// carries `findings.total: 0` and `byRule.circular` absent, beside an unchanged
/// `topRecommendation: {"id":"circular","severity":"critical","topItem":"circularB.ts"}`.
///
/// The BEHAVIOUR is right and is not changed here: this lane reads no findings, so a rule's disable
/// has nothing to act on. What was missing is that the reader is never told the two spellings
/// coincide, or which knob actually silences this lane. Three auditors reaching for one wrong
/// reading is a measurement of the legend, not of them.
pub(crate) const TOP_RECOMMENDATION_MEANING: &str =
    "`topRecommendation.severity` is a RECOMMENDATION PRIORITY BAND, not a finding severity, \
     and the two are computed from different things: this band comes from the ROI ranker's \
     structural rules (import cycles, fat fan-out, churn-per-LOC, hidden coupling), which \
     read no rule findings. So a `critical` here is routine on a tree whose \
     `findings.bySeverity` holds no critical at all, and `topItem` names a FILE that may have \
     zero findings of its own — read `findings.bySeverity` for the finding census and \
     `findings.shown` for what fired where; neither is summarized by this field. The one \
     place the two vocabularies touch: a recommendation whose file also carries a CRITICAL \
     rule finding is escalated into the `urgent-bug-risk` group, so `id: \"urgent-bug-risk\"` \
     is the only band value that implies a finding exists. `id` names a RECOMMENDATION \
     KIND, and `idMeaning` beside it says what THAT kind means — so nothing here has to be \
     looked up, and a kind added in a later version arrives explaining itself. The set is not \
     frozen: read `idMeaning`, never a hard-coded list of spellings. SOME OF THOSE SPELLINGS \
     ARE ALSO RULE IDS, which is the second place these two vocabularies look \
     alike without being the same. `circular` is one. Turning that RULE off with `rules` / \
     `disabledRules` removes its FINDINGS and leaves this recommendation standing, because the \
     ranker never read those findings to begin with: a reply can carry `findings.total: 0` \
     beside a `critical` recommendation naming a specific file, and that is the two lanes \
     agreeing rather than the reply contradicting itself. To silence this lane, disable \
     `recommendations`. Population caveat: same as `criticalTop`'s — test files are not \
     excluded, so a test module can be the `topItem`.";

/// The full text of `architecture.criticalTopMeaning`, moved here on the same day and for the same
/// reason. Interpolates nothing.
///
/// Its load-bearing clause is the POPULATION one: measured 2026-08-18 on this repo, all three
/// `criticalTop` slots were test modules. The second is the one that is easy to make false by
/// accident — `scores.excludeTestFilesFromFileMetrics` re-bases `pain` and does NOT reach this list,
/// verified end to end on `corpus/frameworks/express` (the two runs' `criticalTop` are byte-identical
/// at slot 2 = `test/support/utils.js` while `pain` moves 30.0 -> 11.5).
///
/// # Why each clause is load-bearing — moved here with the text on 2026-09-14
///
/// The POPULATION rides with the list, the same contract `painMeaning`/`verdictMeaning`/
/// `bucketMeaning` already carry: a ranking whose subject set is unstated gets read as a ranking
/// over the code that ships. It is not — measured 2026-08-18 on this repo, where all three
/// `criticalTop` slots were test modules.
///
/// This comment used to justify the sentence with "`crates/metrics` never calls
/// `zzop_core::is_test_file`". THAT IS NO LONGER TRUE (2026-09-11): it does, through
/// `zzop_metrics::PopulationFilter`, whenever `scores.excludeTestFilesFromFileMetrics` is set. The
/// SENTENCE survived the change for a reason that has to be stated rather than inherited —
/// `criticalTop` comes from `compute_criticality`, which takes no `ScoresConfig` and therefore
/// cannot be narrowed by that key. Verified end to end rather than read off the signature: on
/// `corpus/frameworks/express`, the two runs' `criticalTop` are byte-identical (slot 2 =
/// `test/support/utils.js`) while `pain` moves 30.0 -> 11.5.
///
/// That is exactly why the legend now says so OUT LOUD instead of being accidentally correct: a
/// reader who turns the key on and reads "test files are NOT excluded" beside a re-based `pain`
/// would conclude the run narrowed nothing, and the next edit to either half could make the
/// sentence false without anyone noticing it had been load-bearing.
pub(crate) const CRITICAL_TOP_MEANING: &str =
    "Up to 3 paths from the SIZE-WEIGHTED critical list (blast_radius * ln(loc+2)). The \
     population is EVERY analyzed file: test files are NOT excluded, so a test module can \
     and does appear here even though nothing depends on it at runtime. Read a slot as \
     \"this file has the widest structural reach in the tree as analyzed\", then check \
     whether it is test code before treating it as a refactor target. This holds in EVERY \
     run, including one whose `pain` excluded them: \
     `scores.excludeTestFilesFromFileMetrics` narrows the structural SCORES, and this list is \
     not a score — it is computed outside that subsystem and that key does not reach it. \
     Read painMeaning's POPULATION clause for what that run's pain counted; the two can \
     legitimately disagree, and on a real tree they do. The same caveat \
     applies to `topRecommendation`, which ranks over the same population. Blast radius \
     is the TIE-BREAK, not the key: a 400-line core outranks a 5-line re-export barrel of \
     equal blast, so re-sorting `critical` by `blastRadius` alone does NOT reproduce these \
     three. This is also NOT the churn hotspot ranking — that is a different list, and the \
     `rule-catalog` contract document's criticality entry owns it.";
