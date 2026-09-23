//! A. Extraction blindness — zzop did not see something it needed to see.
//!
//! One group of the blindness registry, split from the parent module on the file-size cap along the
//! seam the array already had. The parent concatenates the four in declared order.

use super::super::types::{BlindnessClass, DisclosureStatus, EXTRACTION_BLIND};

pub(super) const ROWS: &[BlindnessClass] = &[
    BlindnessClass {
        id: "consume-side-unextracted",
        group: EXTRACTION_BLIND,
        summary: "A tree whose egress was not extracted contributes no consumes, so another tree's routes \
                  look dead. Asserted as `coverage.joinContributionZero` when a tree analyzed files but \
                  produced no JOINABLE io (zero provides AND zero keyed consumes — an unresolved consume \
                  proves the extractor saw a call site but can never join anything, so it does not count \
                  as a contribution). That conjunction is also the ceiling: a tree that DOES publish \
                  routes can never satisfy it, so its consume channel going entirely dark passes the \
                  assertion silently. Also detected self-report: a recognized http-client package import \
                  (axios, @angular/common/http, ...) while extracted `http` consumes stay near-zero (<3) \
                  self-reports the likely wrapper/DI call-idiom gap on the consume side. A lexical census \
                  of builtin `fetch(` call tokens (a global — no import to key on) likewise self-reports \
                  when 5+ call sites appear in js/ts sources while keyed `http` consumes stay near-zero \
                  (<3) — the hand-rolled-wrapper-over-fetch idiom. Every one of those asks a TREE-WIDE \
                  question at near-zero, so between them they see a tree with almost no io at all and \
                  nothing finer. The sibling `provide-side-unextracted` closes exactly that gap on ITS \
                  channel with a per-FILE gate — a file that imports a server framework and contributed \
                  no route is named with its siblings and a sample. There is no per-file twin here: \
                  nothing asks whether a file that imports an http client contributed no consume, so \
                  both partial and total consume-side loss stay invisible on any tree that also serves \
                  routes.",
        // Demoted `asserted` -> `partial` 2026-08-29. Not because a mechanism was removed — all four
        // above still ship — but because the label read the ASSERTED FACT
        // (`coverage.joinContributionZero`, emitted every run) as if it covered the CLASS, which is
        // "this tree's consume channel was not extracted". It does not: the fact's predicate is
        // conjunctive over provides AND keyed consumes, so it is structurally unable to fire on a
        // route-serving tree, and the three self-reports beside it are near-zero heuristics — the
        // definition of `partial`. Measured on a 717-file Java tree: 322 `http` provides, 0 keyed
        // consumes, 7 unresolved, `joinContributionZero` false, and not one of that run's 15 warnings
        // named the consume side, while two files in the tree drive an http client directly. The mirror
        // row `provide-side-unextracted` has strictly MORE machinery (the per-file gate this side
        // lacks) and has said `partial` since it shipped, so the pair was labelled backwards from its
        // own mechanisms; `mirrored_channel_rows_carry_the_same_status` now pins that pairing.
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "provide-side-unextracted",
        group: EXTRACTION_BLIND,
        summary: "A tree whose routes were not extracted makes a real caller look like it hits a \
                  nonexistent API (false drift). Detected self-report: a server-framework package \
                  (express, koa, fastify, ...) imported anywhere in the tree while extracted `http` \
                  provides stay near-zero (<3) self-reports the likely method-call registration gap, and \
                  the controller-decorator idiom tripwire fires the same way at near-zero (<3) provides, \
                  not just exact zero (a Spring-BE tree that keeps 2 lexically-extracted provides after \
                  losing most of its routes to a parser limit would silence an exact-zero-only gate). \
                  Both of those ask a TREE-WIDE question and can therefore only see a tree with almost \
                  no routes. Partial loss — the common case, and the one that produces confident wrong \
                  answers rather than silence — is now asked per FILE instead: a file that imports a \
                  server framework and contributed no http route, on a tree that DID extract some, is \
                  named with its siblings and a sample. Measured on the 14-route Express tree where 5 \
                  routes vanished into two files taking their app as a parameter, at which point the \
                  run reported three routes as having no provider, published a route census of 4 as a \
                  total when there were 7, and answered `not-found` for a route in the source. Still \
                  `partial`, and the residual is real: a framework-importing file that registers SOME \
                  of its routes and loses the rest contributes to both counts and is invisible to \
                  every gate here, since nothing compares a file's route count to what it should have \
                  been.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "language-unparsed",
        group: EXTRACTION_BLIND,
        summary: "A normal-sized file whose extension has no native parser is NOT counted in \
                  `coverage.degraded` (that field is size-cap/parse-failure only) — it now instead \
                  self-reports as a per-extension warning naming the extension, a file count, and a path \
                  sample, so \"this backend does not serve X\" is disclosed rather than silent. An \
                  oversized file of the same unparsed extension gets BOTH: it lands in `coverage.degraded` \
                  (silent-truncation, a size fact) AND still names its extension in the same per-extension \
                  warning (a coverage fact) — the two are orthogonal, not either/or. Not detected: an \
                  extensionless file (README, Dockerfile — no reliable language signal to key on) and a \
                  file whose extension this engine classifies as non-source (docs/data/styles/assets) but \
                  which in some atypical tree actually holds source.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "classified-skip",
        group: EXTRACTION_BLIND,
        summary: "Files skipped as minified/generated are reported once as a warning (a heuristic content \
                  match, not exhaustive); test-classified files' io facts are excluded from the \
                  cross-layer join and disclosed per tree via a warning naming the dropped counts when \
                  nonzero — raw per-file facts still remain visible in `ir.io` (the raw `zzop-facade` \
                  JSON output from embedding the engine directly; MCP tool replies and the `zzop` \
                  CLI omit `ir`).",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "resolution-gap",
        group: EXTRACTION_BLIND,
        summary: "A recognized call site whose target could not be statically resolved is counted as \
                  `coverage.ioConsumesUnresolved` on every run — so \"this call has no target\" is \
                  disclosed, not silent. That count is the whole of what is ASSERTED here, and the \
                  sentence used to fold a second, lane-conditional signal into it without saying so. \
                  \"Past a threshold, surfaced as a majority-unresolved self-report\" is \
                  `cross-layer/unresolved-consume-ratio`, a native analysis registered only for the \
                  cross-layer join: it reports into `crossLayerFindings`, which a per-tree analyze reply \
                  does not have, so it reaches a reader through the `cross` lane and never through \
                  `findings.byRule`. Measured on a tree sitting at 7 of 7 (100%) unresolved: analyze \
                  carried the count and zero `cross-layer/*` keys, while `cross` over the same file and \
                  binary reported the ratio at `ratioPercent` 100. Which analyses can only report there \
                  is itself published per run as `nativeAnalyses.reportedInCrossLayerFindings`, so the \
                  lane restriction is readable rather than folklore — but read the ratio's absence from \
                  an analyze reply as \"wrong lane\", never as \"under the threshold\".",
        // Still `asserted`, and earned by the COUNT rather than by the ratio rule:
        // `coverage.ioConsumesUnresolved` rides the per-tree census unconditionally, so there is no run
        // in which an unresolved call site can be silently missed. What the class does not claim is
        // that the JUDGMENT about that count reaches every surface — the majority self-report is
        // lane-bound, which is now said in the summary instead of implied by "past a threshold".
        //
        // FIRING PIN: crates/engine/tests/integration/analyze_coverage_census.rs::unresolved_only_tree_is_still_join_contribution_zero
        // FIRING PIN: crates/engine/tests/integration/analyze_coverage_census.rs::no_io_tree_has_zero_counts_and_is_join_contribution_zero
        status: DisclosureStatus::Asserted,
    },
    BlindnessClass {
        id: "key-mismatch-drift",
        group: EXTRACTION_BLIND,
        summary: "A consume and a provide that differ only by letter case or a path prefix are matched as \
                  a near-miss; drift from a captured base-URL prefix or other normalization is not, so a \
                  key artifact can still read as real spec drift.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "generated-client-unrecognized",
        group: EXTRACTION_BLIND,
        summary: "A tree that talks to its backend through a GENERATED client (SDK class/methods built \
                  from a committed OpenAPI/Swagger spec) makes its call sites invisible to the \
                  literal-call-site consume extractor, so a real caller can look like it never calls out. \
                  Detected self-report: a committed OpenAPI/Swagger spec file present in the tree while \
                  this tree's io stays near-zero (<3) in BOTH provides and keyed consumes. Not detected: a \
                  generated client whose backing spec is NOT committed in-tree (e.g. fetched at build \
                  time), which leaves no spec file for the self-report to anchor on.",
        status: DisclosureStatus::Partial,
    },
];
