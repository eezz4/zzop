//! Unit tests for DSL-pack loading/merging through `analyze` and `analyzeEnvelope` (`packsDir`
//! shapes, `packDefs`, same-id collision rules).

use crate::test_support::{
    cycle_fixture, dsl_pack_json, envelope_with_symbols, symbol_scan_pack_json, TempDir,
};
use crate::{analyze_envelope_json, analyze_json, AnalyzeRequest, EnvelopeAnalyzeRequest};

#[test]
fn analyze_json_reports_a_bad_packs_dir_as_a_warning_not_a_failure() {
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "packsDir": {:?}}}"#,
        dir.path().display(),
        dir.path().join("no-such-dir").display()
    );
    let out = analyze_json(&config).expect("analyze_json should still succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("packs_dir")));
}

#[test]
fn analyze_json_empty_string_packs_dir_is_answered_by_name_and_skipped() {
    // `packsDir: ""` is a caller error answered by name — never handed to the loader (which would
    // emit a confusing `failed to load : (os error ...)` for the empty path).
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}, "packsDir": ""}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should still succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("packs_dir entry is an empty string")),
        "expected the empty-string entry named as the caller error, got: {value}"
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("failed to load")),
        "the empty entry must be skipped before the loader, got: {value}"
    );
}

#[test]
fn analyze_envelope_json_empty_string_packs_dir_is_answered_by_name_and_skipped() {
    // Same contract through the envelope entry point — the shared `base_engine_config` chokepoint
    // covers both, and this pins that the envelope path really routes the same way.
    let envelope = envelope_with_symbols(&["SomeName"]);
    let config = r#"{"sourceId": "legacy", "packsDir": ""}"#;
    let out = analyze_envelope_json(&envelope, config)
        .expect("analyze_envelope_json should still succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("packs_dir entry is an empty string")),
        "expected the empty-string entry named as the caller error, got: {value}"
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("failed to load")),
        "the empty entry must be skipped before the loader, got: {value}"
    );
}

#[test]
fn analyze_json_packs_dir_array_loads_and_merges_every_directory() {
    let dir = cycle_fixture();
    dir.write("marker.ts", "// MARKER_A\n// MARKER_B\n");

    let packs_a = TempDir::new("zzop-facade-packs-a");
    packs_a.write("pack-a.json", &dsl_pack_json("pack-a", "r1", "MARKER_A"));
    let packs_b = TempDir::new("zzop-facade-packs-b");
    packs_b.write("pack-b.json", &dsl_pack_json("pack-b", "r1", "MARKER_B"));

    let config = format!(
        r#"{{"root": {:?}, "packsDir": [{:?}, {:?}]}}"#,
        dir.path().display(),
        packs_a.path().display(),
        packs_b.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "pack-a/r1"),
        "expected pack-a's rule to fire, got: {value}"
    );
    assert!(
        findings.iter().any(|f| f["ruleId"] == "pack-b/r1"),
        "expected pack-b's rule to fire, got: {value}"
    );
}

#[test]
fn analyze_json_packs_dir_array_same_pack_id_later_directory_wins_whole_pack() {
    let dir = cycle_fixture();
    dir.write("marker.ts", "// OLD_MARKER\n// NEW_MARKER\n");

    let packs_old = TempDir::new("zzop-facade-packs-old");
    packs_old.write(
        "custom.json",
        &dsl_pack_json("custom", "marker-old", "OLD_MARKER"),
    );
    let packs_new = TempDir::new("zzop-facade-packs-new");
    packs_new.write(
        "custom.json",
        &dsl_pack_json("custom", "marker-new", "NEW_MARKER"),
    );

    let config = format!(
        r#"{{"root": {:?}, "packsDir": [{:?}, {:?}]}}"#,
        dir.path().display(),
        packs_old.path().display(),
        packs_new.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "custom/marker-new"),
        "expected the later directory's rule to fire, got: {value}"
    );
    assert!(
        !findings.iter().any(|f| f["ruleId"] == "custom/marker-old"),
        "expected the earlier directory's same-id pack to be fully replaced (not merged), got: {value}"
    );
}

#[test]
fn analyze_json_packs_dir_array_bad_entry_warns_but_other_entries_still_load() {
    let dir = cycle_fixture();
    dir.write("marker.ts", "// MARKER_A\n");

    let packs_good = TempDir::new("zzop-facade-packs-good");
    packs_good.write("pack-a.json", &dsl_pack_json("pack-a", "r1", "MARKER_A"));
    let bad_dir = dir.path().join("no-such-packs-dir");

    let config = format!(
        r#"{{"root": {:?}, "packsDir": [{:?}, {:?}]}}"#,
        dir.path().display(),
        bad_dir.display(),
        packs_good.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should still succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("packs_dir")),
        "expected a packs_dir warning for the bad directory, got: {value}"
    );
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "pack-a/r1"),
        "expected pack-a's rule to still fire despite the bad directory, got: {value}"
    );
}

#[test]
fn analyze_json_pack_defs_inline_pack_fires_without_packs_dir() {
    // No `packsDir` at all — the inline `packDefs` pack must load and fire on its own.
    let dir = cycle_fixture();
    dir.write("marker.ts", "// MARKER_A\n");

    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}]}}"#,
        dir.path().display(),
        dsl_pack_json("pack-a", "r1", "MARKER_A")
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "pack-a/r1"),
        "expected the inline packDefs rule to fire without any packsDir, got: {value}"
    );
}

