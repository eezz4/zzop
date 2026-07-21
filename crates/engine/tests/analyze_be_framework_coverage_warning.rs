//! End-to-end coverage for `zzop_engine::framework_silence`'s four coverage self-reports, wired into
//! `analyze_tree`'s `AnalyzeOutput::warnings`:
//! - S1 (`controller_silence_warning`): a tree whose files carry controller-decorator-shaped lines in 3+
//!   distinct files, yet extract NEAR-ZERO (<3) `http` provides tree-wide. Guards against an unrecognized
//!   controller-decorator convention (e.g. a framework using `@RestController` under a different package
//!   name, the way n8n's own `@n8n/decorators` does) failing silently — including round 14's shape, where
//!   a Spring-BE tree still had a couple of lexically-extracted provides after losing most of its routes.
//! - S2 (`server_framework_import_warning`): a server-framework package (koa, fastify, ...) is imported
//!   while extracted `http` provides stay near-zero — closes the METHOD-CALL registration idiom gap S1's
//!   decorator regex cannot see (dogfood round 9's be-express class; Koa is used here rather than Express
//!   because Express itself now has a dedicated router-mount extractor post-round-9-fix, per
//!   `parser-typescript`'s `router_mounts` module doc).
//! - S3 (`committed_spec_io_silence_warning`): a committed OpenAPI/Swagger spec sits in the tree while
//!   this tree's io stays near-zero in both directions — the generated-client blind spot (round 9's
//!   fe-vue class).
//! - S4 (`client_library_import_warning`): an http-client package (axios, `@angular/common/http`, ...) is
//!   imported while extracted `http` consumes stay near-zero — the consume-side dual of S2 (round 14's
//!   Angular-FE class: ~15 real `HttpClient` call sites, 0 extracted consumes).
//! - S6 (`orm_schema_silence_warning`): an ORM-schema package (TypeORM, ...) is imported while ZERO
//!   `db-table` io facts were extracted tree-wide — a live NestJS repo full of `@Entity` decorators
//!   produced no db-table facts and no warning at all before this tripwire existed.
//! - S7 (`fetch_wrapper_call_site_warning`): a hand-rolled fetch-wrapper module (`export function
//!   get/post/put/del`, each delegating to one internal `fetch(` call) is imported and its exports called
//!   20+ times across other files, while extracted KEYED `http` consumes stay near-zero — blind-field
//!   test R10's fe-svelte class (`src/lib/api.js`, callers across `src/routes/**`), the wrapper-
//!   indirection shape S5's own tree-wide token count structurally cannot see.
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

const WARNING_SUBSTRING: &str = "route decorators/annotations but only";
const S2_WARNING_SUBSTRING: &str = "server-framework package(s) imported but only";
const S3_WARNING_SUBSTRING: &str = "committed OpenAPI/Swagger spec exists at";
const S4_WARNING_SUBSTRING: &str = "http-client package(s) imported but only";
const S6_WARNING_SUBSTRING: &str = "ORM schema marker(s) detected but zero db-table io facts";
const S7_WARNING_SUBSTRING: &str = "exports a fetch-wrapper idiom";

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

/// Round 14's actual failure shape for S1: 2 real NestJS controllers (each contributing 1 extracted
/// `http` provide, so `http_provides_count` is 2 — near-zero, still below `MIN_PROVIDES_FLOOR`) plus a
/// third file carrying the same unrecognized `@FastController`/`@FastGet` decorator shape
/// `unrecognized_framework_tree` uses, so `MIN_FILES` (3) worth of decorator-matching files is cleared
/// too. An exact-zero gate would have silently passed over this tree (2 > 0); the near-zero gate must not.
fn near_zero_provides_with_three_decorator_files_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-s1-near-zero");
    dir.write(
        "src/users/users.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('api/users')\n",
            "export class UsersController {\n",
            "  @Get()\n",
            "  findAll() { return []; }\n",
            "}\n",
        ),
    );
    dir.write(
        "src/orders/orders.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('api/orders')\n",
            "export class OrdersController {\n",
            "  @Get()\n",
            "  findAll() { return []; }\n",
            "}\n",
        ),
    );
    dir.write(
        "src/items.controller.ts",
        "@FastController('/items')\nexport class ItemsController {\n  @FastGet('/')\n  findAll() { return []; }\n}\n",
    );
    dir
}

