//! DSL rule-pack loader — reads every `rules/dsl/*.json` under a directory into `RulePackDef`, plus the
//! per-pack file-path pre-filter that decides whether a pack has any rule that could ever fire against a
//! given file.
//!
//! Two directory shapes are supported, and may be mixed in the same directory: flat (`<dir>/<id>.json`,
//! what an external/user `packsDir` typically uses) and depth-1 nested (`<dir>/<name>/<id>.json`, the
//! "co-located pack folder" layout this repo's own first-party packs ship in — see `rules/README.md`).
//! Both are valid; nesting is never required.
//!
//! ## Where "appliesTo" lives (design call)
//! Gating a whole pack on the TARGET environment (fe/be/ext-chrome/...) belongs on `RulePackDef` (dsl.rs)'s
//! `framework` field ("any" | "react" | "prisma" | ...) — and that's exactly what `RuleMeta::applies_to`
//! (registry.rs) already gates on for every rule layer uniformly. `RulePackDef` does NOT carry a file-path
//! / language-extension
//! field at the pack level, though: file-path gating lives PER RULE, inside its matcher
//! (`Matcher::{LineScan,MethodScan}.file_pattern`) — a single pack can mix, say, a `.java` rule and a `.jsp`
//! rule. So `applies_to` below is a narrower, additional pre-filter: "does at least one rule in this pack
//! even look at files shaped like `file_path`" — useful for a caller that wants to skip considering a pack
//! entirely for a tree of files none of its rules could ever match. It is NOT a substitute for the
//! framework/target gating, which stays on `RuleMeta::applies_to`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::dsl::{Matcher, RulePackDef};

/// The highest `RulePackDef::schema_version` this engine build understands (see `docs/rules/dsl-reference.md`'s
/// "Schema version policy"). A pack declaring a higher version depends on core IR/matcher shapes
/// this build predates — loading it anyway would silently misinterpret fields this engine has never seen
/// (or, worse, seen with a different meaning), so `load_dsl_packs` rejects it outright as a per-file
/// `PackLoadError` instead. Bump this constant only when a genuinely new, incompatible-with-older-builds
/// DSL schema revision ships; ordinary additive changes (new optional matcher fields with
/// `#[serde(default)]`) do not need a bump — older packs already deserialize fine against them, and this
/// constant only gates the other direction (a pack newer than the engine).
pub const SUPPORTED_DSL_SCHEMA_VERSION: u32 = 1;

/// One `*.json` file under the pack directory that failed to read, deserialize, or pass the schema-version
/// gate — a per-file error, not a panic: a single malformed/too-new pack must not take down every other
/// pack in the directory.
#[derive(Debug, Clone)]
pub struct PackLoadError {
    pub path: PathBuf,
    pub message: String,
}

/// Result of scanning a directory of DSL rule packs. `packs` is sorted by full path for determinism
/// (registration/evaluation order must not depend on OS directory-iteration order, and must stay
/// deterministic across a mix of flat and depth-1-nested pack files); `errors` holds one entry per file
/// that failed to read or parse.
#[derive(Debug, Default)]
pub struct LoadResult {
    pub packs: Vec<(PathBuf, RulePackDef)>,
    pub errors: Vec<PackLoadError>,
}

