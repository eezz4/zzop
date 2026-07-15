//! End-to-end coverage for the generic entity-attribute injection channel
//! (`zzop_core::{Attribute, EntityRef, AttributeStore}`) as consumed through a Mode-B
//! `EngineConfig::adapter_overlays` overlay (same injection surface `analyze_adapter_overlay.rs`
//! exercises for dep-graph/io facts) — proving the channel completes TWO real rules end to end:
//!
//! - `mutating-route-no-auth` (`zzop_rules_http::mutating_route_no_auth`): a router-level middleware
//!   guard the call-graph BFS cannot see is completed by an injected `AUTH_GUARDED_ATTR` attribute, either
//!   as an exact route `IoKey` or a `PathScope` prefix — either clears the route, composing with (not
//!   replacing) the native BFS.
//! - `zzop_rules_schema::usage` (`dead-model` / `schema-churn`): the retrofitted Symbol-keyed
//!   `BOUND_MODEL_ATTR`/`MODEL_CHURN_ATTR` attributes — store-binding and migration-churn are environment
//!   facts with no native recognizer any more, so `dead-model` is suppressible and `schema-churn` is only
//!   reachable at all through this channel.
//!
//! Self-contained: helpers (`TempDir`, `config`, `projection`, `overlay`) are copied/adapted from
//! `analyze_adapter_overlay.rs` and `analyze_io_natives.rs` rather than shared, since each `tests/*.rs`
//! file is its own separate test binary/crate.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use zzop_core::{
    Attribute, EntityRef, FileProjection, IoFacts, NormalizedEnvelope, NORMALIZED_AST_FORMAT,
};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};
use zzop_rules_http::mutating_route_no_auth::AUTH_GUARDED_ATTR;
use zzop_rules_schema::usage::{BOUND_MODEL_ATTR, MODEL_CHURN_ATTR};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as
/// `analyze_adapter_overlay.rs`/`analyze_io_natives.rs`).
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

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "attribute-injection-fixture".to_string(),
        ..EngineConfig::default()
    }
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings.iter().filter(|f| f.rule_id == rule).collect()
}

/// A minimal, all-empty `FileProjection` — same defaults `analyze_adapter_overlay.rs`'s own
/// `projection()` helper uses.
fn projection(path: &str, loc: u32) -> FileProjection {
    FileProjection {
        class_shape_fragments: Vec::new(),
        path: path.to_string(),
        loc,
        symbols: Vec::new(),
        imports: zzop_core::ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
        is_entry: false,
        attributes: Vec::new(),
        loop_spans: Vec::new(),
    }
}

/// Same as `projection`, but carrying `attrs` on its `attributes` channel — the variant this file's
/// tests actually need, since the plain `projection()` helper always sets `attributes: Vec::new()`.
fn projection_with_attrs(path: &str, loc: u32, attrs: Vec<Attribute>) -> FileProjection {
    FileProjection {
        attributes: attrs,
        ..projection(path, loc)
    }
}

fn overlay(parser: &str, files: Vec<FileProjection>) -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: parser.to_string(),
        source: "adapter".to_string(),
        files,
    }
}

/// A single-file overlay whose one `FileProjection` carries exactly `attrs` — the placeholder path/loc
/// are irrelevant to `AttributeStore::from_overlays`, which flattens `attributes` across every overlay
/// file regardless of path (attributes are entity-addressed, not file-addressed).
fn overlay_with_attrs(parser: &str, attrs: Vec<Attribute>) -> NormalizedEnvelope {
    overlay(
        parser,
        vec![projection_with_attrs("overlay/attrs.json", 1, attrs)],
    )
}

// ---------------------------------------------------------------------------------------------
// mutating-route-no-auth: injected `auth-guarded` attribute completion
// ---------------------------------------------------------------------------------------------

/// A mutating route under `/admin` whose handler never calls anything guard-shaped — fires
/// `mutating-route-no-auth` with no overlay at all (same Hono-registration shape
/// `analyze_io_natives.rs`'s own `mutating_no_auth_fixture` uses, moved under `/admin`).
fn mutating_admin_fixture() -> TempDir {
    let dir = TempDir::new("zzop-attr-injection-mutating");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/admin/widgets\", createWidget);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function createWidget(c) {\n  return prisma.widget.create({ data: {} });\n}\n",
    );
    dir
}

#[test]
fn auth_guarded_pathscope_injection_suppresses_mutating_route_no_auth() {
    let dir = mutating_admin_fixture();

    // Baseline: no overlay at all — the handler never calls anything guard-named, so this fires.
    let baseline = analyze_tree(dir.path(), &config());
    let found = hits(&baseline, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", baseline.findings);
    assert_eq!(found[0].file, "routes/api.ts");

    // Injected `auth-guarded` PathScope covering `/admin` — router-level middleware the BFS can't see.
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay_with_attrs(
        "auth-overlay-adapter/1",
        vec![Attribute {
            target: EntityRef::PathScope {
                prefix: "/admin".to_string(),
            },
            key: AUTH_GUARDED_ATTR.to_string(),
            value: json!(true),
        }],
    )];
    let out = analyze_tree(dir.path(), &cfg);
    assert_eq!(
        hits(&out, "mutating-route-no-auth").len(),
        0,
        "{:?}",
        out.findings
    );
}