/// The healthy counterpart of the fixture above: 3 real NestJS controllers across 3 decorator-matching
/// files, clearing `MIN_PROVIDES_FLOOR` (3 extracted provides) — proves S1's near-zero gate still goes
/// silent once provides clear the floor, even with `MIN_FILES`-worth of decorator-shaped files present.
fn healthy_provides_across_three_decorator_files_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-s1-healthy");
    dir.write(
        "src/users/users.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('api/users')\n",
            "export class UsersController {\n",
            "  @Get()\n",
            "  findAll() { return []; }\n",
            "}\n",
        ),
    );
    dir.write(
        "src/orders/orders.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('api/orders')\n",
            "export class OrdersController {\n",
            "  @Get()\n",
            "  findAll() { return []; }\n",
            "}\n",
        ),
    );
    dir.write(
        "src/items/items.controller.ts",
        concat!(
            "import { Controller, Get } from '@nestjs/common';\n\n",
            "@Controller('api/items')\n",
            "export class ItemsController {\n",
            "  @Get()\n",
            "  findAll() { return []; }\n",
            "}\n",
        ),
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
    // Funnel pin (D9): the S2 disclosure must chain to CREATION like every sibling silence
    // warning — a reword dropping the partial-envelope on-ramp or the contract pointer fails here.
    assert!(
        out.warnings.iter().any(|w| w.contains(S2_WARNING_SUBSTRING)
            && w.contains("zzop-mcp contract envelope-guide")
            && w.contains("partial envelope")),
        "expected the S2 warning to carry the adapter-creation funnel tail, got: {:?}",
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

#[test]
fn near_zero_provides_with_three_decorator_files_still_fires_the_s1_warning() {
    // Round 14's actual failure shape: 2 real extracted provides (not zero) across a tree that also
    // clears MIN_FILES worth of decorator-matching files — the near-zero gate must still fire here, where
    // the old exact-zero gate would have silently passed it over.
    let dir = near_zero_provides_with_three_decorator_files_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(WARNING_SUBSTRING) && w.contains("only 2 http route")),
        "expected the S1 near-zero warning naming 2 extracted routes, got: {:?}",
        out.warnings
    );
}

#[test]
fn three_decorator_files_with_three_extracted_provides_never_fires_the_s1_warning() {
    let dir = healthy_provides_across_three_decorator_files_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings.iter().any(|w| w.contains(WARNING_SUBSTRING)),
        "a tree whose extracted http provides clear MIN_PROVIDES_FLOOR must never get the S1 warning, \
even with 3+ decorator-matching files, got: {:?}",
        out.warnings
    );
}

/// Round 14's Angular-FE consume-side shape: `axios` is imported and genuinely used, but through an
/// `axios.create()` instance stored in a variable and called via that variable (`api.get(...)`) — the
/// egress extractor's http-call matcher requires the call-site's member-expression object to be the bare
/// identifier `axios` (or `ky`) itself (`zzop_parser_typescript::adapters::egress::match_http_call`), so
/// neither the `axios.create(...)` call (not an http verb) nor the `api.get(...)` call (object is `api`,
/// not `axios`) is recognized; this tree's real `http`-consumes count is genuinely zero.
fn axios_wrapped_instance_zero_consumes_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-axios-wrapped");
    dir.write(
        "src/api.ts",
        concat!(
            "import axios from 'axios';\n\n",
            "const api = axios.create({ baseURL: 'https://api.example.com' });\n\n",
            "export async function fetchUsers() {\n",
            "  const res = await api.get('/users');\n",
            "  return res.data;\n",
            "}\n",
        ),
    );
    dir
}

/// Round 14's actual Angular-FE fixture shape: `@angular/common/http`'s `HttpClient` injected and called
/// through `this.http.get(...)`. The same batch that added S4 also taught `extract_http_egress` this exact
/// DI shape (`angular-httpclient-v1`), so this tree now yields ONE extracted consume, not zero — which is
/// precisely S4's near-zero band (1 < MIN_PROVIDES_FLOOR): the e2e exercises the warning co-existing with
/// a partially-sighted extractor, while the axios fixture above stays the fully-blind (0-consume) case.
fn angular_http_client_near_zero_consumes_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-angular-http-client");
    dir.write(
        "src/users.service.ts",
        concat!(
            "import { Injectable } from '@angular/core';\n",
            "import { HttpClient } from '@angular/common/http';\n\n",
            "@Injectable({ providedIn: 'root' })\n",
            "export class UsersService {\n",
            "  constructor(private http: HttpClient) {}\n\n",
            "  getUsers() {\n",
            "    return this.http.get('/api/users');\n",
            "  }\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn axios_import_with_zero_extracted_consumes_fires_the_s4_warning() {
    let dir = axios_wrapped_instance_zero_consumes_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(S4_WARNING_SUBSTRING) && w.contains("axios")),
        "expected the S4 http-client-import warning naming axios, got: {:?}",
        out.warnings
    );
}