#[test]
fn analyze_json_pack_defs_collision_with_packs_dir_directory_pack_wins() {
    // Same pack id from both an inline `packDefs` entry and a `packsDir` directory: the directory
    // pack loads AFTER `pack_defs` in `base_engine_config`'s seed order, so it must win the
    // collision whole — same "later wins whole" rule, applied across the two layers.
    let dir = cycle_fixture();
    dir.write("marker.ts", "// INLINE_MARKER\n// DIR_MARKER\n");

    let packs_dir_pack = TempDir::new("zzop-facade-packs-dir-collision");
    packs_dir_pack.write(
        "custom.json",
        &dsl_pack_json("custom", "marker-dir", "DIR_MARKER"),
    );

    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}], "packsDir": {:?}}}"#,
        dir.path().display(),
        dsl_pack_json("custom", "marker-inline", "INLINE_MARKER"),
        packs_dir_pack.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "custom/marker-dir"),
        "expected the packsDir directory pack's rule to fire (a directory pack beats an inline def \
         with the same id), got: {value}"
    );
    assert!(
        !findings
            .iter()
            .any(|f| f["ruleId"] == "custom/marker-inline"),
        "expected the inline packDefs pack to be fully replaced by the packsDir directory pack, \
         got: {value}"
    );
}

#[test]
fn analyze_json_pack_defs_same_id_later_def_wins() {
    // Two inline `packDefs` entries sharing an id: the later array entry must replace the earlier
    // one whole, mirroring the packsDir array's own same-id collision rule.
    let dir = cycle_fixture();
    dir.write("marker.ts", "// OLD_MARKER\n// NEW_MARKER\n");

    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}, {}]}}"#,
        dir.path().display(),
        dsl_pack_json("custom", "marker-old", "OLD_MARKER"),
        dsl_pack_json("custom", "marker-new", "NEW_MARKER")
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "custom/marker-new"),
        "expected the later inline def's rule to fire, got: {value}"
    );
    assert!(
        !findings.iter().any(|f| f["ruleId"] == "custom/marker-old"),
        "expected the earlier inline def with the same id to be fully replaced, got: {value}"
    );
}

// --- The DSL schema-version gate on inline `packDefs` (both entry points) ------------------------
// A `packDefs` entry never passes through the loader's text path (`parse_dsl_pack`), so
// `base_engine_config` re-applies the same gate at the seed chokepoint: a too-new pack is skipped
// with a by-name warning carrying the LOADER'S exact wording (`zzop_core::check_dsl_schema_version`
// — the same message `packsDir`/`validate_rule_pack` give the identical bytes), never run silently.

/// A structurally valid one-rule pack claiming a future DSL schema (`schema_version: 999`).
fn too_new_pack_json(pack_id: &str) -> String {
    format!(
        r#"{{
            "id": "{pack_id}",
            "schema_version": 999,
            "framework": "any",
            "rules": [
                {{
                    "id": "r1",
                    "severity": "warning",
                    "message": "msg",
                    "matcher": {{
                        "type": "symbol-scan",
                        "file_pattern": "\\.ts$",
                        "name_pattern": "^Bad"
                    }}
                }}
            ]
        }}"#
    )
}

/// Asserts the too-new-pack contract on one analysis output: the pack is named in a skip warning
/// whose wording matches the loader's own (pinned against `validate_rule_pack_json` over the SAME
/// bytes — one wording, no fork), and `packsLoaded` excludes it.
fn assert_too_new_pack_skipped(value: &serde_json::Value, pack_id: &str, pack_json: &str) {
    let loader_report: serde_json::Value =
        serde_json::from_str(&crate::validate_rule_pack_json(pack_json)).unwrap();
    assert_eq!(loader_report["valid"], false);
    let loader_message = loader_report["issues"][0]
        .as_str()
        .expect("loader issue string");
    assert!(
        loader_message.contains("pack requires newer DSL schema"),
        "sanity: the loader names the schema gate, got: {loader_report}"
    );

    let warnings = value["warnings"].as_array().expect("warnings array");
    let skip = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains(pack_id) && w.contains("skipped"))
        .unwrap_or_else(|| panic!("expected a by-name skip warning for {pack_id}, got: {value}"));
    assert!(
        skip.contains(loader_message),
        "the packDefs skip warning must carry the loader's exact wording ({loader_message:?}), got: {skip:?}"
    );

    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    assert!(
        !loaded.iter().any(|p| p["id"] == pack_id),
        "a skipped too-new pack must not appear in packsLoaded, got: {value}"
    );
}

#[test]
fn analyze_json_pack_defs_too_new_schema_version_is_skipped_with_the_loaders_warning() {
    let dir = cycle_fixture();
    let pack = too_new_pack_json("future-pack");
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{pack}]}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should still succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_too_new_pack_skipped(&value, "future-pack", &pack);
}

#[test]
fn analyze_envelope_json_pack_defs_too_new_schema_version_is_skipped_with_the_loaders_warning() {
    let envelope = envelope_with_symbols(&["BadName"]);
    let pack = too_new_pack_json("future-pack");
    let config = format!(r#"{{"sourceId": "legacy", "packDefs": [{pack}]}}"#);
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_too_new_pack_skipped(&value, "future-pack", &pack);
    // The rule inside the skipped pack must not have fired either.
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        !findings.iter().any(|f| f["ruleId"] == "future-pack/r1"),
        "a skipped too-new pack's rules must never fire, got: {value}"
    );
}

#[test]
fn analyze_request_defaults_pack_defs_to_empty() {
    // `packDefs` absent from request JSON must behave identically to before this field existed —
    // an empty `Vec`, contributing nothing to `base_engine_config`'s seed layer.
    let config_json = r#"{"root": "unused", "sourceId": "t"}"#;
    let req: AnalyzeRequest = serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
    assert!(
        req.pack_defs.is_empty(),
        "packDefs absent from request JSON must default to empty"
    );
}

