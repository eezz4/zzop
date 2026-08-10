//! End-to-end tests for the external-parser protocol receiver (`zzop_engine::analyze_envelope`,
//! `docs/NORMALIZED_AST.md`).
//!
//! - `envelope_produces_ir_dep_and_native_analyses_deterministically`: a two-file envelope with a dep
//!   edge (import) and an `IoProvide` -> `analyze_envelope` produces the assembled `ir`/`symbols`/`dep`,
//!   runs the `circular`/`dead-candidates` whole-graph native analyses, and is byte-for-byte
//!   deterministic across two runs.
//! - `envelope_be_joins_cross_layer_with_a_ts_parsed_fe`: proves the cross-layer join promise
//!   (`docs/NORMALIZED_AST.md`'s "a parser is first class regardless of how crude it is, as long as its
//!   projection is accurate") by hand-joining an envelope-projected BE tree's `IoFacts` against a real,
//!   natively-parsed (TypeScript) FE tree's `IoFacts` via `zzop_core::link_cross_layer_io` — the same
//!   linker `analyze_trees` itself calls, exercised manually here since `analyze_envelope` takes one
//!   envelope at a time (by design — `analyze_trees` stays untouched by the envelope path).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{
    link_cross_layer_io, FileProjection, ImportBinding, ImportMap, IoFacts, IoProvide,
    NormalizedEnvelope, RouterMountEntry, RouterMountFragment, RulePackDef, SourceIo, SourceSymbol,
    SourceSymbolKind, NORMALIZED_AST_FORMAT,
};
use zzop_engine::{analyze_envelope, analyze_tree, EngineConfig};

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

fn projection(path: &str, loc: u32) -> FileProjection {
    FileProjection {
        class_shape_fragments: Vec::new(),
        path: path.to_string(),
        loc,
        symbols: Vec::new(),
        imports: ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: std::collections::HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
        is_entry: false,
        overrides: Default::default(),
        attributes: Vec::new(),
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        calls: Vec::new(),
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "legacy-jsp".to_string(),
        ..EngineConfig::default()
    }
}

#[test]
fn envelope_produces_ir_dep_and_native_analyses_deterministically() {
    let mut controller = projection("legacy/UserController.jsp", 40);
    controller.symbols.push(SourceSymbol {
        id: "legacy/UserController.jsp#getUser".to_string(),
        file: "legacy/UserController.jsp".to_string(),
        name: "getUser".to_string(),
        kind: SourceSymbolKind::Function,
        line: 5,
        exported: true,
        is_default: false,
        body_start: Some(5),
        body_end: Some(20),
        write_sites: Vec::new(),
    });
    controller.io.provides.push(IoProvide {
        body: None,
        response: None,
        kind: "http".to_string(),
        key: "GET /legacy/user.jsp".to_string(),
        file: "legacy/UserController.jsp".to_string(),
        line: 5,
        symbol: Some("getUser".to_string()),
    });
    controller.imports.insert(
        "util".to_string(),
        ImportBinding {
            specifier: "legacy/util.jsp".to_string(),
            original: "default".to_string(),
            deferred: false,
            type_only: false,
        },
    );

    let util = projection("legacy/util.jsp", 12);

    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "legacy-jsp".to_string(),
        files: vec![controller, util],
    };

    let out1 = analyze_envelope(&envelope, &config());
    assert_eq!(out1.file_count, 2);
    assert_eq!(out1.ir.ir.symbols.len(), 1);
    assert_eq!(
        out1.ir.ir.dep.get("legacy/UserController.jsp").cloned(),
        Some(vec!["legacy/util.jsp".to_string()])
    );
    assert_eq!(
        out1.ir.ir.dep.get("legacy/util.jsp").cloned(),
        Some(Vec::new())
    );
    let io = out1.ir.ir.io.as_ref().expect("expected io facts");
    assert_eq!(io.provides.len(), 1);
    assert_eq!(io.provides[0].key, "GET /legacy/user.jsp");

    // `legacy/UserController.jsp` has no importers within this envelope (only `util.jsp`, which it
    // imports, has a nonzero fan-in). `.jsp` is not a TS-dispatch extension, but `analyze_envelope`
    // inserts every processed file as a `dep` key (even with an empty edge list — see that function's own
    // comment), so `legacy/UserController.jsp` genuinely participates in the dep graph the fan-in was
    // computed from. `dead-candidates`'s union discriminator (`dead_candidates.rs`'s module doc) treats
    // dep-graph participation as sufficient on its own, so this DOES fire here — fan_in == 0 on it is real
    // "no importers" signal, not "untracked".
    assert!(out1
        .findings
        .iter()
        .any(|f| f.rule_id == "dead-candidates" && f.file == "legacy/UserController.jsp"));
    assert!(!out1.findings.iter().any(|f| f.rule_id == "circular"));

    let out2 = analyze_envelope(&envelope, &config());
    assert_eq!(
        serde_json::to_value(&out1.ir).unwrap(),
        serde_json::to_value(&out2.ir).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&out1.findings).unwrap(),
        serde_json::to_value(&out2.findings).unwrap()
    );
    assert_eq!(out1.degraded, out2.degraded);
    assert_eq!(out1.file_count, out2.file_count);
}

