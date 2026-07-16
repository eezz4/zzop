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
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
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

// --- `parse_dsl_pack` — the extracted per-file verdict `load_dsl_packs` and the pre-load
// --- validator (`validate_rule_pack`) both consume ---

#[test]
fn parse_dsl_pack_accepts_a_valid_pack() {
    let pack = parse_dsl_pack(&valid_pack("p")).expect("valid pack must parse");
    assert_eq!(pack.id, "p");
    assert_eq!(pack.rules.len(), 1);
}

#[test]
fn parse_dsl_pack_reports_missing_fields_and_bad_json_as_serde_messages() {
    let err = parse_dsl_pack("{ not json").unwrap_err();
    assert!(
        err.contains("key must be a string") || err.contains("expected"),
        "{err}"
    );

    // Missing required `rules` field — the exact serde message the loader would surface.
    let err = parse_dsl_pack(r#"{"id": "p"}"#).unwrap_err();
    assert!(err.contains("missing field `rules`"), "{err}");

    // Wrong type for `severity`.
    let wrong_type = valid_pack("p").replace("\"warning\"", "42");
    let err = parse_dsl_pack(&wrong_type).unwrap_err();
    assert!(err.contains("expected"), "{err}");
}

#[test]
fn parse_dsl_pack_applies_the_schema_version_gate() {
    let err = parse_dsl_pack(&pack_with_schema_version("p", 999)).unwrap_err();
    assert!(err.contains("newer DSL schema"), "{err}");
    assert!(err.contains("999"), "{err}");
}

// --- `pack_regex_issues` — the eval-time "invalid regex silently no-ops the rule" judgment,
// --- surfaced as named issues ---

#[test]
fn pack_regex_issues_is_empty_for_a_pack_whose_patterns_all_compile() {
    let pack = parse_dsl_pack(&valid_pack("p")).unwrap();
    assert!(pack_regex_issues(&pack).is_empty());
}

#[test]
fn pack_regex_issues_names_the_rule_and_field_of_a_bad_regex() {
    let bad = valid_pack("p").replace(
        r#""line_pattern": "TODO""#,
        r#""line_pattern": "(unclosed""#,
    );
    let pack = parse_dsl_pack(&bad).unwrap();
    let issues = pack_regex_issues(&pack);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].contains("rule \"r1\""), "{issues:?}");
    assert!(issues[0].contains("`line_pattern`"), "{issues:?}");
    assert!(issues[0].contains("never fire"), "{issues:?}");
}

#[test]
fn pack_regex_issues_walks_every_matcher_kind() {
    let pack_json = r#"{
        "id": "p",
        "rules": [
            {"id": "ls", "severity": "info", "message": "m",
             "matcher": {"type": "line-scan", "file_pattern": "(bad", "line_pattern": "ok",
                         "any": [{"pattern": "(worse", "label": "l"}]}},
            {"id": "ms", "severity": "info", "message": "m",
             "matcher": {"type": "method-scan", "file_pattern": "ok",
                         "patterns": [{"pattern": "(bad", "label": "t"}], "trigger": "t"}},
            {"id": "ss", "severity": "info", "message": "m",
             "matcher": {"type": "symbol-scan", "file_pattern": "ok", "name_pattern": "(bad"}},
            {"id": "is", "severity": "info", "message": "m",
             "matcher": {"type": "io-scan", "file_pattern": "ok", "direction": "any", "key_pattern": "(bad"}}
        ]
    }"#;
    let pack = parse_dsl_pack(pack_json).unwrap();
    let issues = pack_regex_issues(&pack);
    let text = issues.join("\n");
    assert!(text.contains("rule \"ls\": `file_pattern`"), "{text}");
    assert!(text.contains("rule \"ls\": `any[].pattern`"), "{text}");
    assert!(text.contains("rule \"ms\": `patterns[].pattern`"), "{text}");
    assert!(text.contains("rule \"ss\": `name_pattern`"), "{text}");
    assert!(text.contains("rule \"is\": `key_pattern`"), "{text}");
    assert_eq!(issues.len(), 5, "{issues:?}");
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
