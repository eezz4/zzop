//! `package.json` manifest scan: entry-like file collection + workspace-package map.

use std::fs;
use std::path::Path;

use super::manifest::{
    collect_export_path_strings, collect_exports_dot_entry, is_package_json_path,
    is_run_lifecycle_script_key, join_and_normalize, looks_like_script_path_token,
    package_json_dir,
};

/// `package_json_entries`' return: the two DEPLOYMENT ROLES a manifest names, plus `workspace_pkgs`, a
/// `name -> WorkspacePkg` map from the same manifest walk. The workspace-alias import resolver needs a
/// directory to resolve `<name>/subpath` specifiers and a resolved entry file to resolve a bare `<name>`
/// specifier.
///
/// ## Why two sets and not one (2026-08-24)
/// A single `extra_entries` field used to hold both, because its only consumer asks one question — "is
/// this file reached by a mechanism the import graph cannot see?" — and both roles answer it `yes`. But
/// the manifest states two DIFFERENT facts, and the second consumer (the summary layer's first-screen
/// ordering) needs them apart:
/// * `entry_paths` — the package's OWN code: what it exports (`main`/`module`/`bin`/`exports`) and what it
///   RUNS (a token named by an npm run-lifecycle script — see `manifest::is_run_lifecycle_script_key`).
/// * `script_paths` — path tokens named ONLY by other `scripts` keys. The package declaring how it is
///   BUILT, released or checked. A finding in one of these is about the toolchain, not the product.
///
/// ⚠ The split is NOT "entry fields vs `scripts`", and reading it that way is the defect this shape was
/// corrected for on 2026-08-24. `"start": "ts-node src/index.ts"` in a manifest with no `main` at all is
/// the whole product of `corpus/x/xai-cookbook/.../telephony/xai` — three live Express servers sorted last
/// in that tree because the first cut called every `scripts` token build surface. A `scripts` token means
/// "not shipped" only for the keys that are not how the package is RUN.
///
/// The union is what `dead-candidates` reads and it is unchanged by either revision —
/// [`PackageJsonScan::all_entry_paths`] is the one place that union is spelled, so no consumer can
/// silently pick up half of it, and moving a path between the two sets cannot move a count.
pub(crate) struct PackageJsonScan {
    /// Files the manifest declares as the package's own code — SHIPPED (`main`/`module`/`bin`/`exports`)
    /// or RUN (a path token named by `prestart`/`start`/`poststart`/the `restart` triple).
    pub entry_paths: std::collections::HashSet<String>,
    /// Files named only by non-run `scripts` keys, MINUS anything also in `entry_paths` — a path a package
    /// both runs (or ships) and invokes from a build script is the product, and that role dominates.
    /// Disjoint from `entry_paths` by construction, so the union below never double-counts and the
    /// deployment-role classification downstream can never call one path two things.
    pub script_paths: std::collections::HashSet<String>,
    pub workspace_pkgs: std::collections::HashMap<String, zzop_parser_typescript::WorkspacePkg>,
}

impl PackageJsonScan {
    /// Every path any `package.json` field named, both roles together — the set `dead-candidates` seeds
    /// as `extra_entries` (see [`crate::analyze`]'s `assemble::rules::entries`). Byte-for-byte the set
    /// the pre-split single field held: the split is a classification, never a filter.
    pub(crate) fn all_entry_paths(&self) -> std::collections::HashSet<String> {
        self.entry_paths
            .union(&self.script_paths)
            .cloned()
            .collect()
    }

    /// [`Self::script_paths`] as a sorted `Vec` — the wire form (`AnalyzeOutput::build_script_paths`).
    /// Sorted HERE, at the one place the set becomes a sequence: `HashSet` iteration order is not stable
    /// across runs, and `output-philosophy.md` §6 makes byte-identical output for the same config a
    /// published contract, so every surface has to get the ordered form and none may re-derive it.
    pub(crate) fn sorted_script_paths(&self) -> Vec<String> {
        let mut v: Vec<String> = self.script_paths.iter().cloned().collect();
        v.sort();
        v
    }
}

