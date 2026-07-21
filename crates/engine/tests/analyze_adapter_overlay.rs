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
//! - `overlay_nest_global_prefix_provide_is_dropped_and_warned_not_reapplied_tree_wide`: an overlay emits
//!   a reserved `nest-global-prefix` sentinel `IoProvide` — producer-forbidden (only the native TS parser
//!   may emit it). Proves `apply_adapter_overlays` drops it before merge (never reaches
//!   `apply_and_strip_global_prefix`, so it can't re-prefix the WHOLE native tree's routes), pushes one
//!   aggregate warning naming the overlay's `parser`, and a native controller's own route in the SAME tree
//!   is untouched.
//! - `overlay_client_base_prefix_consume_is_dropped_and_warned`: the `IoConsume`-side counterpart — an
//!   overlay emits `zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND`, proves it is dropped + warned the
//!   same way.
//! - `overlay_with_only_ordinary_io_kinds_merges_with_no_drop_warning`: a control case — an overlay whose
//!   `io` carries only ordinary (non-reserved) kinds merges in full, with no drop warning at all, proving
//!   the reserved-kind filter has no false-positive reach.
//!
//! G3/G8 overlay-disclosure coverage (both warnings-only, structural facts about an overlay's own shape,
//! never findings/cache/fingerprint changes):
//! - `mismatched_overlay_source_warns_about_the_intra_source_join`: an overlay declaring `source: "dbx"`
//!   attached to a tree whose `EngineConfig::source_id` is `"spring"` — proves the G3 self-report fires,
//!   naming both the overlay's own declared source and the tree it actually merges into.
//! - `overlay_source_equal_to_the_tree_source_id_warns_nothing`: control case — `overlay.source` equal to
//!   `source_id` makes no mismatch claim.
//! - `overlay_with_empty_source_warns_nothing`: control case — an empty `source` string makes no claim at
//!   all, so it is never flagged even though it technically differs from `source_id`.
//! - `synthetic_entry_sample_caps_at_three_with_a_plus_n_more_suffix`: 5 declared paths that all match no
//!   file in the tree — proves the G8a synthetic-entry warning samples at most 3 and appends `+2 more`.
//! - `is_entry_only_overlay_counts_as_fact_carrying_no_zero_fact_warning`: an overlay entry whose ONLY
//!   non-default field is `is_entry: true` (every other channel empty) — proves the G8b zero-fact warning
//!   does not fire for it, i.e. `is_entry` alone is enough to count as a real fact.
//! - `overlay_warnings_are_byte_for_byte_identical_across_two_runs`: the same multi-overlay config run
//!   twice — proves G3/G8's new warnings are exactly as deterministic as every existing overlay warning
//!   (parser-sorted overlay order, no HashMap-iteration nondeterminism).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{
    FileProjection, ImportBinding, IoConsume, IoFacts, IoProvide, NormalizedEnvelope,
    RouterMountEntry, RouterMountFragment, SourceSymbol, SourceSymbolKind, NORMALIZED_AST_FORMAT,
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
            attr_keys: vec![],
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
            attr_keys: vec![],
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
        body: None,
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
    // A genuinely-dead file: not an entry-pattern name (no `index`/`main`/`App`/...) and not a framework
    // convention file, so it reads as dead with no overlay. (Deliberately NOT `hooks.server.ts` — that IS
    // a recognized SvelteKit hook entry now, so it would never read as dead; the overlay `is_entry` path
    // is what this test exercises, on a file that has no native entry recognition.)
    dir.write("src/orphan.ts", "export const noop = 1;\n");

    let baseline = analyze_tree(dir.path(), &config());
    assert!(
        baseline
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/orphan.ts"),
        "expected src/orphan.ts to read as dead with no overlay at all: {:?}",
        baseline.findings
    );

    // Merges onto the EXISTING native artifact at the same path (not the synthetic branch) — `is_entry`
    // is read straight from `config.adapter_overlays` in `assemble`, so it applies regardless of which
    // merge branch a projection's `path` takes.
    let mut orphan = projection("src/orphan.ts", 1);
    orphan.is_entry = true;

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("entry-adapter/1", vec![orphan])];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/orphan.ts"),
        "expected is_entry to exempt src/orphan.ts from dead-candidates: {:?}",
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
    //
    // The `.svelte`'s own on-disk `<script>` body deliberately uses a DYNAMIC `import()` rather than a
    // static one: the native SFC `<script>`-block pre-scan (`zzop_parser_typescript::
    // extract_sfc_script_imports`, wired at `analyze::assemble::sfc`) now gives a STATIC import real
    // fan-in without any overlay at all, so a static-import baseline would no longer reproduce the gap
    // this test exists to cover. A dynamic `import()` is outside that pre-scan's scope (it calls
    // `parse_imports` only, which never collects dynamic imports — see that function's own doc), so the
    // baseline below still needs the overlay to see this file's real dependency.
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/used.ts", "export const helper = 1;\n");
    // Present on disk -> native pass makes a degraded artifact for it (dispatch table can't parse .svelte).
    dir.write(
        "src/View.svelte",
        "<script>import('./used').then(m => m.helper);</script>\n",
    );

    let baseline = analyze_tree(dir.path(), &config());
    assert!(
        baseline
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/used.ts"),
        "the native pass can't see a dynamic import() inside .svelte, so src/used.ts reads as dead \
         without the overlay: {:?}",
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

// --- Reserved engine-internal sentinel kinds are producer-forbidden in overlays (audit item 2-2) ---

#[test]
fn overlay_nest_global_prefix_provide_is_dropped_and_warned_not_reapplied_tree_wide() {
    // Native NestJS controller with its own real route, no `setGlobalPrefix` call anywhere in the tree —
    // its route must come through completely unprefixed regardless of the overlay below.
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write(
        "src/user.controller.ts",
        "@Controller('users')\nclass UserController {\n  @Get('list')\n  list() {}\n}\n",
    );

    let mut proj = projection("external/legacy.jsp", 3);
    proj.io.provides.push(IoProvide {
        body: None,
        kind: "nest-global-prefix".to_string(),
        key: "api".to_string(),
        file: "external/legacy.jsp".to_string(),
        line: 1,
        symbol: None,
    });

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("test-nest-prefix-adapter/1", vec![proj])];
    let out = analyze_tree(dir.path(), &cfg);

    let provides = out.ir.ir.io.expect("expected io facts").provides;
    // The reserved sentinel must never reach output — dropped before merge, not just stripped later.
    assert!(
        !provides.iter().any(|p| p.kind == "nest-global-prefix"),
        "{:?}",
        provides
    );
    // The native controller's own route must NOT have been re-prefixed by the overlay's sentinel: had it
    // survived the merge, `apply_and_strip_global_prefix` would have prepended `/api` to every `http`
    // provide in the WHOLE tree, including this one.
    assert!(
        provides
            .iter()
            .any(|p| p.kind == "http" && p.key == "GET /users/list"),
        "{:?}",
        provides
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("test-nest-prefix-adapter/1")
                && w.contains("dropped 1 reserved engine-internal io entry")
                && w.contains("nest-global-prefix")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn overlay_client_base_prefix_consume_is_dropped_and_warned() {
    // `IoConsume`-side counterpart of the provide test above: an overlay emitting
    // `zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND` directly is producer-forbidden the same way.
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let mut proj = projection("external/legacy.jsp", 2);
    proj.io.consumes.push(IoConsume {
        kind: zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND.to_string(),
        key: Some("/api".to_string()),
        file: "external/legacy.jsp".to_string(),
        line: 1,
        raw: None,
        method: None,
        body: None,
        client: Some("axios".to_string()),
    });

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("test-client-base-adapter/1", vec![proj])];
    let out = analyze_tree(dir.path(), &cfg);

    let consumes = out.ir.ir.io.map(|io| io.consumes).unwrap_or_default();
    assert!(
        !consumes
            .iter()
            .any(|c| c.kind == zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND),
        "{:?}",
        consumes
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("test-client-base-adapter/1")
                && w.contains("dropped 1 reserved engine-internal io entry")
                && w.contains("client-base-prefix")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn overlay_with_only_ordinary_io_kinds_merges_with_no_drop_warning() {
    // Control case: an overlay whose io carries only ordinary (non-reserved) kinds must merge in full,
    // with no drop warning at all — the reserved-kind filter must not have false-positive reach.
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let mut proj = projection("external/legacy.jsp", 4);
    proj.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /legacy/ping".to_string(),
        file: "external/legacy.jsp".to_string(),
        line: 2,
        symbol: None,
    });

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("test-ordinary-io-adapter/1", vec![proj])];
    let out = analyze_tree(dir.path(), &cfg);

    let provides = out.ir.ir.io.expect("expected io facts").provides;
    assert!(
        provides
            .iter()
            .any(|p| p.kind == "http" && p.key == "GET /legacy/ping"),
        "{:?}",
        provides
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains("dropped")),
        "{:?}",
        out.warnings
    );
}