// --- The same `packDefs` contract through the envelope entry point (`analyzeEnvelope`) -----------
// Envelope mode evaluates only symbol-scan/io-scan DSL rules (no source text exists for line-scan),
// so these mirrors use `symbol_scan_pack_json` + `envelope_with_symbols` instead of
// `dsl_pack_json` + a marker file — the pack-merge layer under test is the identical
// `base_engine_config` call either way.

#[test]
fn analyze_envelope_json_pack_defs_inline_pack_fires_without_packs_dir() {
    // No `packsDir` at all — the inline `packDefs` pack must load and fire on its own.
    let envelope = envelope_with_symbols(&["BadName"]);
    let config = format!(
        r#"{{"sourceId": "legacy", "packDefs": [{}]}}"#,
        symbol_scan_pack_json("pack-a", "r1", "^Bad")
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "pack-a/r1"),
        "expected the inline packDefs rule to fire without any packsDir, got: {value}"
    );
}

#[test]
fn analyze_envelope_json_pack_defs_collision_with_packs_dir_directory_pack_wins() {
    // Same pack id from both an inline `packDefs` entry and a `packsDir` directory: the directory
    // pack loads AFTER `pack_defs` in `base_engine_config`'s seed order, so it must win the
    // collision whole — the exact rule the `analyze` path asserts, through the envelope path.
    let envelope = envelope_with_symbols(&["InlineName", "DirName"]);

    let packs_dir_pack = TempDir::new("zzop-facade-envelope-packs-dir-collision");
    packs_dir_pack.write(
        "custom.json",
        &symbol_scan_pack_json("custom", "sym-dir", "^Dir"),
    );

    let config = format!(
        r#"{{"sourceId": "legacy", "packDefs": [{}], "packsDir": {:?}}}"#,
        symbol_scan_pack_json("custom", "sym-inline", "^Inline"),
        packs_dir_pack.path().display()
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "custom/sym-dir"),
        "expected the packsDir directory pack's rule to fire (a directory pack beats an inline def \
         with the same id), got: {value}"
    );
    assert!(
        !findings.iter().any(|f| f["ruleId"] == "custom/sym-inline"),
        "expected the inline packDefs pack to be fully replaced by the packsDir directory pack, \
         got: {value}"
    );
}

#[test]
fn analyze_envelope_json_pack_defs_same_id_later_def_wins() {
    // Two inline `packDefs` entries sharing an id: the later array entry must replace the earlier
    // one whole — same rule as the `analyze` path's inline-collision test.
    let envelope = envelope_with_symbols(&["OldName", "NewName"]);
    let config = format!(
        r#"{{"sourceId": "legacy", "packDefs": [{}, {}]}}"#,
        symbol_scan_pack_json("custom", "sym-old", "^Old"),
        symbol_scan_pack_json("custom", "sym-new", "^New")
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "custom/sym-new"),
        "expected the later inline def's rule to fire, got: {value}"
    );
    assert!(
        !findings.iter().any(|f| f["ruleId"] == "custom/sym-old"),
        "expected the earlier inline def with the same id to be fully replaced, got: {value}"
    );
}

// --- `packsLoaded` — the positive pack-load confirmation through the JSON views ------------------

#[test]
fn analyze_json_packs_loaded_reports_dir_and_inline_sources_sorted_by_id() {
    let dir = cycle_fixture();
    dir.write("marker.ts", "// MARKER_A\n");

    let packs_dir = TempDir::new("zzop-facade-packs-loaded");
    packs_dir.write("zz-dir.json", &dsl_pack_json("zz-dir", "r1", "MARKER_A"));

    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}], "packsDir": {:?}}}"#,
        dir.path().display(),
        dsl_pack_json("aa-inline", "r1", "MARKER_A"),
        packs_dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let summary: Vec<(&str, u64, &str)> = loaded
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap(),
                p["rules"].as_u64().unwrap(),
                p["source"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![("aa-inline", 1, "inline"), ("zz-dir", 1, "dir")],
        "expected id-sorted entries with per-source provenance, got: {value}"
    );
}

#[test]
fn analyze_json_packs_loaded_collision_reports_the_winning_directory_source() {
    // Same pack id from `packDefs` AND `packsDir`: the directory pack wins the collision whole, so the
    // confirmation must report ONE entry for that id, sourced "dir" — never a stale "inline".
    let dir = cycle_fixture();
    dir.write("marker.ts", "// DIR_MARKER\n");

    let packs_dir = TempDir::new("zzop-facade-packs-loaded-collision");
    packs_dir.write(
        "custom.json",
        &dsl_pack_json("custom", "marker-dir", "DIR_MARKER"),
    );

    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}], "packsDir": {:?}}}"#,
        dir.path().display(),
        dsl_pack_json("custom", "marker-inline", "INLINE_MARKER"),
        packs_dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    assert_eq!(loaded.len(), 1, "one entry per pack id, got: {value}");
    assert_eq!(loaded[0]["id"], "custom");
    assert_eq!(loaded[0]["source"], "dir");
}

