//! DSL-rule gating, config-diagnostics parity, determinism, fragment composition, and the
//! envelope-mode fragment-specifier resolver.

use zzop_core::{ImportBinding, RulePackDef, SourceSymbol, SourceSymbolKind};

use crate::envelope::analyze_envelope;

use super::{config, envelope, projection};

// --- Config-diagnostics parity with `assemble` (the envelope-path diagnostics asymmetry) ---

#[test]
fn disabled_rules_typo_self_reports_in_envelope_mode() {
    let mut cfg = config();
    cfg.rule_config.disabled_rules = vec!["no-such-rule".to_string()];
    let out = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &cfg);
    // Config-channel diagnostic — rides `config_warnings`, not `warnings` (parity with `assemble`'s
    // native path — see `zzop_engine::AnalyzeOutput::config_warnings`'s doc).
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("matching no known rule id")),
        "must NOT duplicate into warnings, got: {:?}",
        out.warnings
    );
    assert!(
        out.config_warnings
            .iter()
            .any(|w| w.contains("matching no known rule id") && w.contains("no-such-rule")),
        "got: {:?}",
        out.config_warnings
    );
}

#[test]
fn unmatched_suppression_and_global_exclude_warn_in_envelope_mode() {
    let mut cfg = config();
    cfg.rule_config.suppressions = vec![zzop_core::Suppression {
        rule: "circular".to_string(),
        path: None,
        glob: Some("*.stories.tsx".to_string()),
    }];
    cfg.rule_config.global_excludes = vec![zzop_core::GlobalExclude {
        path: Some("legacy/".to_string()),
        glob: None,
    }];
    let out = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &cfg);
    // One warning per dead filter: the suppression glob and the top-level exclude path.
    assert_eq!(
        out.warnings
            .iter()
            .filter(|w| w.contains("matched no files"))
            .count(),
        2,
        "got: {:?}",
        out.warnings
    );
}

#[test]
fn matched_filters_and_valid_disabled_rules_stay_silent_in_envelope_mode() {
    let mut cfg = config();
    cfg.rule_config.disabled_rules = vec!["circular".to_string()];
    cfg.rule_config.suppressions = vec![zzop_core::Suppression {
        rule: "circular".to_string(),
        path: Some("a.jsp".to_string()),
        glob: None,
    }];
    let out = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &cfg);
    assert!(
        !out.warnings.iter().any(|w| w.contains("matched no files")),
        "got: {:?}",
        out.warnings
    );
    assert!(
        !out.config_warnings
            .iter()
            .any(|w| w.contains("matching no known rule id")),
        "got: {:?}",
        out.config_warnings
    );
}

#[test]
fn symbol_scan_dsl_rule_fires_against_envelope_symbols() {
    let pack: RulePackDef = serde_json::from_str(
        r#"{"id":"t","framework":"any","rules":[{"id":"r","severity":"info","message":"m","matcher":{"type":"symbol-scan","file_pattern":"\\.jsp$","name_pattern":"^Bad"}}]}"#,
    )
    .unwrap();
    let mut a = projection("a.jsp", 5);
    a.symbols.push(SourceSymbol {
        id: "a.jsp#BadName".to_string(),
        file: "a.jsp".to_string(),
        name: "BadName".to_string(),
        kind: SourceSymbolKind::Function,
        line: 4,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });
    let env = envelope(vec![a]);
    let mut cfg = config();
    cfg.packs = vec![pack];
    let out = analyze_envelope(&env, &cfg);
    assert!(out.findings.iter().any(|f| f.rule_id == "t/r"));
}

#[test]
fn envelope_mode_populates_packs_loaded_with_provenance() {
    // The positive pack-load confirmation must ride the envelope path too — same
    // `PackLoaded::from_config` derivation as `analyze_tree`, gated on the identical `config.packs`.
    let pack: RulePackDef = serde_json::from_str(
        r#"{"id":"t","framework":"any","rules":[{"id":"r","severity":"info","message":"m","matcher":{"type":"symbol-scan","file_pattern":"\\.jsp$","name_pattern":"^Bad"}}]}"#,
    )
    .unwrap();
    let mut cfg = config();
    cfg.packs = vec![pack];
    cfg.pack_sources
        .insert("t".to_string(), crate::PackSource::Dir);
    let out = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &cfg);
    assert_eq!(out.packs_loaded.len(), 1);
    assert_eq!(out.packs_loaded[0].id, "t");
    assert_eq!(out.packs_loaded[0].rules, 1);
    assert_eq!(out.packs_loaded[0].source, "dir");

    let zero = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &config());
    assert!(zero.packs_loaded.is_empty());
}