#[test]
fn envelope_be_joins_cross_layer_with_a_ts_parsed_fe() {
    // BE side: a JSP-shaped envelope whose only accurate contribution is its IoFacts (per
    // `docs/NORMALIZED_AST.md`'s promise: a crude parser still joins correctly as long as it extracts
    // IoFacts precisely — no symbols/imports needed for the cross-layer join itself).
    let mut controller = projection("legacy/UserController.jsp", 40);
    controller.io.provides.push(IoProvide {
        body: None,
        response: None,
        kind: "http".to_string(),
        key: "GET /legacy/user.jsp".to_string(),
        file: "legacy/UserController.jsp".to_string(),
        line: 5,
        symbol: Some("getUser".to_string()),
    });
    let be_envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "be".to_string(),
        files: vec![controller],
    };
    let be_config = EngineConfig {
        source_id: "be".to_string(),
        ..EngineConfig::default()
    };
    let be_out = analyze_envelope(&be_envelope, &be_config);

    // FE side: a real, natively-parsed TypeScript tree consuming that same normalized HTTP key.
    let fe_dir = TempDir::new("zzop-engine-envelope-fe");
    fe_dir.write(
        "src/Ctx.tsx",
        "export function load() { return fetch(\"/legacy/user.jsp\"); }\n",
    );
    let fe_config = EngineConfig {
        source_id: "fe".to_string(),
        ..EngineConfig::default()
    };
    let fe_out = analyze_tree(fe_dir.path(), &fe_config);

    // Manual join — the exact same linker `analyze_trees` itself calls, over both trees' `IoFacts`
    // (`analyze_envelope` takes one envelope at a time, so the join is driven by hand here).
    let trees = vec![
        SourceIo {
            source: "be".to_string(),
            io: be_out.ir.ir.io.clone().unwrap_or_default(),
        },
        SourceIo {
            source: "fe".to_string(),
            io: fe_out.ir.ir.io.clone().unwrap_or_default(),
        },
    ];
    let cross_layer = link_cross_layer_io(&trees, &zzop_core::LinkOptions::default());

    let http_edges: Vec<_> = cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected exactly one cross-layer http edge, got: {:?}",
        cross_layer.edges
    );
    let edge = http_edges[0];
    assert_eq!(edge.key, "GET /legacy/user.jsp");
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/Ctx.tsx");
    assert_eq!(edge.to.source, "be");
    assert_eq!(edge.to.file, "legacy/UserController.jsp");
    assert_eq!(edge.to.symbol.as_deref(), Some("getUser"));
    assert!(edge.cross_source);

    assert!(cross_layer.unprovided_consumes.is_empty());
    assert!(cross_layer.unconsumed_provides.is_empty());
    assert!(cross_layer.unresolved_consumes.is_empty());
}