/// Reads every `*.json` file directly under `dir`, PLUS every `*.json` file one level down inside a
/// subdirectory of `dir` (`<dir>/<name>/*.json`) — see the module doc for the two supported shapes — and
/// deserializes each into a `RulePackDef`. Only one level of subdirectory is scanned (a
/// sub-subdirectory's `*.json` is not found): deliberately shallow, matching "one folder per pack" rather
/// than an arbitrary recursive tree. Directory-read failure (missing/unreadable dir) is reported as a
/// single error entry (path = `dir`) rather than a panic, same "surface, don't crash" contract as a
/// malformed pack file.
pub fn load_dsl_packs(dir: &Path) -> LoadResult {
    let mut result = LoadResult::default();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            result.errors.push(PackLoadError {
                path: dir.to_path_buf(),
                message: err.to_string(),
            });
            return result;
        }
    };

    let mut paths: Vec<PathBuf> = Vec::new();
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json") {
            paths.push(p);
        } else if p.is_dir() {
            subdirs.push(p);
        }
    }
    for sub in subdirs {
        if let Ok(sub_entries) = fs::read_dir(&sub) {
            for entry in sub_entries.filter_map(Result::ok) {
                let p = entry.path();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json") {
                    paths.push(p);
                }
            }
        }
    }
    // Sort by full path (not just file name) so load order is deterministic across BOTH the flat and
    // nested shapes, regardless of directory-listing order.
    paths.sort();

    for path in paths {
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<RulePackDef>(&text) {
                Ok(pack) if pack.schema_version > SUPPORTED_DSL_SCHEMA_VERSION => {
                    result.errors.push(PackLoadError {
                        path,
                        message: format!(
                            "pack requires newer DSL schema (schema_version {}, this engine supports up to {})",
                            pack.schema_version, SUPPORTED_DSL_SCHEMA_VERSION
                        ),
                    });
                }
                Ok(pack) => result.packs.push((path, pack)),
                Err(err) => result.errors.push(PackLoadError {
                    path,
                    message: err.to_string(),
                }),
            },
            Err(err) => result.errors.push(PackLoadError {
                path,
                message: err.to_string(),
            }),
        }
    }
    result
}

