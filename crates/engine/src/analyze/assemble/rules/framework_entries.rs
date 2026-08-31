//! Entry files a FRAMEWORK CONVENTION declares — files a build loads because of WHERE they sit, with
//! no import edge and no config line naming them anywhere. Unioned into `dead-candidates`'
//! `extra_entries` beside [`super::config_entries`], and consumed by nothing else.
//!
//! ## Why this is a different module from `config_entries`
//! That module's whole design rests on a config file STATING a path, and its harvest requires a quoted
//! string literal carrying a source extension. A convention states nothing. Nuxt loads every file under
//! `plugins/` and `middleware/` because the directory is named `plugins`/`middleware`, and the app's
//! `nuxt.config.ts` does not mention either — there is no text to harvest. Reading the same recognizer
//! for both would mean weakening `config_entries`' extension requirement, which is the one thing keeping
//! it from turning an alias table into a blanket exemption for a whole source tree.
//!
//! ## What was measured
//! nocodb 3a5cbd5, audited by someone who had never seen this tool: of the `dead-candidates` findings in
//! `packages/nc-gui`, the 21 under `plugins/` and 4 under `middleware/` were **25 false positives with
//! zero true positives among them** — every one is `export default defineNuxtPlugin(...)` or
//! `export default defineNuxtRouteMiddleware(...)`, auto-registered, with no import site BY DESIGN.
//! Acting on the advice this rule prints for them (`Delete the file if it is genuinely unused`) removes,
//! among other things, `middleware/03.auth.global.ts` — the application's auth gate.
//!
//! That is the distinction worth keeping in view: for these files the rule's OBSERVATION is true (nothing
//! imports them) and only its CONCLUSION is false. The fix belongs here, in what counts as an entry, and
//! not in the rule's matching.
//!
//! ## Scope, and what is deliberately left out
//! Only the two directories whose contents are entries UNCONDITIONALLY. Nuxt's auto-import directories
//! (`composables/`, `utils/`, `store/`, and whatever else `imports.dirs` lists) are NOT covered here:
//! those files are referenced, by bare symbol name, from call sites this graph does read — so the
//! honest fix there is to resolve the reference, not to exempt the file, and exempting them would hide
//! the real dead code the same tree contains. That resolution now exists, one phase earlier, in
//! [`crate::analyze::assemble::nuxt_auto_import`]; the dead code an exemption would have hidden is
//! **18 files** across the three measured directories (6 under `composables/`, 9 under `utils/`, 3
//! under `store/`), each still reported by name after that repair — this doc previously said "11
//! genuinely dead composables", a number no scan reproduces in either reading.
//! `pages/` and `layouts/` are still left out: they are convention-loaded
//! too, but they were not measured here, and this module's whole claim is that each row in it has a
//! count behind it.

use std::collections::HashSet;
use std::path::Path;

use crate::analyze::assemble::nuxt_auto_import::nuxt_app_dir;

/// Directory names whose direct contents a Nuxt build registers with no import and no config line.
const NUXT_ENTRY_DIRS: &[&str] = &["plugins", "middleware"];

/// Every tracked path a framework convention makes an entry. `ts_paths` is the tree's known file set
/// (tree-relative, POSIX); `root` is unused today and taken for parity with [`super::config_entries`]'s
/// signature, since both feed one `extra_entries` union.
pub(super) fn scan(_root: &Path, ts_paths: &HashSet<String>) -> HashSet<String> {
    let app_dirs: Vec<&str> = ts_paths
        .iter()
        .filter_map(|p| nuxt_app_dir(p))
        .collect::<HashSet<&str>>()
        .into_iter()
        .collect();
    if app_dirs.is_empty() {
        return HashSet::new();
    }
    ts_paths
        .iter()
        .filter(|p| {
            app_dirs
                .iter()
                .any(|dir| NUXT_ENTRY_DIRS.iter().any(|d| under(p, dir, d)))
        })
        .cloned()
        .collect()
}

/// Is `rel` inside `<app_dir>/<sub>/`? Nested paths count — Nuxt registers `plugins/a/b.ts` too — but
/// the prefix must end at a separator, so a sibling `plugins-legacy/` directory is not swept in.
fn under(rel: &str, app_dir: &str, sub: &str) -> bool {
    let prefix = if app_dir.is_empty() {
        format!("{sub}/")
    } else {
        format!("{app_dir}/{sub}/")
    };
    rel.starts_with(&prefix) && rel.len() > prefix.len()
}

#[cfg(test)]
mod tests;
