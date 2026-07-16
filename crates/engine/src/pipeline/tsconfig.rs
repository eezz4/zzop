//! tsconfig `paths`/`baseUrl` alias collection.
//!
//! `tsconfig_scan` is this engine's filesystem-touching collection pass; the pure resolver logic it
//! feeds lives in `zzop_parser_typescript::resolve` instead (no I/O there).

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::manifest::{
    is_tsconfig_json_path, join_and_normalize, package_json_dir, strip_jsonc_comments,
};

/// One tsconfig file's own (unmerged, un-`extends`-resolved) `compilerOptions.baseUrl`/`paths` +
/// `extends` target, as written — `tsconfig_scan` resolves/merges `extends` and joins `baseUrl`
/// against the file's own directory.
struct RawTsconfig {
    base_url: Option<String>,
    paths: std::collections::BTreeMap<String, Vec<String>>,
    extends: Option<String>,
}

/// Parses one tsconfig file's text (JSONC-tolerant) into its own `compilerOptions.baseUrl`/`paths`/
/// `extends`, un-merged. `None` on any parse failure — `tsconfig_scan` degrades by skipping that file.
fn parse_raw_tsconfig(text: &str) -> Option<RawTsconfig> {
    static TRAILING_COMMA: OnceLock<Regex> = OnceLock::new();
    let stripped = strip_jsonc_comments(text);
    let cleaned = TRAILING_COMMA
        .get_or_init(|| Regex::new(r",(\s*[}\]])").unwrap())
        .replace_all(&stripped, "$1");
    let value: serde_json::Value = serde_json::from_str(&cleaned).ok()?;
    let extends = value
        .get("extends")
        .and_then(|v| v.as_str())
        .map(String::from);
    let compiler_options = value.get("compilerOptions");
    let base_url = compiler_options
        .and_then(|c| c.get("baseUrl"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let mut paths = std::collections::BTreeMap::new();
    if let Some(map) = compiler_options
        .and_then(|c| c.get("paths"))
        .and_then(|v| v.as_object())
    {
        for (pattern, targets) in map {
            if let Some(arr) = targets.as_array() {
                let targets: Vec<String> = arr
                    .iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect();
                if !targets.is_empty() {
                    paths.insert(pattern.clone(), targets);
                }
            }
        }
    }
    Some(RawTsconfig {
        base_url,
        paths,
        extends,
    })
}

/// Collects `compilerOptions.baseUrl`/`paths` from every `tsconfig.json` found during the same manifest
/// walk `package_json_entries` uses, keyed by the tsconfig's own directory (the directory a TypeScript
/// file's nearest ancestor tsconfig governs, per `zzop_parser_typescript::resolve::governing_tsconfig`).
///
/// `extends` handling is minimal: only a local relative target is followed, exactly one level, merged
/// parent-fills-gaps (child's `paths` keys win; `baseUrl` is the child's if set, else the parent's). A
/// second-level or non-local `extends` is left unresolved.
///
/// A directory whose merged tsconfig declares neither `baseUrl` nor `paths` is not registered, so
/// `governing_tsconfig`'s ancestor walk continues past it. Degrades gracefully on every failure mode —
/// never panics.
pub(crate) fn tsconfig_scan(
    root: &Path,
    node_paths: impl Iterator<Item = String>,
) -> std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths> {
    let mut result = std::collections::BTreeMap::new();
    for rel in node_paths.filter(|p| is_tsconfig_json_path(p)) {
        let Ok(text) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        let Some(raw) = parse_raw_tsconfig(&text) else {
            continue;
        };
        let dir = package_json_dir(&rel);

        let mut paths = raw.paths;
        let mut base_url_raw = raw.base_url;
        if let Some(extends) = &raw.extends {
            if extends.starts_with("./") || extends.starts_with("../") {
                let mut parent_rel = join_and_normalize(dir, extends);
                if !parent_rel.ends_with(".json") {
                    parent_rel.push_str(".json");
                }
                if let Ok(parent_text) = fs::read_to_string(root.join(&parent_rel)) {
                    if let Some(parent_raw) = parse_raw_tsconfig(&parent_text) {
                        // Child keys win; any key only the parent declares is kept (parent-fills-gaps).
                        // `parent_raw.extends` (a 2nd extends level) is intentionally not chased further.
                        let mut merged = parent_raw.paths;
                        merged.extend(paths);
                        paths = merged;
                        if base_url_raw.is_none() {
                            base_url_raw = parent_raw.base_url;
                        }
                    }
                }
            }
        }

        if paths.is_empty() && base_url_raw.is_none() {
            continue;
        }
        let base_url = match &base_url_raw {
            Some(b) => join_and_normalize(dir, b),
            None => dir.to_string(),
        };
        result.insert(
            dir.to_string(),
            zzop_parser_typescript::TsconfigPaths { base_url, paths },
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::testutil::TempDir;

    #[test]
    fn tsconfig_scan_collects_star_pattern_and_base_url() {
        let dir = TempDir::new("zzop-tsconfig-star");
        dir.write(
            "tsconfig.json",
            r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["./src/*"]}}}"#,
        );
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        let cfg = scan.get("").unwrap();
        assert_eq!(cfg.base_url, "");
        assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
    }

    #[test]
    fn tsconfig_scan_registers_under_own_directory_not_root() {
        let dir = TempDir::new("zzop-tsconfig-nested-dir");
        dir.write(
            "packages/app/tsconfig.json",
            r#"{"compilerOptions": {"baseUrl": "src"}}"#,
        );
        let scan = tsconfig_scan(
            dir.path(),
            std::iter::once("packages/app/tsconfig.json".to_string()),
        );
        assert!(scan.contains_key("packages/app"));
        assert_eq!(
            scan.get("packages/app").unwrap().base_url,
            "packages/app/src"
        );
    }

    #[test]
    fn tsconfig_scan_follows_one_level_of_local_extends_and_merges() {
        let dir = TempDir::new("zzop-tsconfig-extends");
        dir.write(
            "tsconfig.base.json",
            r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@shared/*": ["./shared/*"], "@app/*": ["./old-app/*"]}}}"#,
        );
        dir.write(
            "tsconfig.json",
            r#"{"extends": "./tsconfig.base.json", "compilerOptions": {"paths": {"@app/*": ["./src/*"]}}}"#,
        );
        let scan = tsconfig_scan(
            dir.path(),
            vec![
                "tsconfig.json".to_string(),
                "tsconfig.base.json".to_string(),
            ]
            .into_iter(),
        );
        let cfg = scan.get("").unwrap();
        // Child's `@app/*` overrides the parent's; parent-only `@shared/*` is kept (parent-fills-gaps); the
        // parent's `baseUrl` is inherited since the child doesn't declare its own.
        assert_eq!(
            cfg.paths.get("@app/*").unwrap(),
            &vec!["./src/*".to_string()]
        );
        assert_eq!(
            cfg.paths.get("@shared/*").unwrap(),
            &vec!["./shared/*".to_string()]
        );
        assert_eq!(cfg.base_url, "");
    }

    #[test]
    fn tsconfig_scan_ignores_non_local_extends() {
        let dir = TempDir::new("zzop-tsconfig-extends-pkg");
        dir.write(
            "tsconfig.json",
            r#"{"extends": "@tsconfig/node18/tsconfig.json", "compilerOptions": {"paths": {"@/*": ["./src/*"]}}}"#,
        );
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        // The non-local `extends` target is never read (no such file exists here); the tsconfig's own
        // `compilerOptions` still register normally.
        let cfg = scan.get("").unwrap();
        assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
    }

    #[test]
    fn tsconfig_scan_tolerates_jsonc_comments_and_trailing_commas() {
        let dir = TempDir::new("zzop-tsconfig-jsonc");
        dir.write(
            "tsconfig.json",
            r#"{
                // line comment
                "compilerOptions": {
                    /* block comment */
                    "baseUrl": ".",
                    "paths": {
                        "@/*": ["./src/*"],
                    },
                },
            }"#,
        );
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        let cfg = scan.get("").unwrap();
        assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
    }

    #[test]
    fn tsconfig_scan_skips_directory_with_neither_base_url_nor_paths() {
        let dir = TempDir::new("zzop-tsconfig-empty");
        dir.write("tsconfig.json", r#"{"compilerOptions": {"strict": true}}"#);
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        assert!(scan.is_empty());
    }

    #[test]
    fn tsconfig_scan_degrades_on_invalid_json() {
        let dir = TempDir::new("zzop-tsconfig-invalid");
        dir.write("tsconfig.json", "{ this is not json");
        let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
        assert!(scan.is_empty());
    }
}