/// Proves `analyze_envelope` applies the same per-rule `"{pack}/{rule}"` `disabled_rules` gating
/// `pipeline::run_file_pass` does (both now share `pipeline::gate_pack_rules` — see `normalized.rs`'s
/// comment at its `enabled_packs` construction). Two `SymbolScan` rules (the only matcher shape envelope
/// mode's text-less `SourceFile` can evaluate — see `normalized.rs`'s module doc) live in one pack and
/// both fire against the same file; disabling one by its full `pack/rule` id must drop only that rule's
/// finding and leave its sibling untouched.
#[test]
fn envelope_disabled_rules_drops_one_rule_and_leaves_its_sibling_pack_mate_intact() {
    let pack: RulePackDef = serde_json::from_str(
        r#"{
            "id": "envelope-test",
            "framework": "any",
            "rules": [
                {
                    "id": "flag-get",
                    "severity": "info",
                    "message": "getter symbol",
                    "matcher": {
                        "type": "symbol-scan",
                        "file_pattern": "\\.jsp$",
                        "name_pattern": "^get"
                    }
                },
                {
                    "id": "flag-post",
                    "severity": "info",
                    "message": "poster symbol",
                    "matcher": {
                        "type": "symbol-scan",
                        "file_pattern": "\\.jsp$",
                        "name_pattern": "^post"
                    }
                }
            ]
        }"#,
    )
    .expect("parse test pack");

    let mut handler = projection("legacy/Handler.jsp", 20);
    handler.symbols.push(SourceSymbol {
        id: "legacy/Handler.jsp#getUser".to_string(),
        file: "legacy/Handler.jsp".to_string(),
        name: "getUser".to_string(),
        kind: SourceSymbolKind::Function,
        line: 3,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });
    handler.symbols.push(SourceSymbol {
        id: "legacy/Handler.jsp#postOrder".to_string(),
        file: "legacy/Handler.jsp".to_string(),
        name: "postOrder".to_string(),
        kind: SourceSymbolKind::Function,
        line: 9,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });

    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "legacy-jsp".to_string(),
        files: vec![handler],
    };

    // Baseline: both sibling rules fire with no gating.
    let baseline_config = EngineConfig {
        source_id: "legacy-jsp".to_string(),
        packs: vec![pack.clone()],
        ..EngineConfig::default()
    };
    let baseline = analyze_envelope(&envelope, &baseline_config);
    assert!(baseline
        .findings
        .iter()
        .any(|f| f.rule_id == "envelope-test/flag-get"));
    assert!(baseline
        .findings
        .iter()
        .any(|f| f.rule_id == "envelope-test/flag-post"));

    // Gated: disabling only "envelope-test/flag-get" removes just that rule's finding.
    let mut gated_config = baseline_config;
    gated_config
        .rule_config
        .disabled_rules
        .push("envelope-test/flag-get".to_string());
    let gated = analyze_envelope(&envelope, &gated_config);
    assert!(!gated
        .findings
        .iter()
        .any(|f| f.rule_id == "envelope-test/flag-get"));
    assert!(gated
        .findings
        .iter()
        .any(|f| f.rule_id == "envelope-test/flag-post"));
}