// --- G3: overlay source-mismatch self-report (audit item G3) ---

#[test]
fn mismatched_overlay_source_warns_about_the_intra_source_join() {
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let mut cfg = EngineConfig {
        source_id: "spring".to_string(),
        ..EngineConfig::default()
    };
    let mut proj = projection("external/legacy.jsp", 3);
    proj.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /legacy/ping".to_string(),
        file: "external/legacy.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    cfg.adapter_overlays = vec![NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "dbx-adapter/1".to_string(),
        source: "dbx".to_string(),
        files: vec![proj],
    }];
    let out = analyze_tree(dir.path(), &cfg);

    let mismatch = out
        .warnings
        .iter()
        .find(|w| w.contains("declares a different source"))
        .unwrap_or_else(|| {
            panic!(
                "expected a source-mismatch warning, got: {:?}",
                out.warnings
            )
        });
    assert!(mismatch.contains("\"dbx\""), "{mismatch}");
    assert!(mismatch.contains("dbx-adapter/1"), "{mismatch}");
    assert!(mismatch.contains("\"spring\""), "{mismatch}");
    assert!(mismatch.contains("overlays: [...]"), "{mismatch}");
    assert!(mismatch.contains("adapterOverlays"), "{mismatch}");
}

#[test]
fn overlay_source_equal_to_the_tree_source_id_warns_nothing() {
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let mut cfg = EngineConfig {
        source_id: "spring".to_string(),
        ..EngineConfig::default()
    };
    let mut proj = projection("external/legacy.jsp", 3);
    proj.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /legacy/ping".to_string(),
        file: "external/legacy.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    cfg.adapter_overlays = vec![NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "spring-adapter/1".to_string(),
        source: "spring".to_string(),
        files: vec![proj],
    }];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("declares a different source")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn overlay_with_empty_source_warns_nothing() {
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let mut cfg = EngineConfig {
        source_id: "spring".to_string(),
        ..EngineConfig::default()
    };
    let mut proj = projection("external/legacy.jsp", 3);
    proj.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /legacy/ping".to_string(),
        file: "external/legacy.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    cfg.adapter_overlays = vec![NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "no-source-adapter/1".to_string(),
        source: String::new(),
        files: vec![proj],
    }];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("declares a different source")),
        "an empty `source` makes no claim, so it must never be flagged: {:?}",
        out.warnings
    );
}