#[test]
fn line_scan_dsl_rule_never_fires_in_envelope_mode() {
    // A LineScan rule that would match "TODO" if it ever saw source text — envelope mode carries no
    // text, so the rule is filtered out rather than silently "running clean".
    let pack: RulePackDef = serde_json::from_str(
        r#"{"id":"t","framework":"any","rules":[{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.jsp$","line_pattern":"TODO"}}]}"#,
    )
    .unwrap();
    let env = envelope(vec![projection("a.jsp", 1)]);
    let mut cfg = config();
    cfg.packs = vec![pack];
    let out = analyze_envelope(&env, &cfg);
    assert!(!out.findings.iter().any(|f| f.rule_id == "t/r"));
}

/// `EngineConfig::profile_rules` in Mode A — the wiring that ended the "envelope path runs outside
/// the timing accumulator" era (and the CLI's exit-2 refusal that guarded it). One symbol-scan rule
/// (fires per-file) + one io-scan rule (fires whole-tree), both against envelope-injected facts.
fn profiling_fixture() -> (zzop_core::NormalizedEnvelope, RulePackDef) {
    let pack: RulePackDef = serde_json::from_str(
        r#"{"id":"t","framework":"any","rules":[
            {"id":"sym","severity":"info","message":"m","matcher":{"type":"symbol-scan","file_pattern":"\\.jsp$","name_pattern":"^Bad"}},
            {"id":"io","severity":"warning","message":"m","matcher":{"type":"io-scan","file_pattern":"\\.jsp$","direction":"provides","kind":"http","key_pattern":"/admin(/|$)"}}
        ]}"#,
    )
    .unwrap();
    let mut a = projection("a.jsp", 5);
    a.symbols.push(SourceSymbol {
        id: "a.jsp#BadName".to_string(),
        file: "a.jsp".to_string(),
        name: "BadName".to_string(),
        kind: SourceSymbolKind::Function,
        line: 4,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });
    a.io.provides.push(zzop_core::IoProvide {
        response: None,
        kind: "http".to_string(),
        key: "GET /admin/users".to_string(),
        file: "a.jsp".to_string(),
        line: 2,
        symbol: None,
        body: None,
    });
    (envelope(vec![a]), pack)
}

#[test]
fn profile_rules_off_leaves_envelope_rule_timings_none() {
    let (env, pack) = profiling_fixture();
    let mut cfg = config();
    cfg.packs = vec![pack];
    let out = analyze_envelope(&env, &cfg);
    assert!(out.rule_timings.is_none(), "{:?}", out.rule_timings);
    // The fixture genuinely fires both rule classes — otherwise the profiled twin below could pass
    // by timing rules that never produce anything.
    assert!(out.findings.iter().any(|f| f.rule_id == "t/sym"));
    assert!(out.findings.iter().any(|f| f.rule_id == "t/io"));
}

#[test]
fn profile_rules_on_times_envelope_dsl_rules_and_whole_graph_analyses() {
    let (env, pack) = profiling_fixture();
    let mut cfg = config();
    cfg.packs = vec![pack.clone()];
    cfg.profile_rules = true;
    let out = analyze_envelope(&env, &cfg);
    let timings = out
        .rule_timings
        .as_ref()
        .expect("profiled Mode A run must populate rule_timings");
    assert!(!timings.is_empty());
    let find = |id: &str| timings.iter().find(|t| t.rule_id == id);
    // Per-file DSL pass: symbol-scan timed under its "{pack}/{rule}" id, with its real finding count.
    assert_eq!(find("t/sym").expect("symbol-scan rule timed").findings, 1);
    // Whole-tree io-scan pass: same shared `eval_pack_timed` evaluator, real whole-tree count (the
    // per-file placeholder alone would leave this at 0 — see `assemble::rules::io_scan`'s tests).
    assert_eq!(find("t/io").expect("io-scan rule timed").findings, 1);
    // Whole-graph native analyses ride the same accumulator, same ids as the native path.
    assert!(
        find("circular").is_some() && find("dead-candidates").is_some(),
        "{timings:?}"
    );
    // Profiling never changes findings — same rule ids fire either way.
    let mut cfg_off = config();
    cfg_off.packs = vec![pack];
    let unprofiled = analyze_envelope(&env, &cfg_off);
    assert_eq!(
        serde_json::to_value(&out.findings).unwrap(),
        serde_json::to_value(&unprofiled.findings).unwrap()
    );
}