#[test]
fn envelope_mode_lists_rules_its_matcher_filter_never_runs_as_zero_admission() {
    // Envelope evaluation retains only SymbolScan/IoScan rules (`envelope_rule_pack`), but the
    // `zeroAdmissionRules` census used to be computed over the UNFILTERED `config.packs` — so a
    // line-scan rule whose `file_pattern` matched the envelope's files read as covered while it
    // NEVER ran, exactly the vacuous green this field exists to disclose. It must be listed; its
    // symbol-scan pack-mate (which really ran and admitted the file) must not.
    let pack: RulePackDef = serde_json::from_str(
        r#"{
            "id": "envelope-census",
            "framework": "any",
            "rules": [
                {
                    "id": "line-rule",
                    "severity": "info",
                    "message": "a line-scan rule envelope mode can never evaluate",
                    "matcher": {
                        "type": "line-scan",
                        "file_pattern": "\\.jsp$",
                        "line_pattern": "TODO"
                    }
                },
                {
                    "id": "symbol-rule",
                    "severity": "info",
                    "message": "getter symbol",
                    "matcher": {
                        "type": "symbol-scan",
                        "file_pattern": "\\.jsp$",
                        "name_pattern": "^get"
                    }
                }
            ]
        }"#,
    )
    .expect("parse test pack");

    let mut handler = projection("legacy/Handler.jsp", 20);
    handler.symbols.push(SourceSymbol {
        id: "legacy/Handler.jsp#getUser".to_string(),
        file: "legacy/Handler.jsp".to_string(),
        name: "getUser".to_string(),
        kind: SourceSymbolKind::Function,
        line: 3,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });

    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "legacy-jsp".to_string(),
        files: vec![handler],
    };
    let cfg = EngineConfig {
        source_id: "legacy-jsp".to_string(),
        packs: vec![pack],
        ..EngineConfig::default()
    };

    let out = analyze_envelope(&envelope, &cfg);
    let entry = out
        .packs_loaded
        .iter()
        .find(|p| p.id == "envelope-census")
        .expect("pack loaded");
    // `files_in_scope` stays the path-candidacy fact (the pattern really matches the file)…
    assert_eq!(entry.files_in_scope, 1);
    // …and the rule-granularity half tells the mode's truth: the never-evaluated line-scan rule is
    // zero-admission (its green is vacuous), the symbol-scan rule that ran is not.
    assert_eq!(
        entry.zero_admission_rules,
        vec!["line-rule".to_string()],
        "{:?}",
        entry.zero_admission_rules
    );

    // Envelope mode has no disk cache (`cache: None`), and the census is a pure function of
    // (packs, file list, mode) — a second run must be byte-identical.
    let out2 = analyze_envelope(&envelope, &cfg);
    assert!(out.cache.is_none());
    assert_eq!(out.packs_loaded, out2.packs_loaded);
}

/// Proves `analyze_envelope` composes `router_mount_fragments` split across two `FileProjection`s (a
/// mount file + a sub-router file with a `Verb` entry) into a whole-tree `http` `IoProvide` — the same
/// composition `analyze::assemble` runs natively, now wired for envelope mode too (see this crate's
/// `envelope.rs` module doc). `specifier` here is the target file's exact `path` — the simplest case
/// `resolve_envelope_specifier`'s exact-match branch handles, deliberately not exercising the
/// `./`-relative-join branch (that has its own unit tests in `envelope.rs`).
#[test]
fn envelope_composes_router_mount_fragments_split_across_two_files() {
    let mut mount = projection("be/router.jsp", 5);
    mount.router_mount_fragments.push(RouterMountFragment {
        name: "app".to_string(),
        entries: vec![RouterMountEntry::Mount {
            prefix: "/api/widgets".to_string(),
            ident: "widgetsRoute".to_string(),
            specifier: Some("be/widgets.jsp".to_string()),
            attr_keys: vec![],
        }],
    });

    let mut sub = projection("be/widgets.jsp", 8);
    sub.router_mount_fragments.push(RouterMountFragment {
        name: "widgetsRoute".to_string(),
        entries: vec![RouterMountEntry::Verb {
            method: "POST".to_string(),
            path: "/create".to_string(),
            handler: Some("createWidget".to_string()),
            line: 6,
            attr_keys: vec![],
        }],
    });

    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "legacy-jsp".to_string(),
        files: vec![mount, sub],
    };

    let out = analyze_envelope(&envelope, &config());
    let provides = out.ir.ir.io.expect("expected io facts").provides;
    assert!(
        provides.iter().any(|p| p.kind == "http"
            && p.key == "POST /api/widgets/create"
            && p.file == "be/widgets.jsp"),
        "{:?}",
        provides
    );
}

// --- The `calls` channel: envelope-supplied call-graph edges light the call-graph-BFS rules ---
//
// Both sides are pinned in one place, red first: the SAME envelope minus its `calls` channel keeps
// `mutating-route-no-auth` silent (the pre-channel Mode A behavior — now DISCLOSED via the
// "Envelope call-graph gap" warning), and with the channel the rule fires / clears on real resolved
// edges. This is the feature's existence proof: a fact producers could never activate in Mode A
// before.

