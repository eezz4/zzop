//! What an overlay run ACTUALLY applied — the record the apply loop builds as it goes, and the one
//! place the extracted/declared distinction survives.

use std::collections::{BTreeMap, HashSet};

use zzop_core::NormalizedEnvelope;

/// What [`super::apply_adapter_overlays`] ACTUALLY applied, as opposed to what config DECLARED. Every
/// field is derived inside the apply loop, after the validation gate, so a REJECTED overlay contributes
/// to none of them — the whole point of returning this instead of letting a consumer re-read
/// `EngineConfig::adapter_overlays`. `Default` is the honest empty value for a run with no overlays.
///
/// - `covered_paths` — every applied fact-carrying projection's path (`analyze::assemble`'s exclusion
///   set for the "no native parser, bring an adapter" disclosure).
/// - `entry_paths` — its `is_entry: true` subset, unioned into `dead_candidate_findings`'
///   `extra_entries` by `analyze::assemble::rules`.
/// - `io_by_parser` — the PROVENANCE record. It exists because merging is lossy in exactly one
///   direction that matters: once an overlay's `IoProvide` sits in an artifact it is indistinguishable
///   from one the native parser read, so nothing downstream can tell a fact zzop EXTRACTED from a fact
///   a caller DECLARED. `analyze::diagnostics::overlay_provenance` owns both consumers and the measured
///   case that made them necessary.
#[derive(Debug, Default)]
pub(crate) struct OverlayApplication {
    pub(crate) covered_paths: HashSet<String>,
    pub(crate) entry_paths: HashSet<String>,
    pub(crate) io_by_parser: BTreeMap<String, OverlayIoCounts>,
}

impl OverlayApplication {
    /// Tallies one ACCEPTED overlay's io contribution. Called at merge time and nowhere else: that is
    /// the last moment the two provenances are still separable, since the merge one step later makes
    /// these entries indistinguishable from natively-extracted ones.
    pub(super) fn record_io(&mut self, overlay: &NormalizedEnvelope) {
        let counts = self.io_by_parser.entry(overlay.parser.clone()).or_default();
        for file in &overlay.files {
            for p in &file.io.provides {
                if p.kind == "http" {
                    counts.http_provides += 1;
                } else {
                    counts.other_provides += 1;
                }
            }
            for c in &file.io.consumes {
                if c.kind == "http" {
                    counts.http_consumes += 1;
                } else {
                    counts.other_consumes += 1;
                }
            }
        }
    }
}

/// One accepted overlay's io contribution, by kind of interest. `http` is broken out because the
/// framework-silence tripwires judge on http counts specifically and must judge on the NATIVE half.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OverlayIoCounts {
    pub(crate) http_provides: usize,
    pub(crate) http_consumes: usize,
    pub(crate) other_provides: usize,
    pub(crate) other_consumes: usize,
}

impl OverlayIoCounts {
    pub(crate) fn total(&self) -> usize {
        self.http_provides + self.http_consumes + self.other_provides + self.other_consumes
    }
}
