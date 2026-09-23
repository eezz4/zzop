//! THE SHORT NOTES — what stays on the wire when a legend folds.
//!
//! Split out of `super` on 2026-09-14 for the repo's 400-line file cap, and the seam is the one the
//! two names draw: the parent owns the MECHANISM (the list, the two pointer shapes, the rendered
//! contract document) and this file owns the WORDS. Every function here answers one question —
//! *which reading, of this one legend, does a reader have to have before they can act on the numbers
//! beside it?* — and the doc above each says which measured misreading put it there.
//!
//! The bar is not a byte target. `super`'s own unit test refuses a note under 200 bytes and refuses one
//! that does not shrink its full text, and `crates/summary/tests/legend_fold.rs` names the surviving
//! clauses per legend, so a shortening that drops the load-bearing sentence reds rather than ships.

use super::inline_pointer;

/// `coverageGaps.meaning`, folded. Two things here are NOT shortening choices:
///
/// The floor is INTERPOLATED, never spelled — the share this sentence publishes has to be the share the
/// filter applied, and the full text it replaces made that same choice for that same reason.
///
/// The two `kind` tokens are DEFINED here rather than pointed at. `kind` is a value in the rows this
/// note sits beside, so a reader meets `"data-config"` in the data with nowhere to decode it; the
/// module's governing rule is that the vocabulary rides inside the object that uses it, and
/// `coverage_gaps_tests`' mall-shape pin (the 114 `.xml` MyBatis mappers) asserts exactly that. What
/// folds is everything a reader does not need AT the row: why the measuring stick is the file count,
/// which filetypes are excluded from the list entirely, how this cell relates to `unreadExtensions`.
pub(crate) fn coverage_gaps_note() -> String {
    format!(
        "Extensions that are a principal filetype of this tree (>={floor}% of its walked files, or of \
         its structurally projected files for one that parsed) and contribute ZERO resolved import \
         edges. Read a row as a place to look, NEVER as a verdict, and read its `kind` first because \
         the two kinds take different remedies. Kind \"source\": a language this tree is written in \
         that no frontend read. Kind \"data-config\": a structured data or configuration filetype \
         (json/yaml/xml/toml/ini/properties/csv/lock/html) that nobody writes a language parser for, \
         but that DOES routinely declare facts this engine's io channels carry — SQL inside MyBatis \
         .xml mappers, services in a k8s manifest, endpoints in an OpenAPI document. Whether such a \
         row cost you anything depends entirely on what those files hold and THIS BUILD DID NOT READ \
         THEM, so a 900-statement mapper directory and an i18n locales bundle are the same row until \
         you look. A `structural` of 0 has two causes taking opposite remedies: no parser claimed the \
         extension (an adapter overlay is the on-ramp), or a parser claimed the files and bailed on \
         them. Either way these files are absent from the resolved dependency graph every \
         unimported/unreachable-export verdict is computed over, so read those findings as being about \
         the REMAINING files. This list is a SHORTLIST: an empty one means the cross found nothing ABOVE the {floor}% bar, \
         never that nothing was held back. `basis` names what the bar removed — read it before reading \
         this list as an all-clear. ZERO is literal and it is the second gate: one resolved edge \
         drops the whole extension, so PARTIAL loss never shows here. {pointer}",
        floor = zzop_facade::MIN_UNCOVERED_EXTENSION_SHARE_PCT,
        pointer = inline_pointer(),
    )
}

/// `findings.byRuleMeaning`, folded. Keeps the caveat BEFORE the instruction, which is the property
/// [`crate::output::by_rule_legend`]'s own position test pins on the full text.
pub(crate) fn by_rule_note() -> String {
    format!(
        "Counts FINDINGS, not places — and how far those two diverge is a property of the RULE, so \
         this map is comparable WITHIN one rule (across runs) and NOT across rules within one run: on \
         a nine-repo corpus one rule's 26 findings stood for 719 files while another's 2,077 stood for \
         2,077 places. Every folding finding declares its own cardinality in `evidencePaths` and \
         `data`. So read an entry as \"how many times this rule had something to say\", never as \"how \
         many places are affected\". Counts here are over the FULL set even when `shown` is capped. \
         {pointer}",
        pointer = inline_pointer(),
    )
}

