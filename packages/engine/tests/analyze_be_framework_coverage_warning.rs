//! End-to-end coverage for `zzop_engine::framework_silence`'s three coverage self-reports, wired into
//! `analyze_tree`'s `AnalyzeOutput::warnings`:
//! - S1 (`controller_silence_warning`): a tree whose files carry controller-decorator-shaped lines in 3+
//!   distinct files, yet extract ZERO `http` provides tree-wide. Guards against an unrecognized
//!   controller-decorator convention (e.g. a framework using `@RestController` under a different package
//!   name, the way n8n's own `@n8n/decorators` does) failing silently.
//! - S2 (`server_framework_import_warning`): a server-framework package (koa, fastify, ...) is imported
//!   while extracted `http` provides stay near-zero — closes the METHOD-CALL registration idiom gap S1's
//!   decorator regex cannot see (dogfood round 9's be-express class; Koa is used here rather than Express
//!   because Express itself now has a dedicated router-mount extractor post-round-9-fix, per
//!   `parser-typescript`'s `router_mounts` module doc).
//! - S3 (`committed_spec_io_silence_warning`): a committed OpenAPI/Swagger spec sits in the tree while
//!   this tree's io stays near-zero in both directions — the generated-client blind spot (round 9's
//!   fe-vue class).
//!
//! Each covers the NEXT unknown framework/idiom at least getting a warning instead of silent
//! cross-layer-join darkness.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

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
        source_id: "coverage-warning-fixture".to_string(),
        ..EngineConfig::default()
    }
}

const WARNING_SUBSTRING: &str = "route decorators/annotations but no http routes were extracted";
const S2_WARNING_SUBSTRING: &str = "server-framework package(s) imported but only";
const S3_WARNING_SUBSTRING: &str = "committed OpenAPI/Swagger spec exists at";

/// 3 files carrying an invented `@FastController`/`@FastGet` decorator shape — structurally identical
/// (class-level gate + method-level verb) to Nest's own idiom, but under decorator NAMES that
/// `zzop_parser_typescript::nest`'s `CONTROLLER_CLASS_GATES` (`["Controller", "RestController"]`, exact-name
/// matched) does not recognize, and that don't match the Spring extractor's regex either. No Nest/Spring/
/// Hono shape appears anywhere in this tree, so this tree's real http-provides count is genuinely zero.
fn unrecognized_framework_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-unrecognized");
    dir.write(
        "src/users.controller.ts",
        "@FastController('/users')\nexport class UsersController {\n  @FastGet('/')\n  findAll() { return []; }\n}\n",
    );
    dir.write(
        "src/orders.controller.ts",
        "@FastController('/orders')\nexport class OrdersController {\n  @FastGet('/')\n  findAll() { return []; }\n}\n",
    );
    dir.write(
        "src/items.controller.ts",
        "@FastController('/items')\nexport class ItemsController {\n  @FastGet('/')\n  findAll() { return []; }\n}\n",
    );
    dir
}

/// A real NestJS-shaped BE tree (`@Controller`/`@Get`) — `zzop_parser_typescript::nest::extract_nest_provides`
/// recognizes this shape and extracts a real `http` provide, so the tree's http-provides count is > 0 and
/// `controller_silence_warning` short-circuits before ever reading a file. Mirrors
/// `analyze_multi_tree_nestjs.rs`'s `nest_be_tree()` helper (adapted to a single-tree `analyze_tree` call
/// rather than the cross-layer `analyze_trees` API that test exercises).
fn real_nest_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-real-nest");
    dir.write(
        "src/users/users.controller.ts",
        concat!(
            "import { Controller, Get, Param } from '@nestjs/common';\n\n",
            "@Controller('api/users')\n",
            "export class UsersController {\n",
            "  @Get(':id')\n",
            "  findOne(@Param('id') id: string) {\n    return id;\n  }\n",
            "}\n",
        ),
    );
    dir
}

/// A pure-FE Angular-style fixture (`@Component`/`@Input`/`@Output`/`@HostListener`) — none of Angular's own
/// decorator vocabulary lexically matches `Controller`/`Mapping`/`Get`/`Post`/`Put`/`Delete`/`Patch`, so the
/// regex never matches at all (verified in `zzop_engine::coverage`'s own unit tests too); this proves the
/// engine-level wiring inherits that same no-false-positive property, not just the bare function in
/// isolation.
fn angular_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-angular");
    dir.write(
        "src/a.component.ts",
        "@Component({ selector: 'app-a' })\nexport class AComponent {\n  @Input() x: string;\n  @Output() y = new EventEmitter();\n  @HostListener('click')\n  onClick() {}\n}\n",
    );
    dir.write(
        "src/b.component.ts",
        "@Component({ selector: 'app-b' })\nexport class BComponent {\n  @Inject(TOKEN) dep: any;\n}\n",
    );
    dir.write(
        "src/c.component.ts",
        "@Component({ selector: 'app-c' })\nexport class CComponent {\n  @Input() z: number;\n}\n",
    );
    dir
}

