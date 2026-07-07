//! e2e coverage for `EngineConfig::adapter_overlays` (Mode B: a partial `NormalizedEnvelope` merged
//! onto a NATIVE `analyze_tree` run's per-file artifacts before whole-tree assembly) — the
//! external-adapter injection point that lets a framework adapter contribute `io`/fragment data to a
//! native run without reimplementing a full parser, contrasted with Mode A (`analyze_envelope`, a full
//! envelope standing in for the entire tree — see `analyze_envelope.rs`'s own tests).
//!
//! - `composed_provide_is_joinable_with_a_native_consume`: an overlay's `router_mount_fragments`,
//!   split across two `FileProjection`s (a mount file + a sub-router file with a `Verb` entry), compose
//!   (via `apply_adapter_overlays` -> whole-tree `assemble` -> `compose_router_mount_provides`, the same
//!   composer Mode A reuses) into an `http` `IoProvide` whose key exactly matches a native TS file's own
//!   `axios.post(...)` egress consume — proving the overlay's composed PROVIDE and the native CONSUME
//!   are join-compatible, not just independently present.
//! - `duplicate_provide_from_overlay_does_not_double_count`: an overlay re-emits the EXACT
//!   `(kind, key, file, line)` of a provide the native pass already produced — proves
//!   `apply_adapter_overlays`'s dedup guard collapses it to one entry, not two.
//! - `invalid_overlay_produces_a_warning_and_never_crashes_the_native_run`: an overlay with a bad
//!   `format` string fails `zzop_core::validate_envelope` — proves it is skipped with one `warnings`
//!   entry naming the overlay's `parser` id, never a panic, and the native tree's own output is
//!   otherwise unaffected.
//! - `projection_for_an_unknown_rel_still_contributes_its_provide`: an overlay carries a
//!   `FileProjection` whose `path` matches no file in the native tree at all — proves the
//!   synthetic-`FileArtifact` push path works (not just the merge-onto-existing-artifact path).
//! - `overlay_import_from_a_synthetic_projection_gives_a_native_target_fan_in`: the dep-graph-completion
//!   contract (the injection contract extends past io/fragments to dep-graph facts, so any non-TS
//!   adapter can complete the graph) — a synthetic overlay `FileProjection` (a `.svelte` path the native
//!   dispatch table never recognizes) whose `imports` names a native TS module with no OTHER importer
//!   anywhere in the tree.
//!   Asserts BOTH directions: without the overlay the TS module reads as a `dead-candidates` finding;
//!   with it, the overlay's import edge gives it real fan-in and the finding disappears.
//! - `overlay_is_entry_marker_exempts_an_otherwise_dead_native_file`: an overlay `FileProjection` for an
//!   existing native TS file's own path, marked `is_entry: true` — proves `assemble` unions it into
//!   `dead_candidate_findings`'s `extra_entries` (the overlay counterpart of a package.json manifest
//!   entry), suppressing a finding that fires with no overlay at all.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{
    FileProjection, ImportBinding, IoFacts, IoProvide, NormalizedEnvelope, RouterMountEntry,
    RouterMountFragment, NORMALIZED_AST_FORMAT,
};
use zzop_engine::{analyze_tree, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as `analyze_routes_hono.rs`/
/// `analyze_io.rs`; this crate's test files do not share a common test-utils module).
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
        source_id: "adapter-overlay-fixture".to_string(),
        ..EngineConfig::default()
    }
}