#[test]
fn analyze_json_packs_loaded_carries_files_in_scope_zero_vs_nonzero() {
    // D16 per-pack applicability through the JSON view (camelCase `filesInScope`, matching
    // packsLoaded's other fields): a loaded pack scoped to an extension the tree lacks reports 0
    // (its zero findings mean "out of scope", not "clean"); a pack whose scope matches reports the
    // exact matching-file count.
    let dir = cycle_fixture(); // a.ts + b.ts — no .py anywhere
    let py_only_pack = r#"{
        "id": "zz-python-only",
        "framework": "any",
        "rules": [
            {
                "id": "r1",
                "severity": "warning",
                "message": "msg",
                "matcher": {
                    "type": "line-scan",
                    "file_pattern": "\\.py$",
                    "line_pattern": "NEVER_MATCHES"
                }
            }
        ]
    }"#;
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}, {}]}}"#,
        dir.path().display(),
        dsl_pack_json("aa-ts", "r1", "NEVER_MATCHES"),
        py_only_pack
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let by_id = |id: &str| {
        loaded
            .iter()
            .find(|p| p["id"] == id)
            .unwrap_or_else(|| panic!("expected pack {id} in packsLoaded, got: {value}"))
    };
    assert_eq!(
        by_id("aa-ts")["filesInScope"],
        2,
        "the .ts-scoped pack must count both fixture files, got: {value}"
    );
    assert_eq!(
        by_id("zz-python-only")["filesInScope"],
        0,
        "the .py-scoped pack has no in-scope file in this tree, got: {value}"
    );
}

#[test]
fn analyze_json_packs_loaded_zero_admission_rules_is_additive_only() {
    // The rule-granularity half of the applicability census on the wire: `zeroAdmissionRules` lists a
    // pack's rules whose own path gates admitted zero files, and it is serialized ONLY when non-empty
    // (the `testPaths` additive-disclosure precedent) — a pack whose every rule admits files carries
    // no new key, so existing consumers see byte-identical entries. A fully out-of-scope pack
    // (`filesInScope: 0`) also carries no key: the pack-level zero already says "all of them".
    let dir = cycle_fixture(); // a.ts + b.ts — no .py anywhere
    let mixed_pack = r#"{
        "id": "zz-mixed",
        "framework": "any",
        "rules": [
            {
                "id": "ts-quiet",
                "severity": "warning",
                "message": "msg",
                "matcher": {
                    "type": "line-scan",
                    "file_pattern": "\\.ts$",
                    "line_pattern": "NEVER_MATCHES"
                }
            },
            {
                "id": "py-blind",
                "severity": "warning",
                "message": "msg",
                "matcher": {
                    "type": "line-scan",
                    "file_pattern": "\\.py$",
                    "line_pattern": "NEVER_MATCHES"
                }
            }
        ]
    }"#;
    let py_only_pack = r#"{
        "id": "zz-python-only",
        "framework": "any",
        "rules": [
            {
                "id": "r1",
                "severity": "warning",
                "message": "msg",
                "matcher": {
                    "type": "line-scan",
                    "file_pattern": "\\.py$",
                    "line_pattern": "NEVER_MATCHES"
                }
            }
        ]
    }"#;
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}, {}, {}]}}"#,
        dir.path().display(),
        dsl_pack_json("aa-ts", "r1", "NEVER_MATCHES"),
        mixed_pack,
        py_only_pack
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let by_id = |id: &str| {
        loaded
            .iter()
            .find(|p| p["id"] == id)
            .unwrap_or_else(|| panic!("expected pack {id} in packsLoaded, got: {value}"))
    };
    assert_eq!(
        by_id("zz-mixed")["zeroAdmissionRules"],
        serde_json::json!(["py-blind"]),
        "the rule that admitted files but found nothing must not be listed, got: {value}"
    );
    assert!(
        by_id("aa-ts").get("zeroAdmissionRules").is_none(),
        "a pack whose every rule admits files must carry NO key (additive-only), got: {value}"
    );
    assert!(
        by_id("zz-python-only").get("zeroAdmissionRules").is_none(),
        "a fully out-of-scope pack repeats nothing — filesInScope: 0 already says it, got: {value}"
    );
    assert_eq!(by_id("zz-python-only")["filesInScope"], 0, "got: {value}");

    // `ruleIds` — the LIST behind the `rules` COUNT, unconditionally serialized (unlike
    // `zeroAdmissionRules` above), because an absent list reads as "this build declines to say" and
    // that is exactly the state a `--rule`/`rule` filter validator must be able to tell apart from
    // "no such rule". Measured defect it closes (2026-08-20): `zzop analyze <tree> --rule
    // security/no-such-rule` exited 0 with an empty stderr and `shown: 0` — a typo inside a LOADED
    // pack, silently reported as a clean run — while `--rule zzz/qqq` (unloaded pack) correctly exited
    // 2. The prefix was all the reply could answer with.
    //
    // Pinned HERE, on a config carrying an inline user pack, because that is the false-refusal case:
    // validating against the catalog compiled into the binary would have refused every id of a pack
    // like `zz-mixed`. Only the run's own ids are right for both pack sources.
    //
    // SORTED, not in the pack file's order (`zz-mixed` declares `ts-quiet` then `py-blind`): the
    // sibling `zeroAdmissionRules` asserted six lines above is a SUBSET of this list and has always
    // been sorted, so two orders inside one object made the obvious consumer question — which of this
    // pack's rules DID admit files — a set difference across mismatched orders, with nothing on the
    // wire saying the orders differ. `zzop_engine::PackLoaded::rule_ids` owns that reasoning.
    assert_eq!(
        by_id("zz-mixed")["ruleIds"],
        serde_json::json!(["py-blind", "ts-quiet"]),
        "a pack must publish every id it loaded with, in the same order its own zeroAdmissionRules \
         subset uses, got: {value}"
    );
    // The subtraction that order buys, done on the wire shape a consumer actually holds.
    let zero: Vec<&str> = by_id("zz-mixed")["zeroAdmissionRules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let admitted: Vec<&str> = by_id("zz-mixed")["ruleIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|id| zero.binary_search(id).is_err())
        .collect();
    assert_eq!(
        admitted,
        vec!["ts-quiet"],
        "a sorted subtraction over the two id lists must name the rule that read this tree and \
         reported nothing, got: {value}"
    );
    assert_eq!(
        by_id("zz-mixed")["ruleIds"].as_array().unwrap().len(),
        by_id("zz-mixed")["rules"].as_u64().unwrap() as usize,
        "the list and the count are two reads of one Vec and must never disagree, got: {value}"
    );
    for id in ["aa-ts", "zz-mixed", "zz-python-only"] {
        assert!(
            by_id(id)["ruleIds"].is_array(),
            "`ruleIds` is unconditional — a missing key means NO DATA to a validator, which would \
             reopen the silent-empty-result hole for {id}: {value}"
        );
    }
}