/// Test-surface-only http-client import: axios is imported and called, but the SOLE importing file is a
/// `*.spec.ts` under `e2e/`. A test-only import is not deployed egress surface, so it must not feed the
/// S4 census (`stage_package_import_candidate`'s `is_test_file` drain) — otherwise a tree whose only axios
/// use is an e2e harness pinging the server root false-fires the tripwire (dogfood: corpus `be-express`).
fn axios_import_only_in_a_test_file_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-axios-test-only");
    dir.write(
        "e2e/server.spec.ts",
        concat!(
            "import axios from 'axios';\n\n",
            "it('pings the root', async () => {\n",
            "  const res = await axios.get('/');\n",
            "  expect(res.status).toBe(200);\n",
            "});\n",
        ),
    );
    dir
}

#[test]
fn axios_import_only_in_a_test_file_does_not_fire_the_s4_warning() {
    let dir = axios_import_only_in_a_test_file_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains(S4_WARNING_SUBSTRING)),
        "a test-only axios import is not deployed egress surface and must not trip S4, got: {:?}",
        out.warnings
    );
}

#[test]
fn angular_http_client_import_with_near_zero_extracted_consumes_fires_the_s4_warning() {
    let dir = angular_http_client_near_zero_consumes_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(S4_WARNING_SUBSTRING) && w.contains("@angular/common/http")),
        "expected the S4 http-client-import warning naming @angular/common/http, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_real_nest_tree_never_fires_the_s4_warning() {
    // `real_nest_tree` imports no http-client package at all (`@nestjs/common` is a server framework, in
    // `SERVER_FRAMEWORK_SPECIFIERS`, not `HTTP_CLIENT_SPECIFIERS`), so S4 must stay silent regardless of
    // this tree's near-zero consume count.
    let dir = real_nest_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains(S4_WARNING_SUBSTRING)),
        "a tree with no http-client package import must never get the S4 warning, got: {:?}",
        out.warnings
    );
}

/// A NestJS-style repo with TypeORM `@Entity` decorators (the live gap this tripwire was built from) but
/// no Prisma-shaped query call anywhere — this tree's real db-table io fact count is genuinely zero.
fn typeorm_entity_zero_db_facts_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-typeorm");
    dir.write(
        "src/user.entity.ts",
        concat!(
            "import { Entity, Column, PrimaryGeneratedColumn } from 'typeorm';\n\n",
            "@Entity()\n",
            "export class User {\n",
            "  @PrimaryGeneratedColumn()\n",
            "  id: number;\n\n",
            "  @Column()\n",
            "  name: string;\n",
            "}\n",
        ),
    );
    dir
}

/// Same TypeORM `@Entity` marker as above, PLUS a real Prisma-shaped query call site elsewhere in the
/// tree (`getPrisma().user.findMany()`) — extracts one real `db-table` io fact, so S6 must go silent even
/// though the ORM marker is still present (nonzero db-table facts always short-circuit).
fn typeorm_marker_with_prisma_provided_db_facts_tree() -> TempDir {
    let dir = typeorm_entity_zero_db_facts_tree();
    dir.write(
        "src/users.service.ts",
        "export function listUsers() { return getPrisma().user.findMany(); }\n",
    );
    dir
}