/// A minimal, all-empty `FileProjection` — same defaults `analyze_envelope.rs`'s own `projection()`
/// helper uses, so a test only needs to fill in the one or two fields it actually cares about.
fn projection(path: &str, loc: u32) -> FileProjection {
    FileProjection {
        path: path.to_string(),
        loc,
        symbols: Vec::new(),
        imports: zzop_core::ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: HashMap::new(),
        trpc_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
        is_entry: false,
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

#[test]
fn composed_provide_is_joinable_with_a_native_consume() {
    const JOIN_KEY: &str = "POST /api/auth/two-factor/setup";

    let dir = TempDir::new("zzop-adapter-overlay");
    // Native FE-ish file: a literal-path `axios.post(...)` egress call resolves immediately (no
    // ControlKey indirection needed) to a `key: Some("POST /api/auth/two-factor/setup")` consume.
    dir.write(
        "src/app.ts",
        "export function callSetup() { return axios.post(\"/api/auth/two-factor/setup\"); }\n",
    );
    // Native placeholder files at the overlay's own target paths — needed so `resolve_file_with_workspace`
    // (the resolver `analyze::assemble` passes to `compose_router_mount_provides`) can resolve the
    // mount's relative `./twoFactor` specifier: that resolver only ever matches against `ts_paths`,
    // which is populated from files the native pass actually dispatched and parsed (see
    // `pipeline::run_file_pass`) — an overlay-only synthetic path never joins `ts_paths`. Content is
    // irrelevant; only the paths' presence in the native tree matters here.
    dir.write("src/routes/index.ts", "export const noop = 1;\n");
    dir.write("src/routes/twoFactor.ts", "export const noop2 = 1;\n");

    let mut mount = projection("src/routes/index.ts", 1);
    mount.router_mount_fragments.push(RouterMountFragment {
        name: "app".to_string(),
        entries: vec![RouterMountEntry::Mount {
            prefix: "/api/auth/two-factor".to_string(),
            ident: "twoFactorRoute".to_string(),
            specifier: Some("./twoFactor".to_string()),
        }],
    });
    let mut sub = projection("src/routes/twoFactor.ts", 1);
    sub.router_mount_fragments.push(RouterMountFragment {
        name: "twoFactorRoute".to_string(),
        entries: vec![RouterMountEntry::Verb {
            method: "POST".to_string(),
            path: "/setup".to_string(),
            handler: None,
            line: 1,
        }],
    });

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("test-router-adapter/1", vec![mount, sub])];
    let out = analyze_tree(dir.path(), &cfg);

    let io = out.ir.ir.io.expect("expected io facts");
    assert!(
        io.provides
            .iter()
            .any(|p| p.kind == "http" && p.key == JOIN_KEY),
        "{:?}",
        io.provides
    );
    assert!(
        io.consumes
            .iter()
            .any(|c| c.kind == "http" && c.key.as_deref() == Some(JOIN_KEY)),
        "{:?}",
        io.consumes
    );
}

#[test]
fn duplicate_provide_from_overlay_does_not_double_count() {
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write(
        "routes/apiRoutes.ts",
        "@Controller('authen')\nclass AuthenController {\n  @Get('getUserInfo')\n  getUserInfo() {}\n}\n",
    );

    // Capture the native pass's own provide exactly (kind/key/file/line) rather than guessing the
    // decorator adapter's line-numbering convention — the overlay below re-emits this verbatim.
    let baseline = analyze_tree(dir.path(), &config());
    let native_provide = baseline
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected io facts")
        .provides
        .iter()
        .find(|p| p.kind == "http" && p.key == "GET /authen/getUserInfo")
        .cloned()
        .expect("expected a native NestJS-decorator provide");

    let mut proj = projection(&native_provide.file, 4);
    proj.io.provides.push(native_provide.clone());

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("test-dup-adapter/1", vec![proj])];
    let out = analyze_tree(dir.path(), &cfg);

    let matches: Vec<_> = out
        .ir
        .ir
        .io
        .expect("expected io facts")
        .provides
        .into_iter()
        .filter(|p| p.kind == native_provide.kind && p.key == native_provide.key)
        .collect();
    assert_eq!(matches.len(), 1, "{:?}", matches);
}

#[test]
fn invalid_overlay_produces_a_warning_and_never_crashes_the_native_run() {
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let bad = NormalizedEnvelope {
        format: "not-the-right-format".to_string(),
        version: 1,
        parser: "test-bad-adapter/1".to_string(),
        source: "adapter".to_string(),
        files: Vec::new(),
    };

    let mut cfg = config();
    cfg.adapter_overlays = vec![bad];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("test-bad-adapter/1")),
        "{:?}",
        out.warnings
    );
    // The native tree's own output is otherwise a normal, successful result — no panic, and the one
    // real native file was analyzed exactly as it would be with no overlay configured at all.
    assert_eq!(out.file_count, 1);
    assert!(out.degraded.is_empty());
}

#[test]
fn projection_for_an_unknown_rel_still_contributes_its_provide() {
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let mut proj = projection("external/legacy.jsp", 5);
    proj.io.provides.push(IoProvide {
        kind: "http".to_string(),
        key: "GET /legacy/status".to_string(),
        file: "external/legacy.jsp".to_string(),
        line: 2,
        symbol: None,
    });

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("test-unknown-rel-adapter/1", vec![proj])];
    let out = analyze_tree(dir.path(), &cfg);

    let provides = out.ir.ir.io.expect("expected io facts").provides;
    assert!(
        provides
            .iter()
            .any(|p| p.key == "GET /legacy/status" && p.file == "external/legacy.jsp"),
        "{:?}",
        provides
    );
}