/// A NestJS-shaped BE tree with 3 recognized routes (clears `MIN_PROVIDES_FLOOR`) — unlike `real_nest_tree`
/// (deliberately just 1 route, itself still below the floor and so a legitimate S2 near-zero disclosure
/// target), this fixture exists solely to prove S2 goes silent once a tree's extracted `http` provides are
/// no longer near-zero, even though `@nestjs/common` is still imported.
fn healthy_nest_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-healthy-nest");
    dir.write(
        "src/users/users.controller.ts",
        concat!(
            "import { Controller, Get, Post, Param } from '@nestjs/common';\n\n",
            "@Controller('api/users')\n",
            "export class UsersController {\n",
            "  @Get(':id')\n",
            "  findOne(@Param('id') id: string) {\n    return id;\n  }\n",
            "  @Get()\n",
            "  findAll() {\n    return [];\n  }\n",
            "  @Post()\n",
            "  create() {\n    return {};\n  }\n",
            "}\n",
        ),
    );
    dir
}

/// Koa, imported and used, but with no route-registration shape any native extractor recognizes (Koa is
/// not in `router_mounts`' Hono/Express vocabulary, nor a decorator framework) — this tree's real
/// `http`-provides count is genuinely zero, and the S2 tripwire should name the `koa` import.
fn koa_import_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-koa");
    dir.write(
        "src/app.ts",
        "import Koa from 'koa';\n\nconst app = new Koa();\napp.use(async (ctx) => {\n  ctx.body = 'ok';\n});\napp.listen(3000);\n",
    );
    dir
}

/// A committed OpenAPI spec with no other io-bearing file in the tree — mirrors dogfood round 9's
/// fe-vue class (a generated client built from `src/services/openapi.yml`, whose call sites the
/// literal-call-site consume extractor cannot see), so io stays near-zero in both directions and the S3
/// tripwire should name the spec file.
fn committed_openapi_spec_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-openapi-spec");
    dir.write(
        "src/services/openapi.yml",
        "openapi: 3.0.0\ninfo:\n  title: Example\npaths:\n  /users:\n    get:\n      summary: list users\n",
    );
    dir.write(
        "src/App.vue",
        "<template>\n  <div>hi</div>\n</template>\n<script setup>\nconst greeting = 'hi';\n</script>\n",
    );
    dir
}

#[test]
fn unrecognized_controller_decorator_shape_with_zero_http_provides_warns() {
    let dir = unrecognized_framework_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings.iter().any(|w| w.contains(WARNING_SUBSTRING)),
        "expected the coverage-gap warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_real_nest_tree_extracts_provides_and_never_warns() {
    let dir = real_nest_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.ir
            .ir
            .io
            .as_ref()
            .is_some_and(|io| io.provides.iter().any(|p| p.kind == "http")),
        "expected at least one real http provide from the Nest fixture, got: {:?}",
        out.ir.ir.io
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains(WARNING_SUBSTRING)),
        "a tree that successfully extracts http provides must never get the coverage-gap warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_pure_fe_angular_tree_never_warns() {
    let dir = angular_fe_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings.iter().any(|w| w.contains(WARNING_SUBSTRING)),
        "Angular's decorator vocabulary must never trigger the coverage-gap warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn two_runs_over_the_unrecognized_framework_tree_produce_identical_warnings() {
    let dir = unrecognized_framework_tree();
    let cfg = config();
    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);
    assert_eq!(out1.warnings, out2.warnings);
}

#[test]
fn koa_import_with_zero_http_provides_fires_the_s2_warning() {
    let dir = koa_import_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(S2_WARNING_SUBSTRING) && w.contains("koa")),
        "expected the S2 server-framework-import warning naming koa, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_real_nest_tree_with_only_one_extracted_route_still_fires_the_s2_warning() {
    // `real_nest_tree` imports `@nestjs/common` (in `SERVER_FRAMEWORK_SPECIFIERS`) and extracts a real
    // http provide via the decorator extractor — but only 1, still below `MIN_PROVIDES_FLOOR`, so S2's
    // near-zero (not exact-zero) gate is correct to still fire: a 1-route BE genuinely gets a
    // gracefully-worded disclosure, per `MIN_PROVIDES_FLOOR`'s own doc.
    let dir = real_nest_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(S2_WARNING_SUBSTRING)),
        "expected the S2 warning on a near-zero (1-route) provides tree, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_nest_tree_with_three_or_more_routes_never_fires_the_s2_warning() {
    let dir = healthy_nest_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings.iter().any(|w| w.contains(S2_WARNING_SUBSTRING)),
        "a tree whose extracted http provides clear MIN_PROVIDES_FLOOR must never get the S2 warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn committed_openapi_spec_with_near_zero_io_fires_the_s3_warning() {
    let dir = committed_openapi_spec_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(S3_WARNING_SUBSTRING) && w.contains("openapi.yml")),
        "expected the S3 committed-spec warning naming openapi.yml, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_pure_fe_angular_tree_never_fires_the_s3_warning() {
    // No committed openapi/swagger spec file anywhere in this tree, so S3 must stay silent regardless of
    // the tree's io levels.
    let dir = angular_fe_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains(S3_WARNING_SUBSTRING)),
        "a tree with no committed spec file must never get the S3 warning, got: {:?}",
        out.warnings
    );
}
