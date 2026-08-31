//! THE REGISTRY DATA — the [`BlindnessClass`] rows themselves, and nothing else. Split out of the
//! parent on the 300-line file cap, on the same seam the parent already drew for its derived views
//! (`disclosure/document.rs`): the parent owns the CONTRACT (what this registry is for, how the two
//! views fold it, why a status may not be inflated) and this file owns the rows. Every entry keeps its
//! full prose — a row whose summary was trimmed to fit a file is a row that stopped saying what it
//! knows, which is this taxonomy pointed at itself.
//!
//! The rows outgrew one file too, so they now sit one group per module — the seam the array itself
//! already had, and the one the ORDER contract below is stated in. Splitting anywhere else would have
//! made the order an accident of where the file happened to get long.

use super::types::BlindnessClass;

mod analysis;
mod extraction;
mod input;
mod trust;

/// The pinned registry, in stable order (group extraction -> analysis -> input -> trust, taxonomy order
/// within a group). Statuses reflect what is SHIPPED as of Stage 2 (the per-tree coverage census +
/// `joinContributionZero` assertion, the pre-existing self-report warnings, near-miss matching, and
/// low-confidence edge markers).
///
/// Built once and leaked rather than assembled per call: the registry is immutable process-wide, and a
/// `&'static [BlindnessClass]` is what every consumer already takes. `const` concatenation of four
/// slices is not expressible today, and a `Vec` returned by value would change that signature for
/// every caller to buy nothing.
pub fn blindness_registry() -> &'static [BlindnessClass] {
    static ROWS: std::sync::OnceLock<Vec<BlindnessClass>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| [extraction::ROWS, analysis::ROWS, input::ROWS, trust::ROWS].concat())
}
