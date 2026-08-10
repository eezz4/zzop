//! The two TYPES the registry is built from — [`DisclosureStatus`] and [`BlindnessClass`].
//!
//! Split out of the parent `disclosure.rs` on 2026-08-08 to stay under the per-file line cap when the
//! `score-population-empty` class was registered. The seam keeps the parent doing exactly what its own
//! doc claims — holding the registry DATA and nothing else — while the vocabulary those rows are
//! written in lives here.

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
    /// The camelCase wire token (the output contract; the wire output view serializes this verbatim).
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

/// Taxonomy group token — see [`BlindnessClass::group`].
pub(super) const EXTRACTION_BLIND: &str = "extraction-blind";
/// Taxonomy group token — see [`BlindnessClass::group`].
pub(super) const ANALYSIS_DARK: &str = "analysis-dark";
/// Taxonomy group token — see [`BlindnessClass::group`].
pub(super) const INPUT_CONFIG: &str = "input-config";
/// Taxonomy group token — see [`BlindnessClass::group`].
pub(super) const TRUST_CALIBRATION: &str = "trust-calibration";