/// A Java-flavored envelope BE: one controller with a mutating route, one sibling file declaring the
/// guard symbol. `with_calls` controls the channel; `callee` is what the handler calls.
fn callgraph_envelope(with_calls: bool, callee: &str) -> NormalizedEnvelope {
    let controller_path = "src/main/java/OrderController.java";
    let guard_path = "src/main/java/AuthCheck.java";

    let mut controller = projection(controller_path, 40);
    controller.symbols.push(SourceSymbol {
        id: format!("{controller_path}#createOrder"),
        file: controller_path.to_string(),
        name: "createOrder".to_string(),
        kind: SourceSymbolKind::Function,
        line: 5,
        exported: true,
        is_default: false,
        body_start: Some(5),
        body_end: Some(20),
        write_sites: Vec::new(),
    });
    controller.symbols.push(SourceSymbol {
        id: format!("{controller_path}#saveOrder"),
        file: controller_path.to_string(),
        name: "saveOrder".to_string(),
        kind: SourceSymbolKind::Function,
        line: 25,
        exported: false,
        is_default: false,
        body_start: Some(25),
        body_end: Some(30),
        write_sites: Vec::new(),
    });
    controller.io.provides.push(IoProvide {
        body: None,
        response: None,
        kind: "http".to_string(),
        key: "POST /orders".to_string(),
        file: controller_path.to_string(),
        line: 5,
        symbol: Some("createOrder".to_string()),
    });
    controller.imports.insert(
        "verifyToken".to_string(),
        ImportBinding {
            specifier: guard_path.to_string(),
            original: "verifyToken".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    if with_calls {
        controller.calls.push(zzop_core::callgraph::RawCall {
            from_symbol: format!("{controller_path}#createOrder"),
            callee_name: callee.to_string(),
            line: 7,
            receiver_type: None,
            is_heritage: false,
        });
    }

    let mut guard = projection(guard_path, 12);
    guard.symbols.push(SourceSymbol {
        id: format!("{guard_path}#verifyToken"),
        file: guard_path.to_string(),
        name: "verifyToken".to_string(),
        kind: SourceSymbolKind::Function,
        line: 3,
        exported: true,
        is_default: false,
        body_start: Some(3),
        body_end: Some(8),
        write_sites: Vec::new(),
    });

    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "java-lexical/1".to_string(),
        source: "legacy-java".to_string(),
        files: vec![controller, guard],
    }
}

fn config_for(source: &str) -> EngineConfig {
    EngineConfig {
        source_id: source.to_string(),
        ..EngineConfig::default()
    }
}

/// RED side: without the channel, the rule is structurally silent — and that silence is DISCLOSED,
/// never mute ("Envelope call-graph gap" names the silent rules and the channel that opens them).
#[test]
fn envelope_without_calls_keeps_callgraph_rules_silent_and_discloses_it() {
    let out = analyze_envelope(&callgraph_envelope(false, ""), &config_for("legacy-java"));
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "mutating-route-no-auth"),
        "no edges -> the BFS rule must not fire: {:?}",
        out.findings
    );
    let gap = out
        .warnings
        .iter()
        .find(|w| w.contains("Envelope call-graph gap"))
        .expect("absence must be disclosed");
    assert!(gap.contains("mutating-route-no-auth"), "{gap}");
    assert!(gap.contains("files[].calls"), "{gap}");
}

/// GREEN side: the same envelope WITH calls fires the rule (unguarded handler) — the end-to-end pin
/// that the channel reaches the BFS. The absence disclosure must be gone.
#[test]
fn envelope_calls_light_mutating_route_no_auth_on_an_unguarded_handler() {
    let out = analyze_envelope(
        &callgraph_envelope(true, "saveOrder"),
        &config_for("legacy-java"),
    );
    let finding = out
        .findings
        .iter()
        .find(|f| f.rule_id == "mutating-route-no-auth")
        .unwrap_or_else(|| panic!("expected the rule to fire: {:?}", out.findings));
    assert_eq!(finding.file, "src/main/java/OrderController.java");
    assert_eq!(finding.line, 5);
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("Envelope call-graph gap")),
        "channel present -> no absence disclosure: {:?}",
        out.warnings
    );
}