/// `findings.shownMeaning`, folded. Leads with the caveat rather than the key list, which is the same
/// ordering [`by_rule_note`] uses and for the same reason: the reading that changes what a reader DOES
/// with row 1 has to survive the shortening, and a five-key recital that ends in the caveat is a note
/// whose first two thirds can be skimmed. [`crate::output::shown_legend`] carries why the repair is a sentence
/// `module_map.meaning`, folded. Four readings survive, and they are the four a reader of a MAP gets
/// wrong: that the two edge numbers are the same population, that a `lines` sum covers the whole
/// module, that a missing row was capped away, and that a big box is a verdict about a big box.
pub(crate) fn module_map_note() -> String {
    format!(
        "A TOPOLOGY, never a verdict: no row says a module is badly placed, and there is no severity, \
         no score and no ranking here. READ THE TWO EDGE NUMBERS AS A PAIR — `edges[]` holds only \
         edges BETWEEN modules, while `census.fileImports` counts ALL file-level imports including \
         the ones that stayed inside one module, so the smaller number is not a subset failure. \
         `modules[].lines` is a SUM and never ships without `linesMeasuredOver`, which says over how \
         many of the row's `files` it was taken; when nothing in a module was measured BOTH keys are \
         absent rather than `0`. NOTHING IS CAPPED — every module and edge at this `fold` is \
         present, and if the answer is too large the knob is a higher `fold`, which makes the boxes \
         bigger rather than hiding some. {pointer}",
        pointer = inline_pointer(),
    )
}

/// rather than a sixth key.
pub(crate) fn shown_note() -> String {
    format!(
        "An ORDER, not a RANKING — the caveat first, because it is the one that changes what a reader \
         does with row 1. Deployment role descending (its two demoted tiers disclose themselves in this \
         same reply, as `testPaths` and `buildPaths`), then severity descending, then THREE keys that \
         INTERLEAVE rather than rank: distinct file-and-line first, then distinct rule-and-file, then \
         rule round-robin — with the engine's own severity/file/line/rule-id order as the deterministic \
         tiebreak. Shaping only — this ORDER drops nothing, and every count beside `shown` is over the \
         full set. Two other things can narrow the window, and each says so in its own key when it \
         does: `truncated` (the list cap) and `filtered` (a `severity`/`rule` filter YOU passed). \
         It carries NO claim that row 1 is likelier to be a real defect than row 40 — no per-finding \
         confidence exists here, by design — so inside one severity band the sequence is coverage of \
         distinct places and rules, and after that the letters in a file path. {pointer}",
        pointer = inline_pointer(),
    )
}

/// `findings.byDirectory.meaning`, folded. Four clauses survive the shortening and each one is a
/// reading that changes what a reader DOES with the rows, which is this fold's own bar:
///
/// The REFUSAL, with one pair from the corpus table kept inline. Stripped of evidence, "this is not a
/// verdict" reads as a disclaimer and gets skipped; the fastapi/typeorm pair is what makes it an
/// argument, and it is the shortest form of the measurement that still shows the inversion.
///
/// The two CONFIG KEYS and the difference between them, which the 2026-09-11 ruling put in this
/// channel's scope by name. A remedy that names one key would be advice to do the wrong one half the
/// time — and unlike the rest of this note, a reader acts on it immediately.
///
/// The POPULATION (`total`'s, not `shown`'s) and the SHORTLIST bar, the pair that stops a row's share
/// from being read as a share of a filtered window, and a short list from being read as the whole fold.
pub(crate) fn by_directory_note() -> String {
    format!(
        "How this tree's findings distribute over the FIRST segment of their file paths — a \
         DISTRIBUTION, never a verdict. A large share is NOT evidence that a directory is noise: \
         folded across four public trees on 2026-09-11 the top segment was documentation in one tree \
         (`fastapi`, 94.7% of 511 under `docs_src/`, with two findings in the shipped package) and \
         the product itself in the next (`typeorm`, 58.1% of 43 under `src/`), so this channel \
         reports the share and refuses to guess which kind yours is. If you decide a directory should \
         stop reporting, the two config keys are not interchangeable: top-level `exclude` drops the \
         FINDINGS and leaves the files walked, parsed and in the dependency graph, while \
         `vocabulary.skipDirs` stops the WALK, so those files leave the graph too and this reply's \
         `coverage` channels change with them. Counts are over the FULL finding set, like the `total` \
         and `byRule` beside them, never over `shown` and never narrowed by a `severity`/`rule`/`limit` \
         argument. The rows are a SHORTLIST of the largest {rows}; `basis` names how many directories \
         were crossed and how much these rows hold, so a short list never means the rest is empty. A \
         finding whose path has no directory is counted under `(root)`, and a row is keyed by NAME, so \
         on a reply that straddles trees two trees' `src/` are ONE row. {pointer}",
        rows = crate::output::by_directory::MAX_ROWS,
        pointer = inline_pointer(),
    )
}

