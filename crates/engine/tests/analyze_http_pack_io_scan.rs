//! End-to-end coverage for the SHIPPED `rules/dsl/http/http.json` pack's A2-migrated `auth-gates` /
//! `route-exposure` rules (io-scan, framework-neutral — the IoScan projection redesign's follow-up pack
//! migration), loaded off the real file (not an inline synthetic pack — that's `analyze_io_scan_tree.rs`'s
//! job) via `zzop_core::load_dsl_packs`. Proves:
//!
//! - THE FRAMEWORK-NEUTRAL WIN: an unguarded Express `/admin` route now fires `auth-gates` — silent
//!   before this migration, since the old rule was hard-gated on the `apiRoutes\.` house convention and
//!   Express never satisfies it.
//! - A native Express middleware guard (`app.use('/admin', requireAuth())`) clears it via the
//!   `auth-guarded` `PathScope` attribute `compose_router_mount_provides` mints unconditionally.
//! - A NestJS `@UseGuards` decorator-guarded route clears it EVEN WITH the native `mutating-route-no-auth`
//!   rule disabled in config — proving the A2 decoupling (`callgraph::packs_read_io_scan_attrs`): the
//!   shipped pack's `attr_absent: "auth-guarded"` gate alone is enough to keep the evidence flowing.
//! - `// auth-gate-ok` on the route's own registration line suppresses it.
//! - A route registered in a test file (`${test-paths-stories}`) never fires — `file_exclude_pattern`.
//! - An unguarded `/debug` route fires `route-exposure`.
//! - A registration line also containing `isProduction` does not fire `route-exposure` —
//!   `anchor_exclude_pattern`'s env-gate=WHERE-axis carve-out.
//!
//! Self-contained `TempDir`/`config` helpers, same pattern as `analyze_io_scan_tree.rs`/
//! `analyze_native_middleware.rs` (each `tests/*.rs` file is its own separate test binary/crate).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RuleConfig, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

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

/// Loads the real, committed `rules/dsl/http/http.json` — never a hand-copied inline fixture, so this
/// file cannot drift from what actually ships.
fn http_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "rules/dsl failed to load: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "http")
        .expect("rules/dsl/http/http.json must load a pack with id \"http\"")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "http-pack-io-scan-fixture".to_string(),
        packs: vec![http_pack()],
        ..EngineConfig::default()
    }
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("http/{rule}"))
        .collect()
}

fn snippet(f: &Finding) -> &str {
    f.data.as_ref().unwrap()["snippet"].as_str().unwrap()
}

// ---------------------------------------------------------------------------------------------
// THE FRAMEWORK-NEUTRAL WIN — an unguarded Express /admin route, silent before this migration.
// ---------------------------------------------------------------------------------------------

