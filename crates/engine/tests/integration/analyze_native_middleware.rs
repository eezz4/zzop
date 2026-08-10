//! End-to-end coverage for NATIVE Express middleware-guard recognition feeding the generic
//! entity-attribute channel (`zzop_core::{Attribute, EntityRef, AttributeStore}`), through the real
//! `zzop_parser_typescript::adapters::router_mounts` recognizer and `analyze::compose`'s
//! `compose_router_mount_provides` — as opposed to `analyze_attribute_injection.rs`, which exercises the
//! same channel via a Mode-B `EngineConfig::adapter_overlays` injection. This file drives `analyze_tree`
//! directly over real TypeScript/Express source, with NO overlay at all, proving:
//!
//! - `app.use('/admin', requireAuth())` (router-level, prefix-scoped guard) exempts every mutating route
//!   under `/admin` from `mutating-route-no-auth`, while a sibling unguarded mutating route elsewhere
//!   still fires — no over-suppression.
//! - A sub-router's own `.use(verifyToken())` guard, mounted under a prefix from another file, composes
//!   into the correct PathScope through the SAME cross-file mount-chain composition
//!   `compose_router_mount_provides` already performs for provides.
//! - `app.post('/x', requireAuth, handler)` (route-level middle-argument guard) exempts that one route.
//! - `app.use(session())` is deliberately NOT judged a guard (`session` is excluded from the producer's
//!   guard-name vocabulary — express-session adds session STATE, it does not reject requests), so the
//!   route stays flagged — a vocabulary-precision pin.
//!
//! This file also PINS the `"auth-guarded"` key pairing between the parser's private
//! `AUTH_GUARDED_ATTR_KEY` const (`router_mounts.rs`) and `zzop_rules_http::mutating_route_no_auth`'s
//! public `AUTH_GUARDED_ATTR`: if the two ever drifted, every suppression assertion below would fail
//! (the rule would stop seeing the attribute the parser emits) — behaviorally pinned, since the
//! parser-side const is intentionally private (producer-owned vocabulary, per this module's own doc).
//!
//! Self-contained: the `TempDir` helper is copied/adapted from `analyze_attribute_injection.rs` /
//! `analyze_io_natives.rs` rather than shared, since each `tests/*.rs` file is its own separate test
//! binary/crate.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as
/// `analyze_attribute_injection.rs`/`analyze_io_natives.rs`).
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
        source_id: "native-middleware-fixture".to_string(),
        ..EngineConfig::default()
    }
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings.iter().filter(|f| f.rule_id == rule).collect()
}

fn mutating_paths(out: &AnalyzeOutput) -> Vec<String> {
    hits(out, "mutating-route-no-auth")
        .iter()
        .map(|f| {
            f.data.as_ref().unwrap()["path"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn router_level_use_guard_exempts_routes_under_its_prefix_but_not_siblings() {
    let dir = TempDir::new("zzop-native-mw-router-level");
    dir.write(
        "routes/api.ts",
        concat!(
            "const app = express();\n",
            "app.use('/admin', requireAuth());\n",
            "app.post('/admin/widgets', createWidget);\n",
            "app.post('/public/items', createItem);\n"
        ),
    );
    dir.write(
        "routes/handlers.ts",
        concat!(
            "export function createWidget(c) {\n",
            "  return prisma.widget.create({ data: {} });\n",
            "}\n",
            "export function createItem(c) {\n",
            "  return prisma.item.create({ data: {} });\n",
            "}\n"
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    let paths = mutating_paths(&out);
    assert!(
        !paths.contains(&"/admin/widgets".to_string()),
        "the /admin-scoped guard must exempt /admin/widgets: {:?}",
        out.findings
    );
    assert!(
        paths.contains(&"/public/items".to_string()),
        "a sibling route outside /admin must still fire — no over-suppression: {:?}",
        out.findings
    );
}

#[test]
fn sub_router_use_guard_composes_through_a_cross_file_mount_into_the_right_pathscope() {
    let dir = TempDir::new("zzop-native-mw-sub-router");
    dir.write(
        "routes/router.ts",
        concat!(
            "import { Router } from 'express';\n",
            "const r = Router();\n",
            "r.use(verifyToken());\n",
            "r.post('/thing', createThing);\n",
            "export default r;\n"
        ),
    );
    dir.write(
        "routes/app.ts",
        concat!(
            "import express from 'express';\n",
            "import router from './router';\n",
            "const app = express();\n",
            "app.use('/api', router);\n"
        ),
    );
    dir.write(
        "routes/handlers.ts",
        concat!(
            "export function createThing(c) {\n",
            "  return prisma.thing.create({ data: {} });\n",
            "}\n"
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    let io = out.ir.ir.io.clone().expect("expected io facts");
    assert!(
        io.provides
            .iter()
            .any(|p| p.kind == "http" && p.key == "POST /api/thing"),
        "the mount chain must compose to POST /api/thing: {:?}",
        io.provides
    );
    assert!(
        mutating_paths(&out).is_empty(),
        "the sub-router's own .use(verifyToken()) guard, composed through the /api mount, must exempt \
         /api/thing: {:?}",
        out.findings
    );
}

#[test]
fn route_level_middle_arg_guard_exempts_that_one_route() {
    let dir = TempDir::new("zzop-native-mw-route-level");
    dir.write(
        "routes/api.ts",
        concat!(
            "const app = express();\n",
            "app.post('/x', requireAuth, handlerX);\n",
            "app.post('/y', handlerY);\n"
        ),
    );
    dir.write(
        "routes/handlers.ts",
        concat!(
            "export function handlerX(c) {\n",
            "  return prisma.x.create({ data: {} });\n",
            "}\n",
            "export function handlerY(c) {\n",
            "  return prisma.y.create({ data: {} });\n",
            "}\n"
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    let paths = mutating_paths(&out);
    assert!(
        !paths.contains(&"/x".to_string()),
        "the route-level requireAuth middle argument must exempt /x: {:?}",
        out.findings
    );
    assert!(
        paths.contains(&"/y".to_string()),
        "a 2-arg sibling route with no guard argument must still fire: {:?}",
        out.findings
    );
}

#[test]
fn session_use_is_never_judged_a_guard_vocabulary_precision_pin() {
    let dir = TempDir::new("zzop-native-mw-session");
    dir.write(
        "routes/api.ts",
        concat!(
            "const app = express();\n",
            "app.use(session());\n",
            "app.post('/orders', createOrder);\n"
        ),
    );
    dir.write(
        "routes/handlers.ts",
        concat!(
            "export function createOrder(c) {\n",
            "  return prisma.order.create({ data: {} });\n",
            "}\n"
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    assert!(
        mutating_paths(&out).contains(&"/orders".to_string()),
        "session() must never be judged a guard — /orders must still fire: {:?}",
        out.findings
    );
}