/// True if at least one rule in `pack` has a matcher whose `file_pattern` matches `file_path` — see the
/// module doc for why this is a per-rule pre-filter, not a whole-pack `appliesTo`. A rule whose
/// `file_pattern` fails to compile as a regex is treated as non-matching (mirrors `eval_pack`, which
/// already no-ops a rule with an invalid pattern rather than panicking).
pub fn applies_to(pack: &RulePackDef, file_path: &str) -> bool {
    pack.rules.iter().any(|rule| {
        let pattern = match &rule.matcher {
            Matcher::LineScan(m) => Some(&m.file_pattern),
            Matcher::MethodScan(m) => Some(&m.file_pattern),
            // Non-exhaustive on purpose: a future matcher kind without a `file_pattern` (or one this
            // module doesn't know about yet) is conservatively treated as "could match" rather than
            // silently excluding the pack — a false "applies" only costs an extra (skippable) pack
            // consideration, whereas a false "doesn't apply" would hide real findings.
            #[allow(unreachable_patterns)]
            _ => None,
        };
        match pattern {
            Some(p) => regex::Regex::new(p)
                .map(|re| re.is_match(file_path))
                .unwrap_or(false),
            None => true,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A self-cleaning temp directory (std-only mkdtemp equivalent — no `tempfile` crate dependency in this
    /// workspace). Same pattern as `schema_usage.rs`'s test-local `TempDir` (duplicated here rather than
    /// shared, since it is a private `#[cfg(test)]` helper with no public home to import from).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, rel: &str, content: &str) {
            let full = self.0.join(rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn valid_pack(id: &str) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "framework": "any",
                "rules": [
                    {{
                        "id": "r1",
                        "severity": "warning",
                        "message": "msg",
                        "matcher": {{
                            "type": "line-scan",
                            "file_pattern": "\\.java$",
                            "line_pattern": "TODO"
                        }}
                    }}
                ]
            }}"#
        )
    }

    /// Same as `valid_pack` but with an explicit `schema_version` field, for schema-gate tests.
    fn pack_with_schema_version(id: &str, schema_version: u32) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "framework": "any",
                "schema_version": {schema_version},
                "rules": [
                    {{
                        "id": "r1",
                        "severity": "warning",
                        "message": "msg",
                        "matcher": {{
                            "type": "line-scan",
                            "file_pattern": "\\.java$",
                            "line_pattern": "TODO"
                        }}
                    }}
                ]
            }}"#
        )
    }

    // --- schema_version gate (see `docs/rules/dsl-reference.md`'s "Schema version policy") ---

    #[test]
    fn pack_missing_schema_version_defaults_to_1_and_loads() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("p.json", &valid_pack("p")); // no schema_version field at all
        let result = load_dsl_packs(dir.path());
        assert!(result.errors.is_empty());
        assert_eq!(result.packs.len(), 1);
        assert_eq!(result.packs[0].1.schema_version, 1);
    }

    #[test]
    fn pack_with_explicit_schema_version_1_loads() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("p.json", &pack_with_schema_version("p", 1));
        let result = load_dsl_packs(dir.path());
        assert!(result.errors.is_empty());
        assert_eq!(result.packs.len(), 1);
        assert_eq!(result.packs[0].1.schema_version, 1);
    }

    #[test]
    fn pack_requiring_a_newer_schema_is_rejected_as_a_load_error_not_a_panic() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("good.json", &valid_pack("good"));
        dir.write("too-new.json", &pack_with_schema_version("too-new", 999));
        let result = load_dsl_packs(dir.path());
        assert_eq!(result.packs.len(), 1);
        assert_eq!(result.packs[0].1.id, "good");
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].path.ends_with("too-new.json"));
        assert!(result.errors[0].message.contains("newer DSL schema"));
    }

    #[test]
    fn loads_every_json_file_sorted_by_name() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("b-pack.json", &valid_pack("b-pack"));
        dir.write("a-pack.json", &valid_pack("a-pack"));
        let result = load_dsl_packs(dir.path());
        assert!(result.errors.is_empty());
        assert_eq!(result.packs.len(), 2);
        assert_eq!(result.packs[0].1.id, "a-pack");
        assert_eq!(result.packs[1].1.id, "b-pack");
    }

    #[test]
    fn discovers_packs_in_depth_one_subdirectories() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("flat.json", &valid_pack("flat"));
        dir.write("nested/nested.json", &valid_pack("nested"));
        let result = load_dsl_packs(dir.path());
        assert!(result.errors.is_empty());
        let mut ids: Vec<&str> = result.packs.iter().map(|(_, p)| p.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["flat", "nested"]);
    }

    #[test]
    fn does_not_recurse_past_one_level_of_subdirectory() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("nested/deeper/too-deep.json", &valid_pack("too-deep"));
        let result = load_dsl_packs(dir.path());
        assert!(result.errors.is_empty());
        assert!(result.packs.is_empty());
    }

    #[test]
    fn load_order_is_deterministic_across_mixed_flat_and_nested_layout() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("z-flat.json", &valid_pack("z-flat"));
        dir.write("a-nested/a-nested.json", &valid_pack("a-nested"));
        let first = load_dsl_packs(dir.path());
        let second = load_dsl_packs(dir.path());
        let first_ids: Vec<&str> = first.packs.iter().map(|(_, p)| p.id.as_str()).collect();
        let second_ids: Vec<&str> = second.packs.iter().map(|(_, p)| p.id.as_str()).collect();
        assert_eq!(first_ids, second_ids);
        assert_eq!(first_ids.len(), 2);
    }

    #[test]
    fn ignores_non_json_files() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("pack.json", &valid_pack("p"));
        dir.write("readme.md", "not a pack");
        let result = load_dsl_packs(dir.path());
        assert_eq!(result.packs.len(), 1);
    }

    #[test]
    fn malformed_file_is_a_per_file_error_not_a_panic() {
        let dir = TempDir::new("zzop-pack-loader");
        dir.write("good.json", &valid_pack("good"));
        dir.write("bad.json", "{ not json");
        let result = load_dsl_packs(dir.path());
        assert_eq!(result.packs.len(), 1);
        assert_eq!(result.packs[0].1.id, "good");
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].path.ends_with("bad.json"));
    }

    #[test]
    fn missing_directory_is_reported_as_an_error_not_a_panic() {
        let result = load_dsl_packs(Path::new("/no/such/dir/zzop-nope"));
        assert!(result.packs.is_empty());
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn applies_to_matches_when_a_rule_file_pattern_matches() {
        let pack: RulePackDef = serde_json::from_str(&valid_pack("p")).unwrap();
        assert!(applies_to(&pack, "src/Foo.java"));
        assert!(!applies_to(&pack, "src/foo.ts"));
    }

    #[test]
    fn applies_to_is_false_for_a_pack_with_no_rules() {
        let pack = RulePackDef {
            id: "empty".into(),
            framework: "any".into(),
            schema_version: 1,
            rules: vec![],
        };
        assert!(!applies_to(&pack, "anything.java"));
    }
}