// --- G8: overlay contribution census (synthetic-entry + zero-fact self-reports) ---

#[test]
fn synthetic_entry_sample_caps_at_three_with_a_plus_n_more_suffix() {
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    // 5 declared paths, none matching any file in the tree — all 5 land on the synthetic branch.
    let files: Vec<FileProjection> = (1..=5)
        .map(|n| projection(&format!("external/legacy{n}.jsp"), 1))
        .collect();

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("bulk-adapter/1", files)];
    let out = analyze_tree(dir.path(), &cfg);

    let synthetic_line = out
        .warnings
        .iter()
        .find(|w| w.contains("added as synthetic entries"))
        .unwrap_or_else(|| {
            panic!(
                "expected a synthetic-entry warning, got: {:?}",
                out.warnings
            )
        });
    assert!(
        synthetic_line.starts_with(
            "adapter overlay \"adapter\" (parser bulk-adapter/1): 5 of 5 declared file(s) matched no file in this tree"
        ),
        "{synthetic_line}"
    );
    // Sampled at most 3, with a "+2 more" suffix for the remainder.
    assert!(
        synthetic_line.contains("external/legacy1.jsp"),
        "{synthetic_line}"
    );
    assert!(
        synthetic_line.contains("external/legacy2.jsp"),
        "{synthetic_line}"
    );
    assert!(
        synthetic_line.contains("external/legacy3.jsp"),
        "{synthetic_line}"
    );
    assert!(
        !synthetic_line.contains("external/legacy4.jsp"),
        "{synthetic_line}"
    );
    assert!(
        !synthetic_line.contains("external/legacy5.jsp"),
        "{synthetic_line}"
    );
    assert!(synthetic_line.contains("+2 more"), "{synthetic_line}");
    // The warning must not under-disclose what a synthetic entry actually DOES: its io still merges
    // and joins under the (possibly typo'd) declared path, and it counts in coverage.files — a config
    // author checking only the file count would otherwise never learn a phantom file is inflating it.
    assert!(
        synthetic_line.contains(
            "their io still merges and joins under the declared path (check for path typos)"
        ),
        "{synthetic_line}"
    );
    assert!(
        synthetic_line.contains("they count in coverage.files"),
        "{synthetic_line}"
    );
}