#[test]
fn overlay_import_from_a_synthetic_projection_gives_a_native_target_fan_in() {
    // Mode B dep-graph-completion contract: the injection contract extends past io/fragments to
    // dep-graph facts, so a synthetic overlay `FileProjection` (a `.svelte` path the native dispatch
    // table never recognizes) whose `imports` names a native TS module gives that module real fan-in,
    // exactly like a native TS importer would.
    let dir = TempDir::new("zzop-adapter-overlay");
    // No other file in the tree imports this — dead-candidate bait without the overlay.
    dir.write("src/used.ts", "export const helper = 1;\n");

    let baseline = analyze_tree(dir.path(), &config());
    assert!(
        baseline
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/used.ts"),
        "expected src/used.ts to read as dead with no overlay at all: {:?}",
        baseline.findings
    );

    // A `.svelte` file the native dispatch table never dispatches at all — pushed down the synthetic
    // (no-native-match) branch of `apply_adapter_overlays`. Its ONLY import is a relative specifier
    // (`./used`, resolved from its own directory via the same `resolve_file_with_workspace` the native
    // path uses — NOT `analyze_envelope`'s exact-path-first resolver, since this is Mode B).
    let mut view = projection("src/View.svelte", 6);
    view.imports.insert(
        "helper".to_string(),
        ImportBinding {
            specifier: "./used".to_string(),
            original: "helper".to_string(),
            deferred: false,
            type_only: false,
        },
    );

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("svelte-adapter/1", vec![view])];
    let out = analyze_tree(dir.path(), &cfg);

    // The overlay edge is real dep-graph data, not just a suppression side effect.
    assert!(
        out.ir
            .ir
            .dep
            .get("src/View.svelte")
            .cloned()
            .unwrap_or_default()
            .contains(&"src/used.ts".to_string()),
        "{:?}",
        out.ir.ir.dep
    );
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/used.ts"),
        "expected the overlay's import edge to give src/used.ts fan-in: {:?}",
        out.findings
    );
}

#[test]
fn overlay_is_entry_marker_exempts_an_otherwise_dead_native_file() {
    // `FileProjection::is_entry` — the overlay counterpart of a package.json manifest entry: `assemble`
    // unions every `is_entry: true` overlay path into `dead_candidate_findings`'s `extra_entries`.
    let dir = TempDir::new("zzop-adapter-overlay");
    // Not an entry-pattern name (no `index`/`main`/`App`/...), so it reads as dead with no overlay.
    dir.write("src/hooks.server.ts", "export const noop = 1;\n");

    let baseline = analyze_tree(dir.path(), &config());
    assert!(
        baseline
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/hooks.server.ts"),
        "expected src/hooks.server.ts to read as dead with no overlay at all: {:?}",
        baseline.findings
    );

    // Merges onto the EXISTING native artifact at the same path (not the synthetic branch) — `is_entry`
    // is read straight from `config.adapter_overlays` in `assemble`, so it applies regardless of which
    // merge branch a projection's `path` takes.
    let mut hooks = projection("src/hooks.server.ts", 1);
    hooks.is_entry = true;

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("sveltekit-entry-adapter/1", vec![hooks])];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/hooks.server.ts"),
        "expected is_entry to exempt src/hooks.server.ts from dead-candidates: {:?}",
        out.findings
    );
}

#[test]
fn overlay_import_onto_a_degraded_on_disk_file_gives_a_target_fan_in() {
    // The MERGE-onto-existing branch (distinct from the synthetic branch above), and the one the real
    // SvelteKit case actually hits: the native pass walks EVERY file, so a `.svelte` file present ON DISK
    // becomes a degraded artifact with `imports: None`. An overlay for that same path then FILLS its
    // dep-graph data (a parsed TS artifact's own imports would instead stay authoritative — only a
    // `None` is filled).
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/used.ts", "export const helper = 1;\n");
    // Present on disk -> native pass makes a degraded artifact for it (dispatch table can't parse .svelte).
    dir.write(
        "src/View.svelte",
        "<script>import { helper } from './used';</script>\n",
    );

    let baseline = analyze_tree(dir.path(), &config());
    assert!(
        baseline
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/used.ts"),
        "the native pass can't parse .svelte, so src/used.ts reads as dead without the overlay: {:?}",
        baseline.findings
    );

    let mut view = projection("src/View.svelte", 1);
    view.imports.insert(
        "helper".to_string(),
        ImportBinding {
            specifier: "./used".to_string(),
            original: "helper".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("svelte-adapter/1", vec![view])];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        out.ir
            .ir
            .dep
            .get("src/View.svelte")
            .cloned()
            .unwrap_or_default()
            .contains(&"src/used.ts".to_string()),
        "the overlay must fill the degraded on-disk .svelte artifact's dep edges: {:?}",
        out.ir.ir.dep
    );
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/used.ts"),
        "expected the overlay edge on the degraded .svelte artifact to give src/used.ts fan-in: {:?}",
        out.findings
    );
}
