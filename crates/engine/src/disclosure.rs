//! Known silent-failure-class registry — zzop's honest, pinned list of the ways its own output can be
//! silently misread (the taxonomy behind the coverage-disclosure feature, Stage 2). An AI consumer must
//! learn not just what zzop found, but which CLASSES of blindness zzop does and does NOT yet actively
//! detect — "meta honesty": zzop never pretends to be silently complete, so even an unknown-unknown
//! leaves the holes in zzop's OWN disclosure visible. Pinned by a meta test (see the `tests` module) so
//! extending the taxonomy without registering the new class fails the gate.
//!
//! DELIVERY, since 2026-07-29 (partial reversal of coverage-disclosure decision 1c): the run reply
//! carries this registry's SHAPE — [`disclosure_counts`], so "gaps exist, and there are this many" is
//! still unmissable without asking — while the PROSE moved to the `disclosure-classes` contract
//! document ([`disclosure_contract_text`], served by `zzop contract` and MCP `resources/read`). The
//! text was ~10.6KB of byte-identical bytes on every call, ~65% of a small tree's whole reply, i.e. a
//! fixed tax on the reader it was written for. Both halves derive from `BLINDNESS_REGISTRY` here, which
//! is what keeps the fold honest; the run-VARYING disclosure channels (`coverage.joinContributionZero`,
//! the `warnings` tripwires) never rode this registry and are untouched.
//!
//! Vocabulary-free by construction: every `summary` describes a MECHANISM (a census fact, a self-report,
//! a low-confidence marker), never a rule-pack id — the registry is meta about detection, not a rule list.

mod types;

pub use types::{BlindnessClass, DisclosureStatus};
use types::{ANALYSIS_DARK, EXTRACTION_BLIND, INPUT_CONFIG, TRUST_CALIBRATION};