#[test]
fn analyze_json_packs_loaded_is_an_empty_array_when_no_packs_are_given() {
    // Always serialized — an empty ARRAY (not an absent field) is the honest zero-packs signal.
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    assert!(loaded.is_empty(), "got: {value}");
}

#[test]
fn analyze_envelope_json_packs_loaded_rides_the_envelope_path_too() {
    let envelope = envelope_with_symbols(&["BadName"]);
    let config = format!(
        r#"{{"sourceId": "legacy", "packDefs": [{}]}}"#,
        symbol_scan_pack_json("pack-a", "r1", "^Bad")
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    // The bundled packs are injected by default alongside the caller's own def (host-consistency
    // default, see `analyze_envelope_json`'s doc) — the caller's pack rides as one inline entry
    // among them.
    let entry = loaded
        .iter()
        .find(|p| p["id"] == "pack-a")
        .unwrap_or_else(|| panic!("caller's pack-a must be loaded, got: {value}"));
    assert_eq!(entry["rules"], 1);
    assert_eq!(entry["source"], "inline");
    assert_eq!(
        loaded.len(),
        zzop_config::BUNDLED_PACK_SOURCES.len() + 1,
        "every bundled pack + the caller's def, got: {value}"
    );
}

// --- Envelope bundled-pack default (facade-level; see `analyze_envelope_json`'s doc) -------------

#[test]
fn analyze_envelope_json_zero_config_injects_the_bundled_packs_as_inline() {
    let envelope = envelope_with_symbols(&["Anything"]);
    let out = analyze_envelope_json(&envelope, r#"{"sourceId": "legacy"}"#)
        .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    assert_eq!(
        loaded.len(),
        zzop_config::BUNDLED_PACK_SOURCES.len(),
        "zero-config envelope analysis must load every bundled pack, got: {value}"
    );
    for p in loaded {
        assert_eq!(
            p["source"], "inline",
            "bundled packs arrive as inline packDefs on the envelope path, got: {value}"
        );
    }
    // With the bundle present, the engine's zero-packs capability warning must NOT fire.
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("no DSL rule packs loaded")),
        "got: {value}"
    );
}

#[test]
fn analyze_envelope_json_packs_dir_null_opts_out_of_the_bundled_packs() {
    // The documented `packsDir: null` opt-out ("no DSL packs at all") must survive the
    // facade-level bundled default — absent key and explicit null are different contracts.
    let envelope = envelope_with_symbols(&["Anything"]);
    let out = analyze_envelope_json(&envelope, r#"{"sourceId": "legacy", "packsDir": null}"#)
        .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    assert!(loaded.is_empty(), "got: {value}");
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("no DSL rule packs loaded")),
        "the zero-packs capability self-report must fire on an explicit opt-out, got: {value}"
    );
}

#[test]
fn analyze_envelope_json_js_wrapper_shaped_packs_dir_shadows_the_inline_bundled_seed() {
    // An embedder / the config front-end sends `packsDir: [<bundled rules/dsl>, ...]` for envelope calls; the facade's
    // inline bundled seed must lose every id collision to that on-disk copy (dir wins whole), so a
    // JS-wrapper caller keeps the exact rule set AND the "dir" provenance it had before this
    // default existed — no double-loading, no behavior change.
    let envelope = envelope_with_symbols(&["Anything"]);
    let bundled_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../rules/dsl");
    let config = format!(r#"{{"sourceId": "legacy", "packsDir": {bundled_dir:?}}}"#);
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    assert_eq!(
        loaded.len(),
        zzop_config::BUNDLED_PACK_SOURCES.len(),
        "one entry per pack id — the dir copy replaces its inline twin, never duplicates it, got: {value}"
    );
    for p in loaded {
        assert_eq!(
            p["source"], "dir",
            "the on-disk bundled copy must win every collision against the inline seed, got: {value}"
        );
    }
}

#[test]
fn analyze_envelope_json_caller_pack_def_with_a_bundled_id_wins_the_collision_whole() {
    // Bundled defs seed FIRST, so a caller `packDefs` entry reusing a bundled id (here: "security")
    // is the LATER inline def and replaces the bundled pack whole — the existing later-wins-whole
    // rule, no new precedence.
    let envelope = envelope_with_symbols(&["BadName"]);
    let config = format!(
        r#"{{"sourceId": "legacy", "packDefs": [{}]}}"#,
        symbol_scan_pack_json("security", "override-rule", "^Bad")
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let security: Vec<&serde_json::Value> =
        loaded.iter().filter(|p| p["id"] == "security").collect();
    assert_eq!(security.len(), 1, "one entry per pack id, got: {value}");
    assert_eq!(
        security[0]["rules"], 1,
        "the caller's 1-rule pack must have replaced the bundled security pack whole, got: {value}"
    );
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings
            .iter()
            .any(|f| f["ruleId"] == "security/override-rule"),
        "the caller's override rule must fire, got: {value}"
    );
    // The replacement itself is silent no longer: a shadow warning must name the id and both
    // sides' rule counts (bundled "security" ships 51 rules — see rules/dsl/security/security.json —
    // the caller's def has 1).
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| {
            let w = w.as_str().unwrap();
            w.contains("security") && w.contains("51 rules") && w.contains("replacement: 1 rule")
        }),
        "expected a shadow warning naming 'security' and both rule counts (51 -> 1), got: {value}"
    );
}

