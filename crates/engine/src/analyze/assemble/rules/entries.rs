//! What counts as an ENTRY for `dead-candidates` — the union of every mechanism that reaches a file
//! without an import edge, so `fan_in == 0` on it proves nothing.
//!
//! Lifted out of `super`'s dead-candidates arm when that file reached its size cap. The seam is real
//! rather than convenient: each source below answers the same question from a different kind of
//! evidence — a manifest FIELD, an adapter's DECLARATION, a config's quoted TEXT, and a framework's
//! DIRECTORY NAME — and the list grows by one whenever a build learns a new way to load a file.

use std::collections::HashSet;
use std::path::Path;

use super::{config_entries, framework_entries};

/// Everything the four mechanisms name, unioned. `pkg_entries` is `PackageJsonScan::all_entry_paths()`
/// (BOTH manifest deployment roles — the package's own code and its build scripts; that method is the one
/// place the union is spelled) and `overlay_entries` is `GraphInputs::overlay_entry_paths`; both are
/// already computed upstream.
pub(super) fn collect(
    root: &Path,
    ts_paths: &HashSet<String>,
    pkg_entries: &HashSet<String>,
    overlay_entries: &HashSet<String>,
) -> HashSet<String> {
    // `extra_entries`: package.json-referenced files (manifest entry fields + lexically-scanned
    // `scripts` path tokens) — real entry points loaded by Node/bundlers/npm directly, never via
    // `import`, so `fan_in == 0` on them is expected, not dead-code signal — UNIONED with every
    // APPLIED Mode B adapter-overlay `FileProjection` marked `is_entry: true`, the overlay
    // counterpart of a manifest entry: a framework-loaded file (SvelteKit `hooks.*`/`+page`, a
    // `.vue` route, ...) an adapter declares reachable by convention rather than import. Overlays
    // are applied post-cache (`envelope::apply_adapter_overlays`, called from `analyze_tree` before
    // this function runs) and never merged into `pkg_scan` itself (a filesystem-only scan), so the
    // apply loop hands its OWN entry set down through `GraphInputs::overlay_entry_paths` — see that
    // field's doc for why re-reading `config.adapter_overlays` here was wrong.
    let mut extra_entries = pkg_entries.clone();
    extra_entries.extend(overlay_entries.iter().cloned());
    // ...and with every entry file a TOOL CONFIG in this tree NAMES: the rule already exempted the
    // config file, and this reads it — see `config_entries` for the whole design.
    extra_entries.extend(config_entries::scan(root, ts_paths));
    // ...and the ones no config STATES at all, loaded by directory CONVENTION — where this rule's
    // observation is true and only its conclusion is false. `framework_entries` has the counts.
    extra_entries.extend(framework_entries::scan(root, ts_paths));
    extra_entries
}