/// The clearing half of GREEN: a handler whose supplied edge reaches an imported guard-named symbol
/// (`verifyToken`, resolved CROSS-FILE through the envelope's own imports + the guard file's symbol
/// set) is exempt — proof the edges are actually RESOLVED, not merely counted. And a fully-resolved
/// channel must carry NO drop disclosure (the zero side of `dropped_calls_warning`'s pin).
#[test]
fn envelope_calls_reaching_an_imported_guard_clear_the_route() {
    let out = analyze_envelope(
        &callgraph_envelope(true, "verifyToken"),
        &config_for("legacy-java"),
    );
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "mutating-route-no-auth"),
        "a guard-reaching handler must be cleared: {:?}",
        out.findings
    );
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("Envelope call-graph drop")),
        "every edge resolved -> no drop disclosure: {:?}",
        out.warnings
    );
}

/// B3 red->green: a supplied `calls` channel whose EVERY edge fails resolution (callee `save` is
/// declared nowhere and imported nowhere) must be DISCLOSED — before this fix the rule fired over an
/// empty graph with zero warnings, so "analyzed the graph" and "the whole graph evaporated" were
/// indistinguishable in the output. The drop itself is the resolver's contract (never guess); the
/// defect was the silence.
#[test]
fn envelope_fully_unresolved_calls_are_disclosed_as_a_total_drop() {
    let path = "app/users.py";
    let mut file = projection(path, 30);
    file.symbols.push(SourceSymbol {
        id: format!("{path}#create_user"),
        file: path.to_string(),
        name: "create_user".to_string(),
        kind: SourceSymbolKind::Function,
        line: 3,
        exported: true,
        is_default: false,
        body_start: Some(3),
        body_end: Some(10),
        write_sites: Vec::new(),
    });
    file.io.provides.push(IoProvide {
        body: None,
        response: None,
        kind: "http".to_string(),
        key: "POST /users".to_string(),
        file: path.to_string(),
        line: 3,
        symbol: Some("create_user".to_string()),
    });
    file.calls.push(zzop_core::callgraph::RawCall {
        from_symbol: format!("{path}#create_user"),
        callee_name: "save".to_string(), // declared nowhere -> the edge drops in resolution
        line: 5,
        receiver_type: None,
        is_heritage: false,
    });
    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "py-lexical/1".to_string(),
        source: "py".to_string(),
        files: vec![file],
    };
    let out = analyze_envelope(&envelope, &config_for("py"));

    // The drop is the contract — the rule still runs over the (empty) graph and fires.
    assert!(
        out.findings
            .iter()
            .any(|f| f.rule_id == "mutating-route-no-auth"),
        "{:?}",
        out.findings
    );
    let w = out
        .warnings
        .iter()
        .find(|w| w.contains("Envelope call-graph drop"))
        .unwrap_or_else(|| panic!("total evaporation must be disclosed: {:?}", out.warnings));
    assert!(w.contains("1 of 1"), "{w}");
    assert!(w.contains("app/users.py (1 of 1)"), "{w}");
    assert!(w.contains("never guessed"), "{w}");
    // Total evaporation gets the gap-grade phrasing: the rules walked an EMPTY graph.
    assert!(w.contains("EMPTY graph"), "{w}");
    assert!(w.contains("recall, not cleanliness"), "{w}");
    // No double-disclosure: the channel was supplied, so the absence warning must not also fire.
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("Envelope call-graph gap")),
        "{:?}",
        out.warnings
    );
}

