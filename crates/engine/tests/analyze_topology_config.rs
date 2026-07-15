//! End-to-end coverage for the "connection topology config" feature: `EngineConfig::mounts`
//! (provide-side deployment-gateway mount apply, `analyze::compose::apply_config_mounts`) and
//! `EngineConfig::hosts` (consume-side internal-host re-keying at cross-layer link time,
//! `zzop_core::LinkOptions::internal_hosts`), driven end to end through `zzop_engine::analyze_trees` —
//! same fixture-tree style as `analyze_cross_layer_findings.rs` / `analyze_multi_tree_nestjs.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_trees, EngineConfig, MountRule};

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

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

fn mount(dir: &str, at: &str) -> MountRule {
    MountRule {
        dir: dir.to_string(),
        at: at.to_string(),
    }
}

// --- 1. Whole-tree mount ---

fn users_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-topo-users-be");
    dir.write(
        "src/users.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('users')\n",
            "export class UsersController {\n",
            "  @Get()\n",
            "  findAll() {\n    return [];\n  }\n",
            "}\n",
        ),
    );
    dir
}

fn users_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-topo-users-fe");
    dir.write(
        "src/api.ts",
        "export function loadUsers() { return fetch('/api/users'); }\n",
    );
    dir
}

#[test]
fn whole_tree_mount_prepends_at_to_every_http_provide() {
    let be = users_be_tree();
    let fe = users_fe_tree();

    let mut be_config = config("be");
    be_config.mounts = vec![mount("", "/api")];

    let trees = vec![
        (be.path().to_path_buf(), be_config),
        (fe.path().to_path_buf(), config("fe")),
    ];
    let out = analyze_trees(&trees);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected the whole-tree mount to produce exactly one joining edge: {:?}",
        out.cross_layer.edges
    );
    assert_eq!(http_edges[0].key, "GET /api/users");
}

// --- 2. Fragmented monorepo: longest-dir-wins mount precedence ---

fn fragmented_monorepo_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-topo-fragmented");
    // Under apps/settle -> mounted at /settle.
    dir.write(
        "apps/settle/settle.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('billing')\n",
            "export class SettleController {\n",
            "  @Get()\n",
            "  bill() {\n    return [];\n  }\n",
            "}\n",
        ),
    );
    // Under apps/settle/inner -> mounted at /deep (the LONGER, more specific dir wins over apps/settle).
    dir.write(
        "apps/settle/inner/inner.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('detail')\n",
            "export class InnerController {\n",
            "  @Get()\n",
            "  detail() {\n    return [];\n  }\n",
            "}\n",
        ),
    );
    // Elsewhere -> no mount matches (dirs above don't cover it), key stays untouched.
    dir.write(
        "apps/other/other.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('misc')\n",
            "export class OtherController {\n",
            "  @Get()\n",
            "  misc() {\n    return [];\n  }\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn fragmented_monorepo_mounts_pick_the_longest_matching_dir() {
    let dir = fragmented_monorepo_tree();
    let mut cfg = config("mono");
    cfg.mounts = vec![
        mount("apps/settle", "/settle"),
        mount("apps/settle/inner", "/deep"),
    ];
    let trees = vec![(dir.path().to_path_buf(), cfg)];
    let out = analyze_trees(&trees);

    let (_, _, output) = &out.trees[0];
    let provides = &output
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected io facts")
        .provides;

    let billing = provides
        .iter()
        .find(|p| p.file == "apps/settle/settle.controller.ts")
        .unwrap_or_else(|| {
            panic!("expected a provide from settle.controller.ts, got: {provides:?}")
        });
    assert_eq!(billing.key, "GET /settle/billing");

    let detail = provides
        .iter()
        .find(|p| p.file == "apps/settle/inner/inner.controller.ts")
        .unwrap_or_else(|| {
            panic!("expected a provide from inner.controller.ts, got: {provides:?}")
        });
    assert_eq!(
        detail.key, "GET /deep/detail",
        "the more specific (longer) dir must win over the shallower apps/settle mount"
    );

    let other = provides
        .iter()
        .find(|p| p.file == "apps/other/other.controller.ts")
        .unwrap_or_else(|| {
            panic!("expected a provide from other.controller.ts, got: {provides:?}")
        });
    assert_eq!(
        other.key, "GET /misc",
        "a file outside every declared dir must be untouched by any mount"
    );
}