/// The pinned registry, in stable order (group extraction -> analysis -> input -> trust, taxonomy order
/// within a group). Statuses reflect what is SHIPPED as of Stage 2 (the per-tree coverage census +
/// `joinContributionZero` assertion, the pre-existing self-report warnings, near-miss matching, and
/// low-confidence edge markers).
pub const BLINDNESS_REGISTRY: &[BlindnessClass] = &[
    // A. Extraction blindness — zzop did not see something it needed to see.
    BlindnessClass {
        id: "consume-side-unextracted",
        group: EXTRACTION_BLIND,
        summary: "A tree whose egress was not extracted contributes no consumes, so another tree's routes \
                  look dead. Asserted as `coverage.joinContributionZero` when a tree analyzed files but \
                  produced no JOINABLE io (zero provides AND zero keyed consumes — an unresolved consume \
                  proves the extractor saw a call site but can never join anything, so it does not count \
                  as a contribution). Also detected self-report: a recognized http-client package import \
                  (axios, @angular/common/http, ...) while extracted `http` consumes stay near-zero (<3) \
                  self-reports the likely wrapper/DI call-idiom gap on the consume side. A lexical census \
                  of builtin `fetch(` call tokens (a global — no import to key on) likewise self-reports \
                  when 5+ call sites appear in js/ts sources while keyed `http` consumes stay near-zero \
                  (<3) — the hand-rolled-wrapper-over-fetch idiom.",
        status: DisclosureStatus::Asserted,
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
                  Not detected: proportional under-extraction on a tree already recognized as SOME \
                  provides (a framework partially, not wholly, unsupported).",
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
                  `coverage.ioConsumesUnresolved` and, past a threshold, surfaced as a majority-unresolved \
                  self-report — so \"this call has no target\" is disclosed, not silent.",
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
    // B. Analysis dark — a channel is empty so a number is meaningless, yet a number is printed.
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
                  declaration, the CLI-only `zzop coverage` lane lists exactly this (it has no MCP tool twin; an \
                  MCP host reads the same declarations out of this document): its `trees[].blindSpots` crosses \
                  each declaration with the tree's structural extension mix and names, per tree, which \
                  declared rules lack their evidence channel there. The analyze reply itself still \
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
        status: DisclosureStatus::Asserted,
    },
    BlindnessClass {
        id: "capability-absent-vs-empty",
        group: ANALYSIS_DARK,
        summary: "An optional capability that was not run (git history, DSL packs) emits a self-report so \
                  \"0 findings\" is not confused with \"never ran\" — a present output field means the \
                  capability ran.",
        status: DisclosureStatus::Asserted,
    },
    // C. Input / config — the run differed from what the user thought they asked for.
    BlindnessClass {
        id: "input-scope-error",
        group: INPUT_CONFIG,
        summary: "A root that does not exist / is not a directory, or that yields zero files, \
                  self-reports as a leading warning; a too-narrow root that still matches SOME files \
                  (partial scope) is not detected.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "config-error",
        group: INPUT_CONFIG,
        summary: "A `disabledRules` id that matches no known rule (a typo) is reported as a diagnostic, so \
                  a mis-disabled rule does not silently look like \"that problem is absent\".",
        status: DisclosureStatus::Asserted,
    },
    // D. Trust calibration — output exists but must not be over-trusted.
    BlindnessClass {
        id: "coincidental-match",
        group: TRUST_CALIBRATION,
        summary: "A cross-layer edge keyed on a generic path (e.g. /health) carries a low-confidence \
                  reason so the agent can discount an over-confident match.",
        status: DisclosureStatus::Asserted,
    },
    BlindnessClass {
        id: "silent-truncation",
        group: TRUST_CALIBRATION,
        summary: "A file over the size cap falls back to a counted `degraded` state and minified skips are \
                  warned, so a dropped file is not invisible; not every internal cap is individually \
                  surfaced.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "stale-cache",
        group: TRUST_CALIBRATION,
        summary: "Stale results from an un-bumped fingerprint are prevented structurally by the cache \
                  fingerprint contract rather than surfaced as a runtime signal, so there is no per-run \
                  staleness flag to read.",
        status: DisclosureStatus::Partial,
    },
    BlindnessClass {
        id: "overlay-facts-unverified",
        group: TRUST_CALIBRATION,
        summary: "A structurally valid adapter overlay's semantic accuracy and in-file extraction \
                  completeness are not verifiable by this engine — injected facts merge trusted exactly \
                  as asserted, so a wrong key, a fabricated provide, or a call site the adapter missed \
                  inside a file it claims to cover reads as a confidently-extracted census. Only the \
                  overlay's structural shape is checked (envelope validation, `source` mismatch, \
                  synthetic-path census, zero-fact coverage — each already a warning); a \
                  well-formed-but-false fact is indistinguishable from a true one.",
        status: DisclosureStatus::NotYetDetected,
    },
    BlindnessClass {
        id: "join-bucket-unfiltered",
        group: TRUST_CALIBRATION,
        summary: "A cross-layer join bucket (`crossLayer.unprovidedConsumes` and its siblings, plus the \
                  `distinctBucketKeys`/`distinctBucketKeyFirstSites` lists derived from them) is the \
                  STRUCTURAL residue of the \
                  (kind, key) join, not a findings list. The only filters applied at that layer are ones \
                  readable from the key or the file itself — an unresolvable key, an absolute-URL \
                  (external-egress) key, a test-classified file, provider absence or ambiguity. No \
                  DOMAIN-VOCABULARY filter runs there by design (the linker is kind-agnostic and holds no \
                  rule vocabulary), so entries a rule layer vetoes as not-really-API — static-asset \
                  fetches and the like — still sit in the bucket. Reading bucket counts or keys as \
                  findings therefore OVER-counts relative to the rules reporting the same class, which \
                  apply those extra vetoes on top: findings are the filtered view, buckets are the raw \
                  join fact, and the two disagreeing on one key is the contract working, not drift. Not \
                  detected: no per-key marker says WHICH bucket entries a rule layer would veto, so the \
                  over-count is disclosed as a contract rather than measured per run.",
        // `NotYetDetected`, not `Partial`: nothing here is detected in the common cases and missed in the
        // rest — the over-count is never measured per run at all. `zzop explain` prints this token
        // verbatim, so a `Partial` here would promise a per-run signal the class explicitly does not have.
        status: DisclosureStatus::NotYetDetected,
    },
];

/// The pinned silent-failure-class registry — see `BLINDNESS_REGISTRY`. A function accessor keeps the
/// static behind the same call-shape as other engine surfaces (`register_all_native`, etc.).
pub fn blindness_registry() -> &'static [BlindnessClass] {
    BLINDNESS_REGISTRY
}

/// The registry's two DERIVED views — the per-status tallies a run reply carries, and the full text
/// the contract lane serves. Both live one file down (`disclosure/document.rs`) so this file stays the
/// registry DATA and nothing else.
mod document;

pub use document::{disclosure_contract_text, disclosure_counts};

#[cfg(test)]
mod tests;