/// The partial side of the same pin: one edge resolves, one drops — the disclosure names the dropped
/// count without the empty-graph phrasing (the rules DID walk the surviving edges).
#[test]
fn envelope_partially_unresolved_calls_name_the_dropped_count_only() {
    let mut envelope = callgraph_envelope(true, "verifyToken"); // resolves cross-file
    envelope.files[0].calls.push(zzop_core::callgraph::RawCall {
        from_symbol: "src/main/java/OrderController.java#createOrder".to_string(),
        callee_name: "ghostHelper".to_string(), // declared nowhere -> drops
        line: 9,
        receiver_type: None,
        is_heritage: false,
    });
    let out = analyze_envelope(&envelope, &config_for("legacy-java"));
    let w = out
        .warnings
        .iter()
        .find(|w| w.contains("Envelope call-graph drop"))
        .unwrap_or_else(|| panic!("a partial drop must be disclosed: {:?}", out.warnings));
    assert!(w.contains("1 of 2"), "{w}");
    assert!(
        w.contains("src/main/java/OrderController.java (1 of 2)"),
        "{w}"
    );
    assert!(
        !w.contains("EMPTY graph"),
        "partial drop must not claim total evaporation: {w}"
    );
}

// --- B5: `body`/`response` dtoRef resolution in Mode A (the same assemble-time resolution the
// native path runs — `resolve_provide_body_refs`/`resolve_provide_response_refs`, reused, never
// copied). Three pins: a dtoRef resolves against the envelope's own `class_shape_fragments` (envE),
// the no-return-type sentinel is stripped + disclosed instead of leaking into `ir` (envG), and an
// adapter-resolved direct-`fields` shape passes through untouched (envF).

#[test]
fn envelope_response_and_body_dto_refs_resolve_against_class_shape_fragments() {
    let controller_path = "legacy/UserController.jsp";
    let dto_path = "legacy/dto.jsp";
    let mut controller = projection(controller_path, 40);
    controller.io.provides.push(IoProvide {
        body: Some(zzop_core::ProvideBodyShape {
            sub_key: None,
            dto_ref: Some("CreateUserDto".to_string()),
            fields: Vec::new(),
            complete: false,
        }),
        response: Some(zzop_core::ProvideResponseShape {
            dto_ref: Some("UserDto".to_string()),
            fields: Vec::new(),
            complete: false,
        }),
        kind: "http".to_string(),
        key: "POST /users".to_string(),
        file: controller_path.to_string(),
        line: 5,
        symbol: Some("createUser".to_string()),
    });
    let mut dto = projection(dto_path, 12);
    dto.class_shape_fragments = vec![
        zzop_core::ClassShapeFragment {
            name: "UserDto".to_string(),
            fields: vec![zzop_core::ProvideBodyField {
                name: "passwordHash".to_string(),
                optional: false,
            }],
            complete: true,
        },
        zzop_core::ClassShapeFragment {
            name: "CreateUserDto".to_string(),
            fields: vec![zzop_core::ProvideBodyField {
                name: "email".to_string(),
                optional: false,
            }],
            complete: true,
        },
    ];
    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "legacy-jsp".to_string(),
        files: vec![controller, dto],
    };
    let out = analyze_envelope(&envelope, &config());
    let provides = out.ir.ir.io.expect("io facts").provides;
    let p = provides
        .iter()
        .find(|p| p.key == "POST /users")
        .expect("provide");
    let resp = p.response.as_ref().expect("resolved response survives");
    assert_eq!(resp.dto_ref, None, "resolved ref must be cleared: {resp:?}");
    assert_eq!(resp.fields.len(), 1, "{resp:?}");
    assert_eq!(resp.fields[0].name, "passwordHash");
    assert!(resp.complete);
    let body = p.body.as_ref().expect("resolved body survives");
    assert_eq!(body.dto_ref, None, "resolved ref must be cleared: {body:?}");
    assert_eq!(body.fields.len(), 1, "{body:?}");
    assert_eq!(body.fields[0].name, "email");
}

#[test]
fn envelope_no_return_type_sentinel_is_stripped_and_disclosed() {
    let path = "legacy/UserController.jsp";
    let mut controller = projection(path, 40);
    controller.io.provides.push(IoProvide {
        body: None,
        // The zero-information sentinel (`dtoRef: None` + empty fields) an adapter should not emit —
        // the engine must strip it in Mode A exactly as native assemble does, never let it reach `ir`.
        response: Some(zzop_core::ProvideResponseShape {
            dto_ref: None,
            fields: Vec::new(),
            complete: false,
        }),
        kind: "http".to_string(),
        key: "GET /users".to_string(),
        file: path.to_string(),
        line: 5,
        symbol: Some("listUsers".to_string()),
    });
    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "legacy-jsp".to_string(),
        files: vec![controller],
    };
    let out = analyze_envelope(&envelope, &config());
    let provides = out.ir.ir.io.expect("io facts").provides;
    assert_eq!(
        provides[0].response, None,
        "the sentinel must never survive into ir: {:?}",
        provides[0]
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("declare no return type")),
        "the strip must be disclosed, not silent: {:?}",
        out.warnings
    );
}

