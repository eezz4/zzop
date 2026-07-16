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
    // The JS wrapper's documented `packsDir: null` opt-out ("no DSL packs at all") must survive the
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
    // The JS wrapper sends `packsDir: [<bundled rules/dsl>, ...]` for envelope calls; the facade's
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