#[test]
fn auth_guarded_iokey_exact_injection_suppresses() {
    let dir = mutating_admin_fixture();

    let baseline = analyze_tree(dir.path(), &config());
    assert_eq!(
        hits(&baseline, "mutating-route-no-auth").len(),
        1,
        "{:?}",
        baseline.findings
    );

    // Exact route `IoKey` injection instead of a PathScope — matches the fixture's own registered
    // `POST /admin/widgets` route precisely.
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay_with_attrs(
        "auth-overlay-adapter/1",
        vec![Attribute {
            target: EntityRef::IoKey {
                kind: "http".to_string(),
                key: "POST /admin/widgets".to_string(),
            },
            key: AUTH_GUARDED_ATTR.to_string(),
            value: json!(true),
        }],
    )];
    let out = analyze_tree(dir.path(), &cfg);
    assert_eq!(
        hits(&out, "mutating-route-no-auth").len(),
        0,
        "{:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// schema-usage: injected `bound-model` / `model-churn` Symbol attributes
// ---------------------------------------------------------------------------------------------

#[test]
fn bound_model_symbol_injection_suppresses_dead_model() {
    let dir = TempDir::new("zzop-attr-injection-bound-model");
    dir.write(
        "prisma/schema.prisma",
        "model Ghost {\n  id String @id\n  payload String\n}\n",
    );

    // Baseline: `Ghost` is never referenced anywhere in BE source and no overlay is present -> dead-model.
    let baseline = analyze_tree(dir.path(), &config());
    let dead = hits(&baseline, "schema/dead-model");
    assert_eq!(dead.len(), 1, "{:?}", baseline.findings);
    assert_eq!(
        dead[0].data.as_ref().unwrap()["model"].as_str(),
        Some("Ghost")
    );

    // Injected `bound-model` on the `Ghost` Symbol (a producer that knows a store-binding convention the
    // engine no longer models natively) suppresses it.
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay_with_attrs(
        "auth-overlay-adapter/1",
        vec![Attribute {
            target: EntityRef::Symbol {
                name: "Ghost".to_string(),
                file: None,
            },
            key: BOUND_MODEL_ATTR.to_string(),
            value: json!(true),
        }],
    )];
    let out = analyze_tree(dir.path(), &cfg);
    assert_eq!(
        hits(&out, "schema/dead-model").len(),
        0,
        "{:?}",
        out.findings
    );
}

#[test]
fn model_churn_symbol_injection_fires_schema_churn_critical() {
    let dir = TempDir::new("zzop-attr-injection-model-churn");
    dir.write(
        "prisma/schema.prisma",
        "model Wobbly {\n  id String @id\n  payload String\n}\n",
    );
    // Referenced as a real identifier (not inside a string literal, which `field_usage_tokens` strips)
    // so dead-model does NOT fire — isolating this test to the churn signal alone.
    dir.write(
        "src/service.ts",
        "import { Wobbly } from \"./types\";\nexport function useWobbly(w: Wobbly) {\n  return w;\n}\n",
    );

    // Baseline: no overlay at all -> `Wobbly` is referenced (no dead-model) and carries no churn count
    // (churn is injection-only now, so it self-gates to zero with an empty attribute store).
    let baseline = analyze_tree(dir.path(), &config());
    assert_eq!(
        hits(&baseline, "schema/dead-model").len(),
        0,
        "{:?}",
        baseline.findings
    );
    assert_eq!(
        hits(&baseline, "schema/schema-churn").len(),
        0,
        "{:?}",
        baseline.findings
    );

    // Injected `model-churn` count of 12 (>= the critical threshold of 10) on the `Wobbly` Symbol.
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay_with_attrs(
        "auth-overlay-adapter/1",
        vec![Attribute {
            target: EntityRef::Symbol {
                name: "Wobbly".to_string(),
                file: None,
            },
            key: MODEL_CHURN_ATTR.to_string(),
            value: json!(12),
        }],
    )];
    let out = analyze_tree(dir.path(), &cfg);
    let churn = hits(&out, "schema/schema-churn");
    assert_eq!(churn.len(), 1, "{:?}", out.findings);
    assert_eq!(churn[0].severity, zzop_core::Severity::Critical);
    assert_eq!(
        churn[0].data.as_ref().unwrap()["model"].as_str(),
        Some("Wobbly")
    );
}