#[test]
fn two_runs_over_the_same_envelope_are_byte_for_byte_identical() {
    let mut a = projection("a.jsp", 5);
    a.imports.insert(
        "b".to_string(),
        ImportBinding {
            specifier: "b.jsp".to_string(),
            original: "default".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    let env = envelope(vec![a, projection("b.jsp", 5)]);
    let out1 = analyze_envelope(&env, &config());
    let out2 = analyze_envelope(&env, &config());
    assert_eq!(
        serde_json::to_value(&out1.ir).unwrap(),
        serde_json::to_value(&out2.ir).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&out1.findings).unwrap(),
        serde_json::to_value(&out2.findings).unwrap()
    );
}

#[test]
fn router_mount_fragments_split_across_two_files_compose_into_one_http_provide() {
    use zzop_core::{RouterMountEntry, RouterMountFragment};

    // Mount file: an "app" router mounting "sub" at "/api", by exact-path specifier.
    let mut mount_file = projection("app.jsp", 4);
    mount_file.router_mount_fragments.push(RouterMountFragment {
        name: "app".to_string(),
        entries: vec![RouterMountEntry::Mount {
            prefix: "/api".to_string(),
            ident: "sub".to_string(),
            specifier: Some("sub.jsp".to_string()),
            attr_keys: vec![],
        }],
    });

    // Sub-router file: registers one verb, `POST /widgets`.
    let mut sub_file = projection("sub.jsp", 3);
    sub_file.router_mount_fragments.push(RouterMountFragment {
        name: "sub".to_string(),
        entries: vec![RouterMountEntry::Verb {
            method: "POST".to_string(),
            path: "/widgets".to_string(),
            handler: Some("createWidget".to_string()),
            line: 2,
            attr_keys: vec![],
        }],
    });

    let env = envelope(vec![mount_file, sub_file]);
    let out = analyze_envelope(&env, &config());
    let provides = out.ir.ir.io.expect("expected io facts").provides;
    assert!(
        provides
            .iter()
            .any(|p| p.kind == "http" && p.key == "POST /api/widgets" && p.file == "sub.jsp"),
        "{:?}",
        provides
    );
}

mod resolve_envelope_specifier_tests {
    use crate::envelope::resolve::resolve_envelope_specifier;
    use std::collections::HashSet;

    #[test]
    fn relative_dot_slash_resolves_against_the_emitting_files_own_directory() {
        let all: HashSet<&str> = ["a/b.ts", "a/sibling.ts"].into_iter().collect();
        assert_eq!(
            resolve_envelope_specifier("./sibling", "a/b.ts", &all),
            Some("a/sibling.ts".to_string())
        );
    }

    #[test]
    fn parent_relative_dot_dot_slash_walks_up_one_directory() {
        let all: HashSet<&str> = ["a/b/c.ts", "a/x.ts"].into_iter().collect();
        assert_eq!(
            resolve_envelope_specifier("../x", "a/b/c.ts", &all),
            Some("a/x.ts".to_string())
        );
    }

    #[test]
    fn exact_match_wins_over_relative_join() {
        // "./x" from "a/b.ts" would join to "a/x" — but an exact path literally named "./x" must win
        // outright per the documented precedence.
        let all: HashSet<&str> = ["./x", "a/x.ts"].into_iter().collect();
        assert_eq!(
            resolve_envelope_specifier("./x", "a/b.ts", &all),
            Some("./x".to_string())
        );
    }

    #[test]
    fn extension_guessing_finds_a_real_source_file_behind_an_extensionless_join() {
        let all: HashSet<&str> = ["a/sibling.tsx"].into_iter().collect();
        assert_eq!(
            resolve_envelope_specifier("./sibling", "a/b.ts", &all),
            Some("a/sibling.tsx".to_string())
        );
    }

    /// The appended-extension candidate list must cover every extension a classic/bundler
    /// extensionless resolution can land on — `.jsx` included. It was the one hole: a JS-flavored
    /// projection whose fragment `Ref` says `./Button` while the file is `Button.jsx` resolved to
    /// `None`, i.e. the mount silently vanished.
    #[test]
    fn extension_guessing_covers_every_extensionless_resolvable_extension() {
        for ext in ["ts", "tsx", "js", "jsx"] {
            let owned = format!("a/sibling.{ext}");
            let all: HashSet<&str> = [owned.as_str()].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./sibling", "a/b.ts", &all),
                Some(owned.clone()),
                "extensionless './sibling' must find {owned}"
            );
        }
    }

    /// The other direction, and the reason the list is not simply `is_ts_source_ext`'s eight: the
    /// `.mjs`/`.cjs`/`.mts`/`.cts` family is NEVER reachable from an extensionless specifier under
    /// any resolver — Node ESM and TypeScript's node16 mode both demand the explicit extension. A
    /// future widening to "everything that parses" would invent a resolution no toolchain performs.
    #[test]
    fn extension_guessing_never_appends_an_explicit_extension_only_family_member() {
        for ext in ["mjs", "cjs", "mts", "cts"] {
            let owned = format!("a/sibling.{ext}");
            let all: HashSet<&str> = [owned.as_str()].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./sibling", "a/b.ts", &all),
                None,
                "extensionless './sibling' must NOT invent a .{ext} resolution"
            );
        }
    }

    #[test]
    fn unresolvable_specifier_is_none() {
        let all: HashSet<&str> = ["a/b.ts"].into_iter().collect();
        assert_eq!(
            resolve_envelope_specifier("some-package", "a/b.ts", &all),
            None
        );
        assert_eq!(
            resolve_envelope_specifier("./missing", "a/b.ts", &all),
            None
        );
    }
}