// --- Item 1 (blind field-test finding): silent bundled-pack replacement via `packs.extraDirs` ------
// A custom pack loaded via a packs directory (the `packs.extraDirs` -> `packsDir` config surface) that
// reuses an already-loaded pack's id used to replace it whole with ZERO acknowledgment anywhere in
// `packsLoaded`/`warnings` — `base_engine_config`'s collision branches now push one `pack_shadow_warning`
// each time this happens. These tests pin: (1) the warning fires and names the id + both rule counts
// when a genuine cross-source collision happens (custom-over-bundled), (2) no warning fires for an
// ordinary, non-colliding multi-pack load, and (3) the replacement behavior itself (custom pack's rules
// win) is unchanged.

#[test]
fn analyze_envelope_json_extra_dir_pack_shadowing_the_bundled_security_pack_warns_with_both_counts()
{
    // Reproduces the blind-test scenario directly: a custom on-disk pack (loaded the same way
    // `packs.extraDirs` ultimately reaches this engine — as a `packsDir` entry) declares `id:
    // "security"`, colliding with the bundled 51-rule "security" pack the envelope path auto-seeds as
    // inline `packDefs`. The custom 1-rule pack must win the collision whole (unchanged behavior) AND
    // the collision must now be named in `warnings`.
    let envelope = envelope_with_symbols(&["BadName"]);
    let custom_security_dir = TempDir::new("zzop-facade-extra-dir-shadows-bundled");
    custom_security_dir.write(
        "security.json",
        &symbol_scan_pack_json("security", "custom-rule", "^Bad"),
    );
    let config = format!(
        r#"{{"sourceId": "legacy", "packsDir": {:?}}}"#,
        custom_security_dir.path().display()
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();

    // Replacement behavior unchanged: one "security" entry, the custom 1-rule pack, sourced "dir".
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let security: Vec<&serde_json::Value> =
        loaded.iter().filter(|p| p["id"] == "security").collect();
    assert_eq!(security.len(), 1, "one entry per pack id, got: {value}");
    assert_eq!(
        security[0]["rules"], 1,
        "the custom pack must win, got: {value}"
    );
    assert_eq!(security[0]["source"], "dir", "got: {value}");
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings
            .iter()
            .any(|f| f["ruleId"] == "security/custom-rule"),
        "the custom pack's rule must fire, got: {value}"
    );

    // The shadowing is no longer silent: one warning names the id and both rule counts (bundled: 51,
    // replacement: 1), and identifies the winning side as coming from a packs directory.
    let warnings = value["warnings"].as_array().expect("warnings array");
    let shadow = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("security") && w.contains("packs directory"))
        .unwrap_or_else(|| panic!("expected a shadow warning for 'security', got: {value}"));
    assert!(
        shadow.contains("51 rules") && shadow.contains("replacement: 1 rule"),
        "expected both rule counts (bundled 51 -> replacement 1) in the shadow warning, got: {shadow:?}"
    );
}

#[test]
fn analyze_json_packs_dir_array_no_collision_produces_no_shadow_warning() {
    // The ordinary, non-colliding multi-directory load (`analyze_json_packs_dir_array_loads_and_merges_
    // every_directory`'s fixture, distinct pack ids) must NOT trip the new shadow warning — it only
    // fires on an actual same-id replacement.
    let dir = cycle_fixture();
    dir.write("marker.ts", "// MARKER_A\n// MARKER_B\n");

    let packs_a = TempDir::new("zzop-facade-no-collision-a");
    packs_a.write("pack-a.json", &dsl_pack_json("pack-a", "r1", "MARKER_A"));
    let packs_b = TempDir::new("zzop-facade-no-collision-b");
    packs_b.write("pack-b.json", &dsl_pack_json("pack-b", "r1", "MARKER_B"));

    let config = format!(
        r#"{{"root": {:?}, "packsDir": [{:?}, {:?}]}}"#,
        dir.path().display(),
        packs_a.path().display(),
        packs_b.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        !warnings.iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("replaces an earlier-loaded pack")),
        "distinct pack ids must never trip the shadow warning, got: {value}"
    );
}

// --- Suppress-marker collision across packs (1.0 gate review, I5) -------------------------------
// The marker is DERIVED `zzop-<rule id>-ok` with no pack namespace, so a third-party pack reusing a
// bundled rule's bare id makes ONE `// zzop-hardcoded-secret-ok` comment silence both rules. The
// grammar is frozen for 1.0 (user source already carries these comments), so the collision is
// DISCLOSED instead of renamed away — see `zzop_core::suppress_marker_collisions`. These two tests are
// the shipping evidence that the disclosure reaches a user-visible `warnings` entry, not just a unit
// test: (1) it fires end-to-end on a real cross-pack collision, (2) an ordinary non-colliding pack
// load stays silent.

