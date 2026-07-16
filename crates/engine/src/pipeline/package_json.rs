//! `package.json` manifest scan: entry-like file collection + workspace-package map.

use std::fs;
use std::path::Path;

use super::manifest::{
    collect_export_path_strings, collect_exports_dot_entry, is_package_json_path,
    join_and_normalize, looks_like_script_path_token, package_json_dir,
};

/// `package_json_entries`' return: `extra_entries` plus `workspace_pkgs`, a `name -> WorkspacePkg` map
/// from the same manifest walk. The workspace-alias import resolver needs a directory to resolve
/// `<name>/subpath` specifiers and a resolved entry file to resolve a bare `<name>` specifier.
pub(crate) struct PackageJsonScan {
    pub extra_entries: std::collections::HashSet<String>,
    pub workspace_pkgs: std::collections::HashMap<String, zzop_parser_typescript::WorkspacePkg>,
}

/// Collects file paths referenced by any `package.json` found during the walk that should be treated as
/// entry-like regardless of `fan_in` (`find_dead_candidates`'s `extra_entries`): manifest entry fields
/// (`main`/`module`/`bin`/`exports`) and lexically-scanned `scripts` path tokens. `all_paths` is the
/// TS-dispatched universe used to resolve an extensionless/compiled manifest value via
/// `zzop_parser_typescript::try_ext`.
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
    let mut result = std::collections::HashSet::new();
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
        if let Some(serde_json::Value::Object(scripts)) = value.get("scripts") {
            for cmd in scripts.values().filter_map(|v| v.as_str()) {
                for tok in cmd.split_whitespace() {
                    if looks_like_script_path_token(tok) {
                        candidates.push(tok.to_string());
                    }
                }
            }
        }
        for candidate in &candidates {
            let normalized = join_and_normalize(dir, candidate);
            if let Some(resolved) = zzop_parser_typescript::try_ext(&normalized, all_paths) {
                result.insert(resolved);
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
    PackageJsonScan {
        extra_entries: result,
        workspace_pkgs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::testutil::TempDir;
    use std::collections::HashSet;

    #[test]
    fn package_json_entries_resolves_extensionless_or_js_main_via_try_ext() {
        let dir = TempDir::new("zzop-pkg-entries-main");
        dir.write("package.json", r#"{"main": "dist/index.js"}"#);
        let all_paths: HashSet<String> = ["dist/index.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_resolves_bin_object_with_multiple_entries() {
        let dir = TempDir::new("zzop-pkg-entries-bin");
        dir.write(
            "package.json",
            r#"{"bin": {"foo-cli": "./bin/foo.ts", "bar-cli": "./bin/bar.ts"}}"#,
        );
        let all_paths: HashSet<String> = ["bin/foo.ts".to_string(), "bin/bar.ts".to_string()]
            .into_iter()
            .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_resolves_nested_exports_and_ignores_condition_keys() {
        let dir = TempDir::new("zzop-pkg-entries-exports");
        dir.write(
            "package.json",
            r#"{
                "exports": {
                    ".": { "import": "./src/index.mts", "require": "./src/index.cts" },
                    "./sub": "./src/sub.ts"
                }
            }"#,
        );
        let all_paths: HashSet<String> = [
            "src/index.mts".to_string(),
            "src/index.cts".to_string(),
            "src/sub.ts".to_string(),
        ]
        .into_iter()
        .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_lexically_scans_scripts_for_path_tokens() {
        let dir = TempDir::new("zzop-pkg-entries-scripts");
        dir.write(
            "package.json",
            r#"{
                "scripts": {
                    "build": "tsc && node scripts/postbuild.js",
                    "test": "jest"
                }
            }"#,
        );
        let all_paths: HashSet<String> = ["scripts/postbuild.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("package.json".to_string()),
            &all_paths,
        );
        // "test": "jest" has no path-looking token — contributes nothing; "tsc" isn't a path either.
        assert_eq!(scan.extra_entries, all_paths);
    }

    #[test]
    fn package_json_entries_resolves_relative_to_own_directory_not_root() {
        let dir = TempDir::new("zzop-pkg-entries-nested");
        dir.write("packages/foo/package.json", r#"{"main": "./index.ts"}"#);
        let all_paths: HashSet<String> =
            ["packages/foo/index.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/foo/package.json".to_string()),
            &all_paths,
        );
        assert_eq!(scan.extra_entries, all_paths);
    }

    // --- PackageJsonScan::workspace_pkgs ---

    #[test]
    fn package_json_entries_collects_workspace_pkg_name_to_main_entry() {
        let dir = TempDir::new("zzop-pkg-entries-ws-main");
        dir.write(
            "packages/prisma/package.json",
            r#"{"name": "@acme/prisma", "main": "index.ts"}"#,
        );
        let all_paths: HashSet<String> = ["packages/prisma/index.ts".to_string()]
            .into_iter()
            .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/prisma/package.json".to_string()),
            &all_paths,
        );
        let pkg = scan.workspace_pkgs.get("@acme/prisma").unwrap();
        assert_eq!(pkg.dir, "packages/prisma");
        assert_eq!(pkg.entry.as_deref(), Some("packages/prisma/index.ts"));
    }

    #[test]
    fn package_json_entries_falls_back_to_index_ts_when_no_main_module_exports() {
        let dir = TempDir::new("zzop-pkg-entries-ws-index-fallback");
        dir.write("packages/lib/package.json", r#"{"name": "@acme/lib"}"#);
        dir.write("packages/lib/index.ts", "export {};\n");
        let all_paths: HashSet<String> =
            ["packages/lib/index.ts".to_string()].into_iter().collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/lib/package.json".to_string()),
            &all_paths,
        );
        let pkg = scan.workspace_pkgs.get("@acme/lib").unwrap();
        assert_eq!(pkg.entry.as_deref(), Some("packages/lib/index.ts"));
    }

    #[test]
    fn package_json_entries_workspace_pkg_entry_none_when_nothing_resolves() {
        // A pure sub-path-only package with no entry point: no `main`/`module`/`exports`, no root
        // `index.ts` — every import of it names a sub-path. `entry` staying `None` (rather than some
        // guessed path) is the honest signal.
        let dir = TempDir::new("zzop-pkg-entries-ws-no-entry");
        dir.write("packages/lib/package.json", r#"{"name": "@acme/lib"}"#);
        dir.write("packages/lib/tracking.ts", "export {};\n");
        let all_paths: HashSet<String> = ["packages/lib/tracking.ts".to_string()]
            .into_iter()
            .collect();
        let scan = package_json_entries(
            dir.path(),
            std::iter::once("packages/lib/package.json".to_string()),
            &all_paths,
        );
        let pkg = scan.workspace_pkgs.get("@acme/lib").unwrap();
        assert_eq!(pkg.dir, "packages/lib");
        assert_eq!(pkg.entry, None);
    }
}
