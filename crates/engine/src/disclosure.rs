//! Known silent-failure-class registry — zzop's honest, pinned list of the ways its own output can be
//! silently misread (the taxonomy behind the coverage-disclosure feature, Stage 2). Emitted every run so
//! an AI consumer learns not just what zzop found, but which CLASSES of blindness zzop does and does NOT
//! yet actively detect — "meta honesty": zzop never pretends to be silently complete, so even an
//! unknown-unknown leaves the holes in zzop's OWN disclosure visible. Pinned by a meta test (see the
//! `tests` module) so extending the taxonomy without registering the new class fails the gate.
//!
//! Vocabulary-free by construction: every `summary` describes a MECHANISM (a census fact, a self-report,
//! a low-confidence marker), never a rule-pack id — the registry is meta about detection, not a rule list.

/// How completely zzop detects a given silent-failure class today. The status is an honest snapshot of
/// SHIPPED detection, not of the design's aspiration — a class the design intends to assert but has not
/// implemented yet is `NotYetDetected` here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisclosureStatus {
    /// Asserted from a structural fact on every run — cannot be silently missed (e.g. a count that is
    /// always emitted).
    Asserted,
    /// Detected in the common cases, but a member of the class can still slip past — a heuristic, or a
    /// signal that is folded into a coarser count rather than broken out.
    Partial,
    /// Recognized as a real failure class that zzop does NOT yet detect — declared precisely so the agent
    /// does not assume coverage it does not have.
    NotYetDetected,
}

impl DisclosureStatus {
    /// The camelCase wire token (the output contract; the napi view serializes this verbatim).
    pub fn as_str(self) -> &'static str {
        match self {
            DisclosureStatus::Asserted => "asserted",
            DisclosureStatus::Partial => "partial",
            DisclosureStatus::NotYetDetected => "notYetDetected",
        }
    }
}

/// One entry in the silent-failure-class registry.
#[derive(Debug, Clone, Copy)]
pub struct BlindnessClass {
    /// Stable kebab-case id — part of the output contract, never renamed silently (the meta test pins the
    /// exact set).
    pub id: &'static str,
    /// Taxonomy group: one of `extraction-blind`, `analysis-dark`, `input-config`, `trust-calibration`.
    pub group: &'static str,
    /// The concrete way an agent could silently misread zzop's output for this class — phrased as the
    /// misreading, so a `NotYetDetected` entry reads as an actionable "do not assume I caught this".
    pub summary: &'static str,
    pub status: DisclosureStatus,
}

const EXTRACTION_BLIND: &str = "extraction-blind";
const ANALYSIS_DARK: &str = "analysis-dark";
const INPUT_CONFIG: &str = "input-config";
const TRUST_CALIBRATION: &str = "trust-calibration";

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
                  produced zero io. Also detected self-report: a recognized http-client package import \
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
        summary: "Files skipped as minified/generated are reported once as a warning; test-classified \
                  files are excluded without a per-reason skip census.",
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
        summary: "The census reports channel-fill counts (`importEdges`, io), so a zero-fill channel is \
                  visible; but zzop does not yet ASSERT that graph findings (cycles, dead code) are \
                  meaningless for a tree whose import edges are zero.",
        status: DisclosureStatus::Partial,
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
];

/// The pinned silent-failure-class registry — see `BLINDNESS_REGISTRY`. A function accessor keeps the
/// static behind the same call-shape as other engine surfaces (`register_all_native`, etc.).
pub fn blindness_registry() -> &'static [BlindnessClass] {
    BLINDNESS_REGISTRY
}

#[cfg(test)]
mod tests;