#[test]
fn is_entry_only_overlay_counts_as_fact_carrying_no_zero_fact_warning() {
    // `is_entry: true` with every other channel at its empty default — the simplest honest way to
    // construct a fact-carrying-but-otherwise-empty projection (per the struct: io/symbols/imports/
    // fragments/attributes are all easy to leave empty; `is_entry` is a plain bool flip).
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/hooks.server.ts", "export const noop = 1;\n");

    let mut hooks = projection("src/hooks.server.ts", 1);
    hooks.is_entry = true;

    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("sveltekit-entry-adapter/1", vec![hooks])];
    let out = analyze_tree(dir.path(), &cfg);

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("carries any extracted facts")),
        "an is_entry-only projection must count as fact-carrying: {:?}",
        out.warnings
    );
}

#[test]
fn overlay_warnings_are_byte_for_byte_identical_across_two_runs() {
    // A config combining every G3/G8 warning shape at once (mismatched source WITH join io — the G3
    // gate requires io, the only join-relevant channel — plus a synthetic entry and a zero-fact
    // entry) — proves their combined order is stable, not just each warning individually.
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");
    dir.write("a.sql", "SELECT 1;\n");

    let mut mismatched = projection("external/mismatched.jsp", 1);
    mismatched.io.provides.push(IoProvide {
        body: None,
        kind: "db-table".to_string(),
        key: "table:widgets".to_string(),
        file: "external/mismatched.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    let mut cfg = EngineConfig {
        source_id: "spring".to_string(),
        ..EngineConfig::default()
    };
    cfg.adapter_overlays = vec![
        NormalizedEnvelope {
            format: NORMALIZED_AST_FORMAT.to_string(),
            version: 1,
            parser: "dbx-adapter/1".to_string(),
            source: "dbx".to_string(),
            files: vec![mismatched],
        },
        overlay("sql-adapter/1", vec![projection("a.sql", 1)]),
    ];

    let first = analyze_tree(dir.path(), &cfg).warnings;
    let second = analyze_tree(dir.path(), &cfg).warnings;
    assert_eq!(first, second);
    // Sanity: this really did exercise every new warning shape, not an accidentally-empty run.
    assert!(first
        .iter()
        .any(|w| w.contains("declares a different source")));
    assert!(first
        .iter()
        .any(|w| w.contains("added as synthetic entries")));
    assert!(first
        .iter()
        .any(|w| w.contains("carries any extracted facts")));
}

#[test]
fn symbols_only_overlay_is_zero_fact_and_does_not_count_as_coverage() {
    // Review pin (blocking finding): Mode B's merge never consumes overlay `symbols`
    // (`merge_projection_onto_artifact` drops the field; the synthetic branch empties it), so a
    // symbols-only overlay must trip the zero-fact census AND keep the per-extension "no native
    // parser" disclosure alive for its file — counting symbols as coverage would mask the disclosure
    // for data the engine silently drops.
    // `.rb` stands in for "a real source extension with no native parser frontend" — `.sql` used to fill
    // this role, but `zzop-parser-sql` now gives `.sql` a real `Language::Sql` dispatch (`db-table`
    // provides from `CREATE TABLE`), so it no longer belongs in this fixture.
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");
    dir.write("a.rb", "def handler\n  1\nend\n");

    let mut proj = projection("a.rb", 1);
    proj.symbols.push(SourceSymbol {
        id: "a.rb#getReport".to_string(),
        file: "a.rb".to_string(),
        name: "getReport".to_string(),
        kind: SourceSymbolKind::Function,
        line: 1,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("rb-adapter/1", vec![proj])];

    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("carries any extracted facts")),
        "symbols-only overlay must be reported zero-fact: {:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("no native parser") && w.contains("a.rb")),
        "symbols-only coverage must not suppress the unparsed-extension disclosure: {:?}",
        out.warnings
    );
}