// --- 3. Stacking: config mount stacks on top of a code-extracted Nest global prefix ---

fn nest_global_prefix_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-topo-stacking");
    dir.write(
        "src/users.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('users')\n",
            "export class UsersController {\n",
            "  @Get()\n",
            "  findAll() {\n    return [];\n  }\n",
            "}\n",
        ),
    );
    dir.write("src/main.ts", "app.setGlobalPrefix('api');\n");
    dir
}

#[test]
fn config_mount_stacks_on_top_of_a_code_extracted_global_prefix() {
    let dir = nest_global_prefix_tree();
    let mut cfg = config("be");
    cfg.mounts = vec![mount("", "/gw")];
    let trees = vec![(dir.path().to_path_buf(), cfg)];
    let out = analyze_trees(&trees);

    let (_, _, output) = &out.trees[0];
    let provides = &output
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected io facts")
        .provides;
    let route = provides
        .iter()
        .find(|p| p.kind == "http")
        .unwrap_or_else(|| panic!("expected an http provide, got: {provides:?}"));
    assert_eq!(
        route.key, "GET /gw/api/users",
        "the config mount must stack ON TOP of the already-Nest-prefixed route"
    );
}

// --- 4. Hosts: absolute-URL consume to a declared host joins internal; undeclared host stays external ---

fn hosts_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-topo-hosts-be");
    dir.write(
        "src/users.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('users')\n",
            "export class UsersController {\n",
            "  @Get()\n",
            "  findAll() {\n    return [];\n  }\n",
            "}\n",
        ),
    );
    dir
}

fn hosts_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-topo-hosts-fe");
    dir.write(
        "src/api.ts",
        "export function loadUsers() { return fetch('https://api.foo.com/users'); }\n\
         export function callOther() { return fetch('https://other.com/x'); }\n",
    );
    dir
}

#[test]
fn declared_host_rekeys_the_absolute_url_consume_into_an_internal_edge() {
    let be = hosts_be_tree();
    let fe = hosts_fe_tree();

    let mut be_config = config("be");
    be_config.hosts = vec!["api.foo.com".to_string()];

    let trees = vec![
        (be.path().to_path_buf(), be_config),
        (fe.path().to_path_buf(), config("fe")),
    ];
    let out = analyze_trees(&trees);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected the declared-host consume to join internally: {:?}",
        out.cross_layer.edges
    );
    assert_eq!(http_edges[0].key, "GET /users");

    // The undeclared host stays in the external bucket.
    assert!(
        out.cross_layer
            .external_consumes
            .iter()
            .any(|c| c.consume.key.as_deref() == Some("GET https://other.com/x")),
        "expected the undeclared host to stay external: {:?}",
        out.cross_layer.external_consumes
    );
    assert!(
        out.cross_layer
            .external_consumes
            .iter()
            .all(|c| c.consume.key.as_deref() != Some("GET https://api.foo.com/users")),
        "the declared-host consume must not also appear in external_consumes: {:?}",
        out.cross_layer.external_consumes
    );

    assert_eq!(
        out.cross_layer.host_rekey_counts,
        vec![("api.foo.com".to_string(), 1)]
    );
}

// --- 5. Tripwires: a mount matching nothing, and a host matching nothing ---

#[test]
fn a_mount_matching_no_provide_produces_a_warning() {
    let dir = TempDir::new("zzop-engine-topo-mount-tripwire");
    // No http provides at all in this tree.
    dir.write("src/util.ts", "export function noop() {}\n");
    let mut cfg = config("be");
    cfg.mounts = vec![mount("", "/api")];
    let trees = vec![(dir.path().to_path_buf(), cfg)];
    let out = analyze_trees(&trees);

    let (_, _, output) = &out.trees[0];
    let warning = output
        .warnings
        .iter()
        .find(|w| w.contains("topology mount") && w.contains("had no effect"))
        .unwrap_or_else(|| {
            panic!(
                "expected a zero-effect mount tripwire warning, got: {:?}",
                output.warnings
            )
        });
    assert!(
        warning.contains("stale mount, wrong dir, or the tree emits no http provides"),
        "expected the stale/wrong-dir/no-http-provides wording (0 provides matched this dir's path at all), got: {warning:?}"
    );
    assert!(
        !warning.contains("redundant"),
        "this entry matched nothing by path — it must not use the shadowed-entry wording, got: {warning:?}"
    );
}