#[test]
fn typeorm_marker_with_zero_db_table_facts_fires_the_s6_warning() {
    let dir = typeorm_entity_zero_db_facts_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains(S6_WARNING_SUBSTRING) && w.contains("TypeORM")),
        "expected the S6 ORM-schema-silence warning naming TypeORM, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_tree_with_prisma_provided_db_facts_never_fires_the_s6_warning_even_with_an_orm_marker() {
    let dir = typeorm_marker_with_prisma_provided_db_facts_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.ir
            .ir
            .io
            .as_ref()
            .is_some_and(|io| io.consumes.iter().any(|c| c.kind == "db-table")),
        "expected at least one real db-table consume from the Prisma-shaped call, got: {:?}",
        out.ir.ir.io
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains(S6_WARNING_SUBSTRING)),
        "nonzero db-table facts must short-circuit S6 even with a TypeORM marker present, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_pure_fe_tree_with_no_orm_marker_never_fires_the_s6_warning() {
    // `angular_fe_tree` carries no ORM-schema import at all — S6 must stay silent regardless of its
    // (also zero) db-table fact count.
    let dir = angular_fe_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains(S6_WARNING_SUBSTRING)),
        "a tree with no ORM-schema marker must never get the S6 warning, got: {:?}",
        out.warnings
    );
}

/// blind-field test R10's fe-svelte class, distilled: `src/lib/api.js` exports `get`/`post`/`put`/`del`,
/// each delegating to one internal `fetch(` call, and 5 other files import it (`import * as api from
/// '$lib/api'`, SvelteKit's alias for `src/lib/api.js`) and call its exports across `src/routes/**` —
/// this tree's real extracted keyed `http`-consume count is genuinely near-zero (the literal-call-site
/// consume extractor sees `api.get(...)`, not a recognized http-client shape, at every call site).
fn fetch_wrapper_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-fetch-wrapper");
    dir.write(
        "src/lib/api.js",
        concat!(
            "const base = 'https://api.example.com';\n\n",
            "async function send(method, path) {\n",
            "  return fetch(`${base}/${path}`, { method });\n",
            "}\n\n",
            "export function get(path) {\n  return send('GET', path);\n}\n",
            "export function post(path, data) {\n  return send('POST', path, data);\n}\n",
            "export function put(path, data) {\n  return send('PUT', path, data);\n}\n",
            "export function del(path) {\n  return send('DELETE', path);\n}\n",
        ),
    );
    dir.write(
        "src/routes/a.js",
        "import * as api from '$lib/api';\nexport async function load() {\n  await api.get('a');\n  await api.post('b', {});\n}\n",
    );
    dir.write(
        "src/routes/b.js",
        "import * as api from '$lib/api';\nexport async function load() {\n  await api.put('c', {});\n  await api.del('d');\n}\n",
    );
    dir.write(
        "src/routes/c.js",
        "import * as api from '$lib/api';\nexport async function load() {\n  await api.get('e');\n  await api.get('f');\n}\n",
    );
    dir
}

#[test]
fn fetch_wrapper_module_with_enough_cross_file_call_sites_fires_the_s7_warning() {
    let dir = fetch_wrapper_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings.iter().any(|w| w.contains(S7_WARNING_SUBSTRING)
            && w.contains("src/lib/api.js")
            && w.contains("zzop-mcp contract envelope-guide")
            && w.contains("partial envelope")),
        "expected the S7 fetch-wrapper warning naming src/lib/api.js, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_real_nest_tree_never_fires_the_s7_warning() {
    // No fetch-wrapper module anywhere in this tree — S7 must stay silent regardless of the tree's
    // (also near-zero) keyed http-consume count.
    let dir = real_nest_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains(S7_WARNING_SUBSTRING)),
        "a tree with no fetch-wrapper module must never get the S7 warning, got: {:?}",
        out.warnings
    );
}

// --- S5 per-app census: de-masking + internal-intent filter --------------------------------------

