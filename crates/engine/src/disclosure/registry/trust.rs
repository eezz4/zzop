//! D. Trust calibration — output exists but must not be over-trusted.
//!
//! One group of the blindness registry, split from the parent module on the file-size cap along the
//! seam the array already had. The parent concatenates the four in declared order.

use super::super::types::{BlindnessClass, DisclosureStatus, TRUST_CALIBRATION};

pub(super) const ROWS: &[BlindnessClass] = &[
    BlindnessClass {
        id: "coincidental-match",
        group: TRUST_CALIBRATION,
        summary: "A cross-layer edge keyed on a generic path carries a low-confidence reason so the \
                  agent can discount an over-confident match. What is ASSERTED is exactly a NAMED LIST, \
                  not a general judgment of genericness: nine literal keys (`/health`, `/healthz`, \
                  `/ping`, `/metrics`, `/status`, `/login`, `/logout`, `/version`, `/favicon.ico`), \
                  and that list excludes by SPELLING in two ways, not the one this row used to name. \
                  Anchored, so `/api/health` is deliberately not one of them — already stated. \
                  Case-SENSITIVE, which was not: the patterns spell each path in lower case, and while \
                  key normalization upper-cases the method and drops a trailing slash (so `GET /health/` \
                  does carry the reason), nothing folds the path's case. Measured on a two-tree fixture: \
                  `GET /health` and `GET /health/` join WITH the reason, `GET /Health` and `GET /HEALTH` \
                  join WITHOUT it, and a non-generic control joins without it correctly. The sibling veto \
                  answering the same question for `unconsumed-endpoint` DOES fold case, so the two lists \
                  disagree on one spelling of one path and this side is the permissive one. Every other \
                  key joins with \
                  no reason attached, and the join itself is an exact `(kind, key)` string match with no \
                  service or deployment identity in it — so two trees that merely share a path spelling \
                  produce an edge, and that edge additionally SUPPRESSES the `unconsumed-endpoint` \
                  finding the provide would otherwise have carried. The single-provider false link is \
                  the gap this row does not close; the multi-provider collision is handled separately \
                  by the provider index's ambiguity gate.",
        // Still `asserted`, and the status is only as wide as the sentence: what cannot be silently
        // missed is that a key matching the shipped table carries its reason, on every run, with no
        // heuristic in between. It is NOT a claim that every generic-looking path gets one — the
        // anchoring and the case-sensitivity are both named above precisely so the status is read
        // against the table as SPELLED rather than against the idea of genericness.
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
        summary: "Staleness from an un-bumped fingerprint is prevented structurally by the cache \
                  fingerprint contract rather than by a runtime check, and an external reviewer spent \
                  seven tricks against it (an equal-length replacement at an identical mtime, a \
                  byte-identical file that must still invalidate cross-file, truncated/empty/garbage \
                  entries, a changed config fingerprint, four concurrent runs) with every one correctly \
                  recomputing. The run reply now also says how much of ITSELF was replayed rather than \
                  recomputed — `cache.hitFiles` of `cache.fileCount`, present whenever a cache was in \
                  play — so a reader can tell a recomputed answer from a served one without asking for \
                  a profile. TAMPERING is a different axis from staleness and was the one trick that \
                  worked: editing a well-formed entry's findings to `[]` deleted those findings from \
                  every later run, no self-healing, nothing in the output to contradict it (measured \
                  2026-08-16 with the hardcoded credential still sitting in the source). Every \
                  fingerprint in the key describes an INPUT; nothing described the output. Each entry \
                  now carries a self-hash binding its payload to its key, so a payload that does not \
                  hash to what zzop wrote there is REFUSED — recomputed from source rather than served \
                  — and the run says how many entries that happened to. The limit is stated rather than \
                  papered over: the digest is keyless and public, so it detects edits that did not know \
                  about it (corruption, a partial overwrite, a hand-edit) and not a forger who \
                  recomputed it. A keyed MAC would, and needs a key with somewhere to live. The cache \
                  directory is local and gitignored, so this is not a repository-borne vector.",
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
                  well-formed-but-false fact is indistinguishable from a true one. Detected in \
                  AGGREGATE, which is what makes the rest readable: every run an overlay contributed to \
                  now carries a warning naming each `parser` id and the counts it supplied, and saying \
                  what share of this tree's http routes were DECLARED rather than extracted — so a \
                  reader knows how much of the report rests on the adapter even though no individual \
                  fact carries its origin. The framework-silence tripwires judge on the extracted half \
                  for the same reason: an overlay used to silence the very warning that asked for it.",
        status: DisclosureStatus::Partial,
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