#[test]
fn io_less_overlay_with_mismatched_source_does_not_warn_source_mismatch() {
    // Review pin: io is the only join-relevant channel, so a mismatched `source` on an overlay
    // carrying no io (attributes-/is_entry-only adapters, e.g. the auth-overlay example) is inert —
    // the "joins read as intra-source" warning would be false noise about joins that cannot happen.
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");

    let mut proj = projection("src/app.ts", 1);
    proj.is_entry = true; // a real, consumed fact — just not io
    let mut cfg = EngineConfig {
        source_id: "backend".to_string(),
        ..EngineConfig::default()
    };
    cfg.adapter_overlays = vec![NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "auth-overlay/1".to_string(),
        source: "web".to_string(),
        files: vec![proj],
    }];

    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("declares a different source")),
        "io-less overlay must not trip the source-mismatch warning: {:?}",
        out.warnings
    );
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("carries any extracted facts")),
        "is_entry is a consumed fact — not zero-fact: {:?}",
        out.warnings
    );
}

#[test]
fn reserved_io_only_overlay_is_zero_fact_not_coverage() {
    // Review pin: the fact-carrying predicate judges io NET of reserved sentinel kinds (the same set
    // `drop_reserved_io` strips), so a projection whose only io is a reserved sentinel is zero-fact at
    // BOTH call sites (census and the unparsed-extension exclusion set) — raw vs cleaned input cannot
    // judge differently, and a reserved-only overlay cannot mask the disclosure.
    // `.rb` stands in for "a real source extension with no native parser frontend" — see the identical
    // note on `symbols_only_overlay_is_zero_fact_and_does_not_count_as_coverage` above (`.sql` graduated
    // out of this fixture once `zzop-parser-sql` gave it a real dispatch).
    let dir = TempDir::new("zzop-adapter-overlay");
    dir.write("src/app.ts", "export function noop() { return 1; }\n");
    dir.write("a.rb", "def handler\n  1\nend\n");

    let mut proj = projection("a.rb", 1);
    proj.io.provides.push(IoProvide {
        body: None,
        kind: "nest-global-prefix".to_string(),
        key: "api".to_string(),
        file: "a.rb".to_string(),
        line: 1,
        symbol: None,
    });
    let mut cfg = config();
    cfg.adapter_overlays = vec![overlay("rb-adapter/1", vec![proj])];

    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("carries any extracted facts")),
        "reserved-io-only overlay must be reported zero-fact: {:?}",
        out.warnings
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("no native parser") && w.contains("a.rb")),
        "reserved-io-only coverage must not suppress the unparsed-extension disclosure: {:?}",
        out.warnings
    );
}