#[test]
fn analyze_envelope_json_third_party_rule_id_colliding_with_a_bundled_one_warns_about_the_marker() {
    // Distinct PACK ids (`acme` vs the bundled `security`), so nothing is shadowed and both packs stay
    // loaded — the collision is purely on the derived marker, which is exactly the silent case.
    let envelope = envelope_with_symbols(&["BadName"]);
    let acme_dir = TempDir::new("zzop-facade-marker-collision");
    acme_dir.write(
        "acme.json",
        &symbol_scan_pack_json("acme", "hardcoded-secret", "^Bad"),
    );
    let config = format!(
        r#"{{"sourceId": "legacy", "packsDir": {:?}}}"#,
        acme_dir.path().display()
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();

    let warnings = value["warnings"].as_array().expect("warnings array");
    let collision = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("zzop-hardcoded-secret-ok"))
        .unwrap_or_else(|| panic!("expected a marker-collision warning, got: {value}"));
    assert!(
        collision.contains("\"acme/hardcoded-secret\"")
            && collision.contains("\"security/hardcoded-secret\""),
        "the warning must name BOTH colliding pack-qualified ids, got: {collision:?}"
    );
    // No pack was replaced — the shadow warning is a different report and must stay quiet here.
    assert!(
        !warnings.iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("replaces an earlier-loaded pack")),
        "distinct pack ids must not trip the shadow warning, got: {value}"
    );
}

#[test]
fn analyze_envelope_json_distinct_rule_ids_produce_no_marker_collision_warning() {
    // CONTROL: same shape, one character of difference — a rule id no bundled pack uses. The
    // disclosure must be silent, or it would fire on every run that loads a second pack.
    let envelope = envelope_with_symbols(&["BadName"]);
    let acme_dir = TempDir::new("zzop-facade-marker-no-collision");
    acme_dir.write(
        "acme.json",
        &symbol_scan_pack_json("acme", "acme-token-in-source", "^Bad"),
    );
    let config = format!(
        r#"{{"sourceId": "legacy", "packsDir": {:?}}}"#,
        acme_dir.path().display()
    );
    let out =
        analyze_envelope_json(&envelope, &config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("suppress marker `")),
        "distinct rule ids must never trip the marker-collision warning, got: {value}"
    );
}

#[test]
fn envelope_analyze_request_defaults_pack_defs_to_empty() {
    // `packDefs` absent from envelope config JSON must behave identically to before this field
    // existed — an empty `Vec`, contributing nothing to `base_engine_config`'s seed layer.
    let config_json = r#"{"sourceId": "legacy"}"#;
    let req: EnvelopeAnalyzeRequest =
        serde_json::from_str(config_json).expect("valid EnvelopeAnalyzeRequest JSON");
    assert!(
        req.pack_defs.is_empty(),
        "packDefs absent from envelope config JSON must default to empty"
    );
}

// -------------------------------------------------------------------------------------------------
// A pack the config switched OFF must not look like a pack that ran and found nothing (2026-08-26).
//
// The defect these pin, from an uncontaminated outside auditor's four-file repro: with
// `packs: { disabled: ["security", "browser"] }` the reply's `findings.total` fell 22 -> 2 and
// `byRule` kept only `dead-candidates`, while `packsLoaded` still carried
// `security  rules=51  filesInScope=9  ruleIds=[...51 ids...]` with nothing marking it off. The only
// contrary signal was a DIFFERENT top-level key (`ruleOverridesApplied.disabled`). A reader who
// joins the two fields the obvious way concludes "the security pack scanned 9 files with 51 rules
// and reported nothing — clean". It never ran once. That turns "not analyzed" into "analyzed and
// safe", for the security pack.
//
// `packs.only` is the same axis from the other side and is pinned here too: an allowlist suppresses
// strictly MORE than `disabled`, and a fix that covered only `disabled` would leave the identical
// lie standing under the other knob.
//
// NOTHING about which packs load or run changes here — only what the reply SAYS about them.
// -------------------------------------------------------------------------------------------------

/// Two packs, both reporting zero findings, for OPPOSITE reasons: `zz-ran` was evaluated over both
/// fixture files and matched nothing; `zz-off` was never evaluated at all. The reply on its own — no
/// second key to join, no prior knowledge of the caller's config — has to tell them apart. That
/// discrimination IS this batch's acceptance condition.
#[test]
fn a_disabled_pack_is_distinguishable_from_a_pack_that_ran_and_found_nothing() {
    let dir = cycle_fixture(); // a.ts + b.ts
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}, {}], "disabledRules": ["zz-off"]}}"#,
        dir.path().display(),
        dsl_pack_json("zz-ran", "r1", "NEVER_MATCHES"),
        dsl_pack_json("zz-off", "r1", "NEVER_MATCHES")
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let by_id = |id: &str| {
        loaded
            .iter()
            .find(|p| p["id"] == id)
            .unwrap_or_else(|| panic!("expected pack {id} in packsLoaded, got: {value}"))
    };

    // Precondition: both packs are still LOADED and both contribute zero findings. The row must not
    // vanish — a pack that loaded is a fact, and a consumer validating a `--rule` id against
    // `ruleIds` still needs it.
    assert_eq!(loaded.len(), 2, "both packs still load, got: {value}");
    assert!(
        value["findings"]["byRule"].get("zz-off/r1").is_none()
            && value["findings"]["byRule"].get("zz-ran/r1").is_none(),
        "both packs report zero findings — that is the whole point, got: {value}"
    );

    // The pack that RAN says nothing new: no gating key at all.
    assert!(
        by_id("zz-ran").get("didNotRun").is_none(),
        "a pack that ran must carry no gating disclosure, got: {value}"
    );
    // The pack that did NOT run says so, in its own row, naming the knob that switched it off.
    assert_eq!(
        by_id("zz-off")["didNotRun"],
        "disabled",
        "the switched-off pack must say so in its own row, got: {value}"
    );
}