/// Shadow case (`F1`, release-audit v0.14.0): a mount whose `dir` DOES match provides can still record 0
/// hits when every one of those matches was won by a more specific (longer-`dir`, or equal-`dir` earlier)
/// entry — reachable via documented config, e.g. `mountedAt` (folds in as an implicit `dir: ""` entry
/// LAST) behind an explicit `mounts` entry that already covers every file. Here we reproduce that shape
/// directly via two equal-empty-`dir` entries: the tie-break rule (first entry wins ties) means the first
/// entry claims every http provide and the second can never win, even though its `dir` matches everything.
/// This must NOT reuse the stale/wrong-dir wording (both are false here: the dir matched plenty, and it is
/// not stale) — it gets the dedicated "shadowed" wording instead.
#[test]
fn a_mount_shadowed_by_an_earlier_equal_dir_mount_gets_the_shadow_worded_warning() {
    let be = users_be_tree();
    let mut cfg = config("be");
    // Two entries with the identical (empty) `dir` -- mirrors an explicit `{dir:"", at:...}` mount
    // followed by `mountedAt`'s implicit `dir:""` entry, which always folds in LAST. The first entry wins
    // every tie, so the second is shadowed on every provide despite its `dir` matching all of them.
    cfg.mounts = vec![mount("", "/explicit"), mount("", "/shadowed")];
    let trees = vec![(be.path().to_path_buf(), cfg)];
    let out = analyze_trees(&trees);

    let (_, _, output) = &out.trees[0];

    // The winning entry rewrote the provide; the shadowed entry produced no edge under its own prefix.
    let provides = &output
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected io facts")
        .provides;
    assert!(
        provides.iter().any(|p| p.key == "GET /explicit/users"),
        "expected the first (winning) entry's prefix to apply, got: {provides:?}"
    );
    assert!(
        provides
            .iter()
            .all(|p| !p.key.starts_with("/shadowed") && !p.key.contains("GET /shadowed")),
        "the shadowed entry must never win a rewrite, got: {provides:?}"
    );

    // `apply_config_mounts` trims surrounding "/" off `at` before using it in the warning text.
    let warning = output
        .warnings
        .iter()
        .find(|w| w.contains("topology mount \"shadowed\""))
        .unwrap_or_else(|| {
            panic!(
                "expected a zero-effect warning for the shadowed entry, got: {:?}",
                output.warnings
            )
        });
    assert!(
        warning.contains("had no effect") && warning.contains("redundant"),
        "expected the shadowed-entry wording (claimed by a more specific mount), got: {warning:?}"
    );
    assert!(
        !warning.contains("stale mount, wrong dir, or the tree emits no http provides"),
        "the shadowed entry's dir DID match provides -- the stale/wrong-dir wording would be false here, got: {warning:?}"
    );

    // The winning entry itself must NOT get a zero-effect warning.
    assert!(
        output
            .warnings
            .iter()
            .all(|w| !w.contains("topology mount \"explicit\"")),
        "the winning entry rewrote a provide and must not be flagged zero-effect, got: {:?}",
        output.warnings
    );
}

#[test]
fn a_host_matching_no_consume_produces_a_warning() {
    let be = hosts_be_tree();
    let mut be_config = config("be");
    // Declared but nothing in this single-tree fixture calls it via an absolute URL.
    be_config.hosts = vec!["never-called.example.com".to_string()];
    let trees = vec![(be.path().to_path_buf(), be_config)];
    let out = analyze_trees(&trees);

    let (_, _, output) = &out.trees[0];
    assert!(
        output
            .warnings
            .iter()
            .any(|w| w.contains("topology host") && w.contains("had no effect")),
        "expected a zero-effect host tripwire warning, got: {:?}",
        output.warnings
    );
    assert_eq!(
        out.cross_layer.host_rekey_counts,
        vec![("never-called.example.com".to_string(), 0)]
    );
}