#[test]
fn envelope_adapter_resolved_response_fields_pass_through_untouched() {
    let path = "legacy/UserController.jsp";
    let shape = zzop_core::ProvideResponseShape {
        dto_ref: None,
        fields: vec![zzop_core::ProvideBodyField {
            name: "id".to_string(),
            optional: false,
        }],
        complete: true,
    };
    let mut controller = projection(path, 40);
    controller.io.provides.push(IoProvide {
        body: None,
        response: Some(shape.clone()),
        kind: "http".to_string(),
        key: "GET /users".to_string(),
        file: path.to_string(),
        line: 5,
        symbol: Some("listUsers".to_string()),
    });
    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "jsp-lexical/1".to_string(),
        source: "legacy-jsp".to_string(),
        files: vec![controller],
    };
    let out = analyze_envelope(&envelope, &config());
    let provides = out.ir.ir.io.expect("io facts").provides;
    assert_eq!(
        provides[0].response.as_ref(),
        Some(&shape),
        "a direct-fields shape is already resolved and must pass through unchanged"
    );
}

/// The write-site scanners consume the channel too, and are NOT extension-gated: a Ruby envelope
/// (no native parser, no covered extension) whose GET handler reaches a `writeSites`-carrying symbol
/// fires `unsafe-read-endpoint` — plus the residual disclosure that `mutating-route-no-auth`'s
/// covered-extension gate still exempts `.rb` routes even with supplied edges.
#[test]
fn envelope_calls_and_write_sites_light_unsafe_read_endpoint_for_an_uncovered_language() {
    let path = "app/orders.rb";
    let mut file = projection(path, 30);
    file.symbols.push(SourceSymbol {
        id: format!("{path}#index"),
        file: path.to_string(),
        name: "index".to_string(),
        kind: SourceSymbolKind::Function,
        line: 3,
        exported: true,
        is_default: false,
        body_start: Some(3),
        body_end: Some(10),
        write_sites: Vec::new(),
    });
    file.symbols.push(SourceSymbol {
        id: format!("{path}#write_audit"),
        file: path.to_string(),
        name: "write_audit".to_string(),
        kind: SourceSymbolKind::Function,
        line: 12,
        exported: false,
        is_default: false,
        body_start: Some(12),
        body_end: Some(18),
        write_sites: vec![zzop_core::WriteSite {
            file: path.to_string(),
            line: 14,
            sink: "INSERT INTO audits".to_string(),
            kind: None,
        }],
    });
    file.io.provides.push(IoProvide {
        body: None,
        response: None,
        kind: "http".to_string(),
        key: "GET /orders".to_string(),
        file: path.to_string(),
        line: 3,
        symbol: Some("index".to_string()),
    });
    file.calls.push(zzop_core::callgraph::RawCall {
        from_symbol: format!("{path}#index"),
        callee_name: "write_audit".to_string(),
        line: 5,
        receiver_type: None,
        is_heritage: false,
    });

    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "ruby-lexical/1".to_string(),
        source: "rb".to_string(),
        files: vec![file],
    };
    let out = analyze_envelope(&envelope, &config_for("rb"));

    let finding = out
        .findings
        .iter()
        .find(|f| f.rule_id == "unsafe-read-endpoint")
        .unwrap_or_else(|| panic!("expected unsafe-read-endpoint: {:?}", out.findings));
    assert_eq!(finding.file, path);
    assert_eq!(finding.line, 14, "anchors on the write site");
    // The residual gate on mutating-route-no-auth for uncovered extensions is named, not silent.
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("Envelope call-graph residual")),
        "{:?}",
        out.warnings
    );
}