/// The opt-IN half. `packsOnly: ["zz-ran"]` leaves `zz-off` loaded and unevaluated exactly as
/// `disabled` does, and it suppresses strictly more (every pack the allowlist fails to name), so the
/// disclosure has to reach it too. An all-typo allowlist — the shape in which EVERY DSL rule goes
/// silent at once — is the extreme of this same case.
#[test]
fn a_pack_outside_packs_only_is_disclosed_on_the_same_axis_as_a_disabled_one() {
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}, {}], "packsOnly": ["zz-ran"]}}"#,
        dir.path().display(),
        dsl_pack_json("zz-ran", "r1", "NEVER_MATCHES"),
        dsl_pack_json("zz-off", "r1", "NEVER_MATCHES")
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let by_id = |id: &str| {
        loaded
            .iter()
            .find(|p| p["id"] == id)
            .unwrap_or_else(|| panic!("expected pack {id} in packsLoaded, got: {value}"))
    };
    assert!(
        by_id("zz-ran").get("didNotRun").is_none(),
        "the allowlisted pack ran, got: {value}"
    );
    assert_eq!(
        by_id("zz-off")["didNotRun"],
        "notAllowlisted",
        "a pack an allowlist never named did not run either, got: {value}"
    );
}

/// `filesInScope` was the half that made the lie convincing: a positive file count on a pack that
/// read nothing. The key now means what section 1 of the output-philosophy says every key means —
/// its PRESENCE is the claim that the pack ran. On a pack that did not run it is replaced by
/// `filesInScopeIfEnabled`, which states the counterfactual instead of a fact.
#[test]
fn a_pack_that_did_not_run_reports_no_files_in_scope_and_says_what_it_would_have_been() {
    let dir = cycle_fixture(); // two .ts files, both in scope of a `\.ts$` pack
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}, {}], "disabledRules": ["zz-off"]}}"#,
        dir.path().display(),
        dsl_pack_json("zz-ran", "r1", "NEVER_MATCHES"),
        dsl_pack_json("zz-off", "r1", "NEVER_MATCHES")
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    let by_id = |id: &str| {
        loaded
            .iter()
            .find(|p| p["id"] == id)
            .unwrap_or_else(|| panic!("expected pack {id} in packsLoaded, got: {value}"))
    };
    assert_eq!(
        by_id("zz-ran")["filesInScope"],
        2,
        "the pack that ran keeps the field unchanged, got: {value}"
    );
    assert!(
        by_id("zz-off").get("filesInScope").is_none(),
        "a pack that read nothing must not publish a scanned-file count, got: {value}"
    );
    assert_eq!(
        by_id("zz-off")["filesInScopeIfEnabled"],
        2,
        "the counterfactual is still worth stating — it is what re-enabling would buy, got: {value}"
    );
    // Rule-level admission is a statement about a run that happened. On a pack that never ran, the
    // pack-level disclosure already says "all of them", the same reasoning that omits the field on a
    // `filesInScope: 0` pack.
    assert!(
        by_id("zz-off").get("zeroAdmissionRules").is_none(),
        "admission is meaningless for a pack that did not run, got: {value}"
    );
}

/// `packsLoadedMeaning` — the legend every other numeric channel in this reply already carries and
/// this one did not. Two sentences it must contain, because the auditor found BOTH ambiguities in
/// the same row: what `filesInScope` counts, and what a rule's presence in `zeroAdmissionRules`
/// means (no file ever reached it — the opposite of "files reached it and it found nothing").
#[test]
fn packs_loaded_meaning_defines_the_scope_and_admission_axes() {
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}]}}"#,
        dir.path().display(),
        dsl_pack_json("zz-ran", "r1", "NEVER_MATCHES")
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let meaning = &value["packsLoadedMeaning"];
    assert!(
        meaning.is_object(),
        "the array needs a sibling legend — it cannot host one per row, got: {value}"
    );
    let scope = meaning["filesInScope"].as_str().unwrap_or_default();
    assert!(
        scope.contains("candidacy") && scope.contains("did not run"),
        "the scope legend must say it is path candidacy AND what its absence means: {scope}"
    );
    let admission = meaning["zeroAdmissionRules"].as_str().unwrap_or_default();
    assert!(
        admission.contains("no analyzed file"),
        "admission 0 has to be pinned to the harmless reading, not the clean-bill one: {admission}"
    );
    // No pack was gated, so the reply says NOTHING about gating. A disclosure that fires when there
    // is nothing to disclose is the noise that teaches readers to skip disclosures.
    assert!(
        meaning.get("didNotRun").is_none(),
        "no pack was gated — the legend must stay silent about gating, got: {value}"
    );
}

/// The canary in both directions, on the field that carries the batch's whole claim: with no pack
/// gated, not one row grows a `didNotRun` key. If this ever passes while
/// `a_disabled_pack_is_distinguishable_from_a_pack_that_ran_and_found_nothing` also passes on a
/// hardcoded value, the pair is inconsistent — the reason both live here.
#[test]
fn an_ungated_run_grows_no_gating_key_anywhere_in_packs_loaded() {
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "packDefs": [{}, {}]}}"#,
        dir.path().display(),
        dsl_pack_json("zz-one", "r1", "NEVER_MATCHES"),
        dsl_pack_json("zz-two", "r1", "NEVER_MATCHES")
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let loaded = value["packsLoaded"].as_array().expect("packsLoaded array");
    assert_eq!(loaded.len(), 2, "{value}");
    for row in loaded {
        assert!(row.get("didNotRun").is_none(), "{value}");
        assert!(row.get("filesInScopeIfEnabled").is_none(), "{value}");
        assert!(
            row.get("filesInScope").is_some(),
            "every pack ran, so every row keeps the plain count: {value}"
        );
    }
}
