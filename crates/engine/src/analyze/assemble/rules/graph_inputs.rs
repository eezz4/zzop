//! The whole-graph input bundle [`super::run`] reads — its own file because the parent module head is
//! at the repo's 300-line ceiling and this struct is the one part of it that is pure declaration.

/// The whole-graph inputs the graph analyses (`circular`, `unreachable`, `dead-candidates`,
/// `unimported-export`) read, grouped so [`super::run`] stays readable — it took 21 positional
/// parameters before this, and threading the overlay entry set as a 22nd would have made the signature
/// itself the defect. Every field here is consumed only inside those gates.
pub(in crate::analyze::assemble) struct GraphInputs<'a> {
    pub(in crate::analyze::assemble) cycles: &'a [Vec<String>],
    pub(in crate::analyze::assemble) nodes: &'a [zzop_core::FileNode],
    pub(in crate::analyze::assemble) dep: &'a zzop_core::ir::DepGraph,
    /// `.ts` targets imported ONLY by a pre-scanned file — seeded as `unreachable` entries.
    pub(in crate::analyze::assemble) prescan_targets: &'a std::collections::HashSet<String>,
    /// Runtime asset-URL targets (worklet/worker/`importScripts`/`new URL`) — same `unreachable` seed.
    pub(in crate::analyze::assemble) asset_targets: &'a std::collections::HashSet<String>,
    /// Files a Nuxt AUTO-IMPORT reaches by bare symbol name — same `unreachable` seed, and the largest
    /// of the three on a Nuxt tree.
    pub(in crate::analyze::assemble) auto_import_targets: &'a std::collections::HashSet<String>,
    /// The SAME resolution at symbol granularity: per reached file, WHICH of its export names another
    /// file in the same app writes bare. `auto_import_targets` answers `dead-candidates`' FILE
    /// question; this answers `unimported-export`'s SYMBOL question, and a fan-in count cannot stand in
    /// for it — a file with one live export and six dead ones has fan-in 1.
    pub(in crate::analyze::assemble) auto_import_names:
        &'a std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// Paths an APPLIED Mode B adapter overlay declared `is_entry: true`
    /// (`envelope::OverlayApplication::entry_paths`), unioned into `dead-candidates`' `extra_entries`.
    ///
    /// It comes from the apply loop's verdict and NOT from `EngineConfig::adapter_overlays`, which is
    /// what this field exists to fix: reading the config directly honored the `is_entry` of an overlay
    /// `apply_adapter_overlays` had REJECTED (failed `validate_envelope`, or failed to serialize), so a
    /// file behind a rejected overlay stayed exempt from `dead-candidates` — a dead file going unnamed.
    /// Under-detection, so it survived a long time; still wrong. Same "applied, not declared" rule
    /// `covered_paths` already enforces for the "no native parser" disclosure.
    pub(in crate::analyze::assemble) overlay_entry_paths: &'a std::collections::HashSet<String>,
}