/// `nativeAnalysesMeaning`, folded to the pointer object. The note keeps the four readings under
/// which an id's ABSENCE from `findings` is not a measured zero — the whole reason this legend was
/// added.
///
/// The count in that sentence is the one thing here that can go stale silently, and it did not survive
/// its first test: `shippedOff` was added on 2026-09-03 and this note still said "Three ways", which
/// made the folded note disagree with the very legend it points at. The reading it left behind is the
/// worst available one — a reader who counts three and finds an id in none of them concludes "measured
/// zero" about an analysis that was never run. Kept as prose rather than derived because the four
/// readings are not four keys (two of them live under `registered`), so there is nothing to count from.
pub(crate) fn native_analyses_note() -> String {
    "What the counts and lists beside `nativeAnalyses` mean, and — the reason this key exists — when \
     an id's ABSENCE from `findings` is NOT a measured zero. Four ways it is not. (1) A `disabled` \
     analysis was NOT evaluated, so its absence means not analyzed, never analyzed-and-clean; to get a \
     verdict from one, stop disabling it. (2) A `shippedOff` analysis was not evaluated either, but by \
     THIS BUILD's default rather than by your config — nothing is removed, and naming it in `rules` \
     with a severity turns it on. (3) A `reportedInCrossLayerFindings` analysis judges the \
     CROSS-TREE JOIN and reports into `crossLayerFindings`, a channel a per-tree reply does not have; \
     to get a verdict from those, run the cross-layer join over these same trees (each surface names \
     its own entry point for it). (4) Two registration classes `registered` names key no `findings` \
     entry under their own id at all — one gates a score computation and emits no finding, the other \
     is an umbrella whose findings arrive under finer `schema/<label>` ids, so look for those. For \
     every OTHER registered analysis in none of the three lists, absence from `findings` IS a measured \
     zero — and that is all it says, never whether the analysis had anything to judge."
        .to_string()
}

/// `packsLoadedMeaning`, folded to the pointer object. The note keeps `didNotRun`'s sharp edge, which
/// is the one sentence in this legend that changes what a zero means.
pub(crate) fn packs_loaded_note() -> String {
    "What each `packsLoaded` row's numbers count, and which of them is not a scan. Loading is not \
     running: a row carrying `didNotRun` loaded but was never evaluated, so ZERO FINDINGS FROM IT \
     MEAN NOT ANALYZED — never analyzed-and-clean. `filesInScope` is path candidacy checked before \
     any file content is read, and its very presence is what says the pack ran. `zeroAdmissionRules` \
     names rules of the pack that read not one byte of this tree, so their zero is scope."
        .to_string()
}

/// The folded value for a legend that ships as a JSON STRING.
/// `architecture.topRecommendationMeaning`, folded. Four auditors misread this object, each on a
/// different word, and the note keeps one clause per misread — that is the bar, not a byte target:
///
/// `severity` is the band, not a finding severity (2026-08-20, three auditors on dotnet/eShop), with
/// `findings.bySeverity` named so a reader has somewhere to go. `topItem` may be a file with no
/// findings — the same misread's second half. Disabling the RULE does not silence this lane
/// (2026-09-13, a fourth auditor), and the knob that does is named. And `idMeaning` rides beside `id`,
/// so the kind vocabulary needs no lookup at all — the one clause here that REPLACES a lookup rather
/// than pointing at one.
///
/// What folds is the ranker's input list, the `urgent-bug-risk` escalation, and the population caveat
/// `criticalTop`'s own note states two keys away.
pub(crate) fn top_recommendation_note() -> String {
    format!(
        "`topRecommendation.severity` is a RECOMMENDATION PRIORITY BAND, not a finding severity: it \
         comes from the ROI ranker's structural rules and reads NO rule findings. So a `critical` \
         here is routine on a tree whose `findings.bySeverity` holds no critical at all, and \
         `topItem` can name a file with zero findings of its own — that is the two lanes agreeing, \
         not the reply contradicting itself. Turning a RULE off does not silence this lane, even \
         where a rule id and a recommendation id are spelled the same (`circular` is one); to \
         silence it, disable `recommendations`. `id` names a recommendation KIND and `idMeaning` \
         ships beside it saying what that kind means, so read that rather than any hard-coded list \
         of spellings. {pointer}",
        pointer = inline_pointer(),
    )
}

/// `architecture.criticalTopMeaning`, folded. Two clauses survive, and both change what a reader DOES
/// with a slot:
///
/// The POPULATION — measured 2026-08-18 on this repo, all three slots were test modules — plus the
/// instruction that follows from it. And the sentence that stops the reader who has narrowed `pain`
/// from concluding this list narrowed too; it is asserted out loud rather than left accidentally
/// correct, and `analyze::architecture`'s own test pins the phrase.
///
/// What folds is the tie-break argument (why blast radius alone does not reproduce the three) and the
/// hand-off to the `rule-catalog` document for the churn ranking this is not.
pub(crate) fn critical_top_note() -> String {
    format!(
        "Up to 3 paths from the SIZE-WEIGHTED critical list (blast_radius * ln(loc+2) — blast radius \
         alone does NOT reproduce them). The population is EVERY analyzed file: test files are NOT \
         excluded, so a test module can and does appear here, and a slot reads as \"widest \
         structural reach in the tree as analyzed\" rather than \"refactor this\". This holds in \
         EVERY run, including one whose `pain` excluded them — `scores.excludeTestFilesFromFileMetrics` \
         narrows the structural SCORES and this list is not a score, so the two can legitimately \
         disagree and on a real tree they do. {pointer}",
        pointer = inline_pointer(),
    )
}
