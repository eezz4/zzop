//! B. Analysis dark — a channel is empty so a number is meaningless, yet a number is printed.
//!
//! One group of the blindness registry, split from the parent module on the file-size cap along the
//! seam the array already had. The parent concatenates the four in declared order.

use super::super::types::{BlindnessClass, DisclosureStatus, ANALYSIS_DARK};

pub(super) const ROWS: &[BlindnessClass] = &[
    BlindnessClass {
        id: "channel-empty-family-dark",
        group: ANALYSIS_DARK,
        summary: "The census reports channel-fill counts (`resolvedImportEdges`, io), so a zero-fill \
                  channel is visible; but zzop does not yet ASSERT that graph findings (cycles, dead \
                  code) are meaningless for a tree whose resolved import edges are zero.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "rule-evidence-language-gap",
        group: ANALYSIS_DARK,
        summary: "A rule's verdict is only as wide as the STRUCTURAL FACT it reads, and per-fact language \
                  coverage is uneven — several facts have exactly one producer while the rules consuming \
                  them apply no language filter of their own. Where that fact is absent the rule is \
                  structurally silent (a zero that reads as an all-clear) or, when the fact is liveness \
                  evidence, asserts from an empty channel (a finding that reads as a verdict). ONE fact \
                  is now measured per run: a tree that extracted http routes from a language with no \
                  call-site (call-graph) producer self-reports a warning naming that language, the route \
                  count, an example path, the RULE ID that goes silent on them, and the three ways to \
                  open it (adapter-overlay `auth-guarded` injection, the envelope call channel, a parser \
                  extractor). A SECOND signal joined it on 2026-07-28 and is the same class one axis \
                  over — the fact missing is not the language but a FIELD: routes extracted with an \
                  unknown HTTP method are filtered out by every write-gated rule before evaluation, so \
                  they are out of range rather than clean, and a per-run warning names their count, the \
                  file extensions carrying them, and the two ways to bring them in range \
                  (`trees[].routes`, or a Mode B overlay). A THIRD signal is one gate finer than the \
                  extension axis: http routes with no response-shape evidence (outside the one built-in \
                  Nest capture, or an unreadable annotation) get a per-tree warning counting them \
                  against the tree's http total. For the rules with a compiled-in sightline \
                  declaration, the visibility lane lists exactly this — `zzop coverage` on the CLI, the \
                  `check_coverage` tool on MCP: its `trees[].blindSpots` crosses \
                  each declaration with the tree's structural extension mix and names, per tree, which \
                  declared rules lack their evidence channel there. A FOURTH signal is the same class one \
                  layer down, gated by a PATH rather than by a missing fact: a DSL rule runs only on files \
                  its `file_pattern` matches, and the shipped packs reach the languages very unevenly \
                  (re-measured 2026-08-17 over the 118 bundled rules, by running the analysis on a \
                  tree holding only that extension and reading back how many rules were in range: \
                  `.ts` 95, `.java` 23, `.rs` 19, `.py` 13, `.go` 13, `.cs` 10 — one tree shape for \
                  all six, because a `file_pattern` carrying a path anchor admits or refuses on the \
                  directories a tree actually has, so reach is a property of the tree as much as of \
                  the rule set). A principal filetype few loaded \
                  rules reach — or, its stricter and older sibling, none — gets a per-run warning naming \
                  it, its share of the tree and how many rules were in range, so a near-empty findings \
                  list over it reads as reach and not as a clean bill. The analyze reply itself still \
                  carries no such field — the sightlined rules publish their language sightline in their \
                  own finding message and catalog entry, which by construction the silent case never \
                  renders, since a message ships only ON a finding. Read a native rule's zero as a claim \
                  about its evidence channel, not about the code: the per-fact producer matrix is in \
                  `crates/cache/src/ir_slice.rs`'s module doc and each rule's row in \
                  `docs/rules/catalog.md`.",
        // `Partial`, not `Asserted`: the per-run warnings cover two fact/rule pairs and the coverage
        // query's blindSpots cross covers only the DECLARED sightline rules, on one opt-in CLI lane —
        // a subset of rules on a subset of surfaces is not coverage. Promoting would promise a signal
        // for every uneven fact everywhere, which does not exist, and `zzop explain` prints this token
        // verbatim.
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "score-population-empty",
        group: ANALYSIS_DARK,
        summary: "A structural score is 0-100 with higher being healthier, and EVERY per-metric formula \
                  returns 100 when its population is empty — so \"judged thousands of subjects, all \
                  clean\" and \"found nothing it could judge\" produced the same number. Three ways a \
                  population empties, all measured on real trees: an input channel with no producer \
                  anywhere in the build; a metric defined over a directory CONVENTION the tree never \
                  adopted (a Go tree's `api/` is its BOTTOM layer while Feature-Sliced Design reads \
                  that name as the TOP entry layer, so the same directory scores 0 or 100 depending \
                  only on a name collision); and a resolver that returns \"no module\" for every path \
                  outside a declared vocabulary, which empties the denominator before counting starts. \
                  Asserted: every score ships the POPULATION it scored over in the same object \
                  (`featureSlicedDesign.layerClassifiedImports`, `busFactor.total`, \
                  `mainSequence.classifiedFiles`, \
                  ...), so a 0 there IS the never-measured signal — a denominator cannot be dropped in \
                  transit the way a caveat sentence can, and a derived test refuses any score field that \
                  ships without one. `health.pain` renormalizes over the measured metrics only and \
                  carries `measuredWeight`/`totalWeight` beside it, so an unmeasurable axis can no \
                  longer make a repo look HEALTHIER by quietly passing; `pain` is `null`, never 0, when \
                  nothing was measurable at all. NOT detected: whether a population that is non-zero is \
                  REPRESENTATIVE — a metric judging 3 of a tree's 4,000 files reports 3 honestly and \
                  says nothing about the other 3,997, so read every score against its own denominator \
                  rather than against the tree's file count.",
        // `Asserted`, not `Partial`: the population rides every score unconditionally and is derived
        // from the same computation that produced the number, so there is no case where a score ships
        // without it. The residual named above is a different question (is the population big enough to
        // generalize from), which this class deliberately does not claim to answer.
        //
        // FIRING PIN: crates/metrics/src/scores/meanings/tests.rs::every_score_field_the_disclosure_prose_names_is_that_scores_population
        status: DisclosureStatus::Asserted,
    },
    BlindnessClass {
        id: "capability-absent-vs-empty",
        group: ANALYSIS_DARK,
        summary: "An optional capability that was not run emits a self-report, so \"0 findings\" is not \
                  confused with \"never ran\". The ENUMERATION is the disclosure — a capability class with \
                  no channel of its own is exactly the silence this row is about — and it is three, not \
                  the two this row named until 2026-08-29: git history (a warning naming the omitted \
                  option, distinct from the one collection-failed warning, so \"never asked\" and \"asked, \
                  failed\" are told apart by which string is present); DSL rule packs (`packsLoaded`, plus \
                  a no-packs-loaded warning that counts the native analyses that ran instead); and the \
                  NATIVE analyses (`nativeAnalyses`), which got their own channel on 2026-08-28 after \
                  having none at all — it counts every registered analysis and splits the ones that could \
                  not have produced a `findings.byRule` key by CAUSE, since the remedies differ (off by id \
                  this run; shipped off by this build and not turned on, which is the same non-evaluation \
                  under someone else's choice; or reporting into the cross-layer channel a per-tree reply \
                  does not carry). \"A present output field means the capability ran\" was written for the first \
                  two and does not carry to the third: the native roster is present unconditionally, and \
                  what it asserts is which analyses could NOT have contributed — an id in none of those lists \
                  ran, and OUTSIDE the two registration classes that key no finding under their own id \
                  at all (an id gating a score computation emits no finding; an umbrella id's findings \
                  arrive under the finer `schema/<label>` ids, so look for those instead), its absence \
                  from findings is a measured zero.",
        // 🟢 And since 2026-09-07 that is MEASURED, not reasoned: `tests/integration/
        // disclosure_capability_absent.rs` withholds all three capabilities in one run and asserts each
        // channel still speaks. `disclosure/tests.rs` had named this exact hole — its own checks
        // "measure no FIRING; that needs a fixture tree shaped like the class, one per class, and is
        // not built" — and an `asserted` row that nothing runs is the worst kind to leave, because a
        // disclosure registry is the canonical answer to what this tool cannot see.
        //
        // ✅ That "one class covered, the rest label-only" residual closed on 2026-09-11: EVERY
        // `asserted` row now carries a firing-pin line naming a test that runs its claim, and
        // `disclosure::tests::every_asserted_row_names_a_firing_pin_that_exists_and_names_it_back`
        // walks each pin from both ends (the file exists, holds that `fn`, and names this class id
        // back). What it still does not do is read the assertions inside the pinned test.
        // Still `asserted`: each of the three classes emits its channel unconditionally on the run shape
        // it describes, so none of them can be silently absent. Re-judged 2026-08-29 when the residual
        // carve-out was copied in above: the six-row `asserted` audit ran three minutes BEFORE the
        // carve-out existed, so it certified this label against a sentence that has since changed. The
        // label survives that change because the two are about different things — `asserted` here is a
        // claim about CHANNEL PRESENCE (does a capability that did not run still say so), while the
        // carve-out is about how to READ one of those channels. Nothing in it makes a channel
        // conditional.
        // What this row is NOT is a derived census — the enumeration is written by hand, and it went
        // stale within a day of a third capability class shipping its channel (that commit touched no
        // file in `disclosure/`). Adding a fourth class without editing this sentence would be invisible
        // the same way; the standing spec for closing that is to derive the list from the capability
        // self-reports themselves.
        //
        // FIRING PIN: crates/engine/tests/integration/disclosure_capability_absent.rs::every_capability_the_class_names_still_reports_itself_when_it_did_not_run
        status: DisclosureStatus::Asserted,
    },
];