#[test]
fn unguarded_express_admin_route_fires_auth_gates() {
    let dir = TempDir::new("zzop-http-pack-unguarded-admin");
    dir.write(
        "routes/api.ts",
        "const app = express();\napp.post('/admin/widgets', createWidget);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function createWidget(c) {\n  return prisma.widget.create({ data: {} });\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());
    let found = hits(&out, "auth-gates");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(snippet(found[0]), "POST /admin/widgets");
}

// ---------------------------------------------------------------------------------------------
// Native Express middleware guard clears it.
// ---------------------------------------------------------------------------------------------

#[test]
fn native_express_middleware_guard_clears_auth_gates() {
    let dir = TempDir::new("zzop-http-pack-mw-guarded");
    dir.write(
        "routes/api.ts",
        concat!(
            "const app = express();\n",
            "app.use('/admin', requireAuth());\n",
            "app.post('/admin/widgets', createWidget);\n"
        ),
    );
    dir.write(
        "routes/handlers.ts",
        "export function createWidget(c) {\n  return prisma.widget.create({ data: {} });\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());
    assert!(
        hits(&out, "auth-gates").is_empty(),
        "the /admin-scoped native middleware guard must clear attr_absent: {:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// NestJS @UseGuards decorator-guarded route clears it EVEN WITH mutating-route-no-auth disabled —
// proves the A2 decoupling (`callgraph::packs_read_io_scan_attrs`).
// ---------------------------------------------------------------------------------------------

#[test]
fn decorator_guarded_route_clears_auth_gates_even_with_the_native_rule_disabled() {
    let dir = TempDir::new("zzop-http-pack-decorator-guarded");
    dir.write(
        "admin-items.controller.ts",
        concat!(
            "import { Controller, Post, UseGuards } from '@nestjs/common';\n\n",
            "@Controller('admin/items')\n",
            "@UseGuards(JwtAuthGuard)\n",
            "export class AdminItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );

    let mut cfg = config();
    cfg.rule_config = RuleConfig {
        disabled_rules: vec!["mutating-route-no-auth".to_string()],
        ..RuleConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        hits(&out, "auth-gates").is_empty(),
        "decorator-guard evidence must still be minted for the shipped pack's attr_absent gate even \
         when the native mutating-route-no-auth rule is disabled: {:?}",
        out.findings
    );

    // Control: an UNGUARDED sibling under the same disabled-native-rule config still fires — proves the
    // decoupling didn't silently break the rule for everyone.
    let sibling = TempDir::new("zzop-http-pack-decorator-guarded-sibling");
    sibling.write(
        "admin-orders.controller.ts",
        concat!(
            "import { Controller, Post } from '@nestjs/common';\n\n",
            "@Controller('admin/orders')\n",
            "export class AdminOrdersController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = analyze_tree(sibling.path(), &cfg);
    let found = hits(&out, "auth-gates");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(snippet(found[0]), "POST /admin/orders/x");
}

// ---------------------------------------------------------------------------------------------
// `// auth-gate-ok` on the registration line suppresses.
// ---------------------------------------------------------------------------------------------

#[test]
fn auth_gate_ok_marker_on_the_registration_line_suppresses() {
    let dir = TempDir::new("zzop-http-pack-marker");
    dir.write(
        "routes/api.ts",
        "const app = express();\napp.post('/admin/widgets', createWidget); // auth-gate-ok\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function createWidget(c) {\n  return prisma.widget.create({ data: {} });\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());
    assert!(
        hits(&out, "auth-gates").is_empty(),
        "the // auth-gate-ok comment on the registration line must suppress: {:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// A route registered in a test file never fires — `file_exclude_pattern` (${test-paths-stories}).
// ---------------------------------------------------------------------------------------------

#[test]
fn a_route_registered_in_a_test_file_does_not_fire() {
    let dir = TempDir::new("zzop-http-pack-test-file");
    dir.write(
        "routes/admin.test.ts",
        "const app = express();\napp.post('/admin/widgets', createWidget);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function createWidget(c) {\n  return prisma.widget.create({ data: {} });\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());
    assert!(
        hits(&out, "auth-gates").is_empty(),
        "a route registered in a *.test.ts file must be excluded by file_exclude_pattern: {:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// An unguarded /debug route fires `route-exposure`.
// ---------------------------------------------------------------------------------------------

#[test]
fn unguarded_debug_route_fires_route_exposure() {
    let dir = TempDir::new("zzop-http-pack-debug-route");
    dir.write(
        "routes/api.ts",
        "const app = express();\napp.post('/debug/tools', showTools);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function showTools(c) {\n  return true;\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());
    let found = hits(&out, "route-exposure");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(snippet(found[0]), "POST /debug/tools");
}

// ---------------------------------------------------------------------------------------------
// A registration line also naming `isProduction` does not fire `route-exposure` —
// `anchor_exclude_pattern`'s env-gate=WHERE-axis carve-out.
// ---------------------------------------------------------------------------------------------

#[test]
fn is_production_on_the_registration_line_clears_route_exposure() {
    let dir = TempDir::new("zzop-http-pack-is-production-carveout");
    dir.write(
        "routes/api.ts",
        "const app = express();\napp.post('/debug/tools', isProduction, showTools);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function showTools(c) {\n  return true;\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "an isProduction check on the same registration line must clear route-exposure via \
         anchor_exclude_pattern: {:?}",
        out.findings
    );
}