/// A three-app monorepo proving the per-app S5 census both DE-MASKS a dark app (a healthy sibling no
/// longer lifts the whole tree above the keyed-consume floor) AND applies the internal-intent filter
/// (an app that only ever hits ABSOLUTE external services has nothing internal to join, so stays
/// silent):
/// - `apps/healthy` — 5 static-relative `fetch('/api/h<n>')` calls the extractor keys (keyed >= floor,
///   healthy, silent).
/// - `apps/dark` — 5 computed-internal `fetch(`${apiBase}query<n>`)` calls (`apiBase` a function param,
///   so genuinely unresolved => keyed 0; each carries an internal-relative template literal => the
///   census counts it) — this app must WARN, naming `apps/dark`.
/// - `apps/external` — 5 `fetch(CDN)` calls to an absolute-URL const (`https://cdn.example.com/…`); a
///   bare-const arg carries no string literal so the intent filter excludes it (and even if the
///   extractor const-resolves + keys it, the app is then healthy) — silent either way.
fn multi_app_demasking_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-multi-app-demask");
    dir.write("apps/healthy/package.json", "{ \"name\": \"healthy\" }\n");
    let healthy: String = (0..5)
        .map(|n| format!("export const h{n} = () => fetch('/api/h{n}');\n"))
        .collect();
    dir.write("apps/healthy/src/h.ts", &healthy);

    dir.write("apps/dark/package.json", "{ \"name\": \"dark\" }\n");
    let dark: String = (0..5)
        .map(|n| format!("export const d{n} = (apiBase) => fetch(`${{apiBase}}query{n}`);\n"))
        .collect();
    dir.write("apps/dark/src/d.ts", &dark);

    dir.write("apps/external/package.json", "{ \"name\": \"external\" }\n");
    dir.write(
        "apps/external/src/config.ts",
        "export const CDN = \"https://cdn.example.com/m.json\";\n",
    );
    let mut external = String::from("import { CDN } from './config';\n");
    for n in 0..5 {
        external.push_str(&format!("export const e{n} = () => fetch(CDN);\n"));
    }
    dir.write("apps/external/src/e.ts", &external);
    dir
}

#[test]
fn per_app_census_names_the_dark_app_and_masks_neither_healthy_nor_external() {
    let dir = multi_app_demasking_tree();
    let out = analyze_tree(dir.path(), &config());

    // The dark app fires an APP-SCOPED S5 warning naming its own bucket.
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("within app `apps/dark`") && w.contains("internal-relative URLs")),
        "expected an app-scoped S5 warning naming apps/dark, got: {:?}",
        out.warnings
    );
    // Neither the healthy nor the external app may be named by ANY warning.
    assert!(
        !out.warnings.iter().any(|w| w.contains("apps/healthy")),
        "the keyed (healthy) app must not be named by any warning, got: {:?}",
        out.warnings
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains("apps/external")),
        "the absolute-URL-only (external) app must not be named by any warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn per_app_census_is_deterministic_across_two_runs() {
    let dir = multi_app_demasking_tree();
    let cfg = config();
    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);
    assert_eq!(out1.warnings, out2.warnings);
}

/// A two-package tree where the internal-fetch mass SPLITS across packages below every per-app floor,
/// but its aggregate still clears the call-site floor — proving the census's tree-wide FALLBACK path:
/// `pkgs/x` and `pkgs/y` each hold 3 computed-internal `fetch(`${base}q<n>`)` calls (each 3 < 5, so no
/// per-app warning fires), yet 6 >= 5 tree-wide, so ONE tree-wide-worded fallback warning is emitted.
fn split_below_floor_fallback_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-coverage-split-fallback");
    dir.write("pkgs/x/package.json", "{ \"name\": \"x\" }\n");
    let x: String = (0..3)
        .map(|n| format!("export const x{n} = (base) => fetch(`${{base}}q{n}`);\n"))
        .collect();
    dir.write("pkgs/x/src/x.ts", &x);

    dir.write("pkgs/y/package.json", "{ \"name\": \"y\" }\n");
    let y: String = (0..3)
        .map(|n| format!("export const y{n} = (base) => fetch(`${{base}}q{n}`);\n"))
        .collect();
    dir.write("pkgs/y/src/y.ts", &y);
    dir
}

#[test]
fn split_internal_fetch_mass_fires_one_tree_wide_fallback_not_a_per_app_warning() {
    let dir = split_below_floor_fallback_tree();
    let out = analyze_tree(dir.path(), &config());

    let census_warnings: Vec<&String> = out
        .warnings
        .iter()
        .filter(|w| w.contains("builtin `fetch(` call site(s)"))
        .collect();
    // Exactly one S5 census warning, and it is the TREE-WIDE fallback wording (`extracted tree-wide`),
    // NOT an app-scoped one (`within app ...`). The `pkgs/x`/`pkgs/y` substrings legitimately appear in
    // the sample-file list, so the app-vs-tree discriminator is the wording, not path presence.
    assert_eq!(
        census_warnings.len(),
        1,
        "expected exactly one S5 census warning (the tree-wide fallback), got: {:?}",
        out.warnings
    );
    let w = census_warnings[0];
    assert!(
        w.contains("extracted tree-wide") && !w.contains("within app"),
        "the fallback must use the tree-wide wording and not name any app bucket, got: {w:?}"
    );
}