/// Collects file paths referenced by any `package.json` found during the walk that should be treated as
/// entry-like regardless of `fan_in` (`find_dead_candidates`'s `extra_entries`): manifest entry fields
/// (`main`/`module`/`bin`/`exports`) and lexically-scanned `scripts` path tokens — kept APART by
/// deployment role in the returned [`PackageJsonScan`] (see its doc), with the `scripts` half itself split
/// by KEY so a run-lifecycle target lands on the entry side, and unioned back by
/// [`PackageJsonScan::all_entry_paths`]. `all_paths` is the TS-dispatched universe used to resolve an
/// extensionless/compiled manifest value via `zzop_parser_typescript::try_ext`.
///
/// Also collects each manifest's `name` into `PackageJsonScan::workspace_pkgs` (own directory, plus a
/// resolved bare-specifier entry tried in Node's own order: `main`, `module`, `exports["."]`, then a
/// conventional `index.*` file; `entry` stays `None` when nothing resolves) — same loop/read, not a
/// second walk.
///
/// Degrades gracefully on every failure mode — never panics. Each manifest's candidates resolve
/// relative to its own directory, not `root`; an unresolvable candidate is simply dropped.
pub(crate) fn package_json_entries(
    root: &Path,
    node_paths: impl Iterator<Item = String>,
    all_paths: &std::collections::HashSet<String>,
) -> PackageJsonScan {
    let mut entry_paths = std::collections::HashSet::new();
    let mut script_paths = std::collections::HashSet::new();
    let mut workspace_pkgs = std::collections::HashMap::new();
    for rel in node_paths.filter(|p| is_package_json_path(p)) {
        let Ok(text) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let dir = package_json_dir(&rel);
        let mut candidates: Vec<String> = Vec::new();
        for key in ["main", "module"] {
            if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
                candidates.push(s.to_string());
            }
        }
        match value.get("bin") {
            Some(serde_json::Value::String(s)) => candidates.push(s.clone()),
            Some(serde_json::Value::Object(map)) => {
                for v in map.values() {
                    if let Some(s) = v.as_str() {
                        candidates.push(s.to_string());
                    }
                }
            }
            _ => {}
        }
        if let Some(exports) = value.get("exports") {
            collect_export_path_strings(exports, &mut candidates);
        }
        // `scripts` splits by KEY, not as a block. A token named by an npm RUN-lifecycle key is the
        // package's run entry and joins `candidates` beside `main`/`bin`/`exports`; every other key's
        // tokens go to `script_candidates`, the build-surface role. See
        // `manifest::is_run_lifecycle_script_key` for the key set, the measurement behind it, and both
        // residuals. Both lists resolve through the same `try_ext` walk immediately below.
        let mut script_candidates: Vec<String> = Vec::new();
        if let Some(serde_json::Value::Object(scripts)) = value.get("scripts") {
            for (key, cmd) in scripts
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k, s)))
            {
                let run_entry = is_run_lifecycle_script_key(key);
                for tok in cmd.split_whitespace() {
                    if looks_like_script_path_token(tok) {
                        if run_entry {
                            candidates.push(tok.to_string());
                        } else {
                            script_candidates.push(tok.to_string());
                        }
                    }
                }
            }
        }
        for candidate in &candidates {
            let normalized = join_and_normalize(dir, candidate);
            if let Some(resolved) = zzop_parser_typescript::try_ext(&normalized, all_paths) {
                entry_paths.insert(resolved);
            }
        }
        for candidate in &script_candidates {
            let normalized = join_and_normalize(dir, candidate);
            if let Some(resolved) = zzop_parser_typescript::try_ext(&normalized, all_paths) {
                script_paths.insert(resolved);
            }
        }

        if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
            let mut entry_candidates: Vec<String> = Vec::new();
            for key in ["main", "module"] {
                if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
                    entry_candidates.push(s.to_string());
                }
            }
            if let Some(exports) = value.get("exports") {
                collect_exports_dot_entry(exports, &mut entry_candidates);
            }
            for fallback in ["index.ts", "index.tsx", "src/index.ts", "src/index.tsx"] {
                entry_candidates.push(fallback.to_string());
            }
            let entry = entry_candidates.iter().find_map(|candidate| {
                let normalized = join_and_normalize(dir, candidate);
                zzop_parser_typescript::try_ext(&normalized, all_paths)
            });
            workspace_pkgs.insert(
                name.to_string(),
                zzop_parser_typescript::WorkspacePkg {
                    dir: dir.to_string(),
                    entry,
                },
            );
        }
    }
    // The package's own code dominates: subtracted ONCE here, over the fully accumulated sets, never per
    // manifest — in a monorepo the entry field and the build-script token that name the same file
    // routinely sit in DIFFERENT `package.json` files, and a per-manifest subtraction would leave the
    // collision standing whenever the scripts manifest was read first. This is also what makes the
    // run-lifecycle rescue compose: `"start": "ts-node src/index.ts"` beside
    // `"dev": "nodemon --exec 'ts-node src/index.ts'"` puts the same path in both lists, and the file is
    // the product, so the run role wins.
    script_paths.retain(|p| !entry_paths.contains(p));
    PackageJsonScan {
        entry_paths,
        script_paths,
        workspace_pkgs,
    }
}

#[cfg(test)]
mod tests;
