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
        path: path.to_string(),
        loc,
        symbols: Vec::new(),
        imports: ImportMap::new(),
        re_exports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: std::collections::HashMap::new(),
        trpc_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
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
        version: 1,
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
        kind: "http".to_string(),
        key: "GET /legacy/user.jsp".to_string(),
        file: "legacy/UserController.jsp".to_string(),
        line: 5,
        symbol: Some("getUser".to_string()),
    });
    let be_envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
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
        version: 1,
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
        }],
    });

    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
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
