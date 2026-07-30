//! End-to-end coverage for the whole-tree `Matcher::IoScan` DSL pass (the 2026 projection redesign's
//! engine half): `eval_pack_io_scan`, run once after assemble/ingest rather than per-file, wired through
//! both `analyze_tree` (native) and `analyze_envelope` (Mode A). One inline custom pack (`io-scan-e2e`,
//! built as a `RulePackDef` literal — same pattern `analyze_profiling.rs`'s `security_java_pack`/
//! `analyze_rule_config.rs` use for injecting packs via `EngineConfig::packs`) carries every `IoScan` rule
//! this file needs. Proves:
//!
//! - THE DESIGN WIN: an assemble-composed Express router-mount route (invisible to the OLD per-file
//!   `IoScan` evaluation, since neither contributing file's own `IoFacts` carries the composed key) is
//!   matched by a whole-tree `IoScan` rule (`admin-route-scan`).
//! - `attr_absent` veto via a NATIVE Express middleware guard (`compose_router_mount_provides`'s own
//!   `auth-guarded` `PathScope` attribute) — `unguarded-mutation-scan` does not fire for the guarded tree,
//!   does fire for the unguarded sibling.
//! - `attr_absent` veto via engine-MINTED decorator-guard evidence — a NestJS `@UseGuards` route, whose
//!   guard the call-graph BFS itself cannot see, is exempted the same way once `analyze::assemble::rules`
//!   mints `auth-guarded` from `run_callgraph_rules`' exported `decorator_guarded` set.
//! - Suppress-marker recognition (`marker-scan`) via the native `anchor_line` channel: a `// zzop-my-rule-ok`
//!   comment on the route's own registration line suppresses; its absence fires.
//! - Envelope mode (`envelope-idempotency-scan`): `attr_absent` honors an envelope-injected attribute, and
//!   the derived `zzop-<id>-ok` suppress marker has no effect (envelope mode's `anchor_line` is always `None` —
//!   no source text to check).
//! - `disabled_rules` gates an `IoScan` rule exactly like every other rule id.
//!
//! Self-contained `TempDir`/`config` helpers, same pattern as `analyze_native_middleware.rs`/
//! `analyze_attribute_injection.rs` (each `tests/*.rs` file is its own separate test binary/crate).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use zzop_core::{
    Attribute, EntityRef, FileProjection, Finding, IoConsume, IoDirection, IoFacts, IoProvide,
    IoScan, Matcher, NormalizedEnvelope, RuleConfig, RuleDef, RulePackDef, Severity,
    NORMALIZED_AST_FORMAT,
};
use zzop_engine::{analyze_envelope, analyze_tree, AnalyzeOutput, EngineConfig};
use zzop_rules_http::mutating_route_no_auth::AUTH_GUARDED_ATTR;

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

fn io_scan_rule(id: &str, m: IoScan) -> RuleDef {
    // Suppress marker is derived `zzop-<id>-ok` (see `RuleDef::suppress_marker`), so the rule's id alone
    // determines the marker text a fixture comment must carry — e.g. `marker-scan` -> `// zzop-marker-scan-ok`.
    RuleDef {
        id: id.to_string(),
        severity: Severity::Warning,
        message: format!("io-scan-e2e/{id} fired"),
        matcher: Matcher::IoScan(m),
    }
}

/// The one inline pack every test in this file loads — see the module doc for what each rule proves.
fn pack() -> RulePackDef {
    RulePackDef {
        id: "io-scan-e2e".to_string(),
        framework: "any".to_string(),
        schema_version: 1,
        fragments: BTreeMap::new(),
        rules: vec![
            // (a) design win: matches only an assemble-composed provide under /admin.
            io_scan_rule(
                "admin-route-scan",
                IoScan {
                    file_pattern: ".*".to_string(),
                    file_exclude_pattern: None,
                    direction: IoDirection::Provides,
                    kind: Some("http".to_string()),
                    key_pattern: Some("/admin".to_string()),
                    negate: false,
                    symbol_pattern: None,
                    attr_absent: None,
                    attr_present: None,
                    anchor_exclude_pattern: None,
                },
            ),
            // (b)/(c) attr_absent veto — native middleware guard AND minted decorator-guard evidence.
            io_scan_rule(
                "unguarded-mutation-scan",
                IoScan {
                    file_pattern: ".*".to_string(),
                    file_exclude_pattern: None,
                    direction: IoDirection::Provides,
                    kind: Some("http".to_string()),
                    key_pattern: None,
                    negate: false,
                    symbol_pattern: None,
                    attr_absent: Some(AUTH_GUARDED_ATTR.to_string()),
                    attr_present: None,
                    anchor_exclude_pattern: None,
                },
            ),
            // (d) suppress-marker recognition via the native anchor_line channel.
            io_scan_rule(
                "marker-scan",
                IoScan {
                    file_pattern: ".*".to_string(),
                    file_exclude_pattern: None,
                    direction: IoDirection::Provides,
                    kind: Some("http".to_string()),
                    key_pattern: None,
                    negate: false,
                    symbol_pattern: None,
                    attr_absent: None,
                    attr_present: None,
                    anchor_exclude_pattern: None,
                },
            ),
            // (e) envelope mode: attr_absent honors an injected attribute; suppress_marker is inert there.
            io_scan_rule(
                "envelope-idempotency-scan",
                IoScan {
                    file_pattern: ".*".to_string(),
                    file_exclude_pattern: None,
                    direction: IoDirection::Provides,
                    kind: Some("http".to_string()),
                    key_pattern: None,
                    negate: false,
                    symbol_pattern: None,
                    attr_absent: Some("idempotent".to_string()),
                    attr_present: None,
                    anchor_exclude_pattern: None,
                },
            ),
        ],
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "io-scan-tree-fixture".to_string(),
        packs: vec![pack()],
        ..EngineConfig::default()
    }
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("io-scan-e2e/{rule}"))
        .collect()
}

fn snippet(f: &Finding) -> &str {
    f.data.as_ref().unwrap()["snippet"].as_str().unwrap()
}

// ---------------------------------------------------------------------------------------------
// (a) THE DESIGN WIN — an assemble-composed router-mount route, invisible to the old per-file pass.
// ---------------------------------------------------------------------------------------------

#[test]
fn design_win_admin_route_scan_matches_only_the_assemble_composed_mount_route() {
    let dir = TempDir::new("zzop-io-scan-tree-design-win");
    // Neither file's OWN `IoFacts` carries a "/admin"-prefixed key: `router.ts` provides plain
    // `POST /tasks`, and `app.ts` provides nothing at all (it only calls `.use`) — the composed
    // `POST /admin/tasks` key exists only after `compose_router_mount_provides` runs at assemble time.
    dir.write(
        "routes/router.ts",
        concat!(
            "import { Router } from 'express';\n",
            "const r = Router();\n",
            "r.post('/tasks', createTask);\n",
            "export default r;\n"
        ),
    );
    dir.write(
        "routes/app.ts",
        concat!(
            "import express from 'express';\n",
            "import router from './router';\n",
            "const app = express();\n",
            "app.use('/admin', router);\n"
        ),
    );
    dir.write(
        "routes/handlers.ts",
        concat!(
            "export function createTask(c) {\n",
            "  return prisma.task.create({ data: {} });\n",
            "}\n"
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    let found = hits(&out, "admin-route-scan");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(snippet(found[0]), "POST /admin/tasks");
}

// ---------------------------------------------------------------------------------------------
// (b) attr_absent veto via a NATIVE Express middleware guard.
// ---------------------------------------------------------------------------------------------

#[test]
fn native_middleware_guard_clears_attr_absent_but_the_unguarded_sibling_still_fires() {
    let guarded = TempDir::new("zzop-io-scan-tree-mw-guarded");
    guarded.write(
        "routes/api.ts",
        concat!(
            "const app = express();\n",
            "app.use('/admin', requireAuth());\n",
            "app.post('/admin/widgets', createWidget);\n"
        ),
    );
    guarded.write(
        "routes/handlers.ts",
        "export function createWidget(c) {\n  return prisma.widget.create({ data: {} });\n}\n",
    );
    let out = analyze_tree(guarded.path(), &config());
    assert!(
        hits(&out, "unguarded-mutation-scan").is_empty(),
        "the /admin-scoped native middleware guard must clear attr_absent: {:?}",
        out.findings
    );

    let unguarded = TempDir::new("zzop-io-scan-tree-mw-unguarded");
    unguarded.write(
        "routes/api.ts",
        "const app = express();\napp.post('/public/items', createItem);\n",
    );
    unguarded.write(
        "routes/handlers.ts",
        "export function createItem(c) {\n  return prisma.item.create({ data: {} });\n}\n",
    );
    let out = analyze_tree(unguarded.path(), &config());
    let found = hits(&out, "unguarded-mutation-scan");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(snippet(found[0]), "POST /public/items");
}

// ---------------------------------------------------------------------------------------------
// (c) attr_absent veto via engine-MINTED decorator-guard evidence (NestJS @UseGuards).
// ---------------------------------------------------------------------------------------------

#[test]
fn nestjs_use_guards_decorator_mints_auth_guarded_and_clears_attr_absent() {
    let guarded = TempDir::new("zzop-io-scan-tree-decorator-guarded");
    guarded.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post, UseGuards } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "@UseGuards(JwtAuthGuard)\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = analyze_tree(guarded.path(), &config());
    assert!(
        hits(&out, "unguarded-mutation-scan").is_empty(),
        "the minted auth-guarded attribute (from decorator_guarded) must clear attr_absent: {:?}",
        out.findings
    );

    let unguarded = TempDir::new("zzop-io-scan-tree-decorator-unguarded");
    unguarded.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = analyze_tree(unguarded.path(), &config());
    let found = hits(&out, "unguarded-mutation-scan");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(snippet(found[0]), "POST /items/x");
}

// ---------------------------------------------------------------------------------------------
// (d) suppress-marker recognition via the native anchor_line channel.
// ---------------------------------------------------------------------------------------------

#[test]
fn suppress_marker_on_the_registration_line_suppresses_without_it_fires() {
    let marked = TempDir::new("zzop-io-scan-tree-marker-present");
    marked.write(
        "routes/api.ts",
        "const app = express();\napp.post('/orders', createOrder); // zzop-marker-scan-ok\n",
    );
    marked.write(
        "routes/handlers.ts",
        "export function createOrder(c) {\n  return prisma.order.create({ data: {} });\n}\n",
    );
    let out = analyze_tree(marked.path(), &config());
    assert!(
        hits(&out, "marker-scan").is_empty(),
        "the // zzop-marker-scan-ok comment on the registration line must suppress: {:?}",
        out.findings
    );

    let unmarked = TempDir::new("zzop-io-scan-tree-marker-absent");
    unmarked.write(
        "routes/api.ts",
        "const app = express();\napp.post('/orders', createOrder);\n",
    );
    unmarked.write(
        "routes/handlers.ts",
        "export function createOrder(c) {\n  return prisma.order.create({ data: {} });\n}\n",
    );
    let out = analyze_tree(unmarked.path(), &config());
    let found = hits(&out, "marker-scan");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(snippet(found[0]), "POST /orders");
}

// ---------------------------------------------------------------------------------------------
// (e) envelope mode: attr_absent honors an injected attribute; suppress_marker is inert (no text).
// ---------------------------------------------------------------------------------------------

fn envelope_projection(path: &str, provide_key: &str, attrs: Vec<Attribute>) -> FileProjection {
    FileProjection {
        path: path.to_string(),
        loc: 1,
        symbols: Vec::new(),
        imports: zzop_core::ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: std::collections::HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        class_shape_fragments: Vec::new(),
        io: IoFacts {
            provides: vec![IoProvide {
                kind: "http".to_string(),
                key: provide_key.to_string(),
                file: path.to_string(),
                line: 1,
                symbol: None,
                body: None,
            }],
            consumes: Vec::<IoConsume>::new(),
        },
        degraded: false,
        is_entry: false,
        overrides: Default::default(),
        attributes: attrs,
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
    }
}

#[test]
fn envelope_mode_attr_absent_honors_injected_attribute_and_ignores_the_inert_suppress_marker() {
    let envelope = NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "io-scan-e2e-adapter/1".to_string(),
        source: "test".to_string(),
        files: vec![
            // No injected attribute — fires, and the rule's own derived marker `zzop-envelope-idempotency-scan-ok`
            // has no effect (envelope mode's `anchor_line` is always `None` — no source text to check).
            envelope_projection("routes/a.json", "POST /orders", Vec::new()),
            // Injected `idempotent` attribute on this exact route — attr_absent clears it.
            envelope_projection(
                "routes/b.json",
                "POST /payments",
                vec![Attribute {
                    target: EntityRef::IoKey {
                        kind: "http".to_string(),
                        key: "POST /payments".to_string(),
                    },
                    key: "idempotent".to_string(),
                    value: json!(true),
                }],
            ),
        ],
    };

    let out = analyze_envelope(&envelope, &config());
    let found = hits(&out, "envelope-idempotency-scan");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "routes/a.json");
    assert_eq!(snippet(found[0]), "POST /orders");
}

// ---------------------------------------------------------------------------------------------
// (f) disabled_rules gates an IoScan rule exactly like every other rule id.
// ---------------------------------------------------------------------------------------------

#[test]
fn disabled_rules_gates_an_io_scan_rule_id_like_any_other() {
    let dir = TempDir::new("zzop-io-scan-tree-disabled");
    dir.write(
        "routes/router.ts",
        concat!(
            "import { Router } from 'express';\n",
            "const r = Router();\n",
            "r.post('/tasks', createTask);\n",
            "export default r;\n"
        ),
    );
    dir.write(
        "routes/app.ts",
        concat!(
            "import express from 'express';\n",
            "import router from './router';\n",
            "const app = express();\n",
            "app.use('/admin', router);\n"
        ),
    );
    dir.write(
        "routes/handlers.ts",
        "export function createTask(c) {\n  return prisma.task.create({ data: {} });\n}\n",
    );

    // Baseline (same fixture as the design-win test): the rule fires.
    let baseline = analyze_tree(dir.path(), &config());
    assert_eq!(
        hits(&baseline, "admin-route-scan").len(),
        1,
        "{:?}",
        baseline.findings
    );

    let mut cfg = config();
    cfg.rule_config = RuleConfig {
        disabled_rules: vec!["io-scan-e2e/admin-route-scan".to_string()],
        ..RuleConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        hits(&out, "admin-route-scan").is_empty(),
        "disabled_rules must gate an IoScan rule id: {:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// (c2) MINTED decorator-guard evidence, Django edition — a guard that is neither a call NOR a line
// ---------------------------------------------------------------------------------------------

/// Django is the shape that forced the Python guard producer to have TWO halves. Its route anchor lives
/// in `urls.py` while the auth evidence (`permission_classes`) lives on the VIEW CLASS in `views.py`, so
/// the evidence cannot be a `(file, line)` the way FastAPI's `Depends` is — it is joined to the provide
/// by the provide's own `symbol` (`callgraph::python_guard::apply_django_view_guards`).
///
/// This is also the ONLY way to observe that join: a Django URLconf registers a PATH bound to a view,
/// never a METHOD, so every Django provide carries the `UNKNOWN_VERB` sentinel and the native
/// `mutating-route-no-auth` rule (which gates on write METHODS) is structurally out of reach for it. The
/// minted `auth-guarded` attribute an `IoScan` rule reads is the channel that is live for Django.
///
/// SHAPE FROM CORPUS: `corpus/oss/be-django/conduit/apps/articles/{urls,views}.py` — `ArticlesFeedAPIView`
/// (`IsAuthenticated`) and `TagListAPIView` (`AllowAny`), registered side by side in one URLconf.
fn django_permission_classes_tree(dir: &TempDir) {
    dir.write(
        "conduit/apps/articles/urls.py",
        concat!(
            "from django.conf.urls import url\n\n",
            "from .views import ArticlesFeedAPIView, TagListAPIView\n\n",
            "urlpatterns = [\n",
            "    url(r'^articles/feed$', ArticlesFeedAPIView.as_view()),\n",
            "    url(r'^tags$', TagListAPIView.as_view()),\n",
            "]\n"
        ),
    );
    dir.write(
        "conduit/apps/articles/views.py",
        concat!(
            "from rest_framework import generics\n",
            "from rest_framework.permissions import AllowAny, IsAuthenticated\n\n",
            "class ArticlesFeedAPIView(generics.ListAPIView):\n",
            "    permission_classes = (IsAuthenticated,)\n\n",
            "class TagListAPIView(generics.ListAPIView):\n",
            "    permission_classes = (AllowAny,)\n"
        ),
    );
}

#[test]
fn django_permission_classes_mint_auth_guarded_only_for_the_authenticated_view() {
    let dir = TempDir::new("zzop-io-scan-tree-django-permissions");
    django_permission_classes_tree(&dir);
    let out = analyze_tree(dir.path(), &config());
    let found = hits(&out, "unguarded-mutation-scan");
    let keys: Vec<&str> = found.iter().map(|f| snippet(f)).collect();
    // `AllowAny` is not auth evidence, so that route stays unguarded and fires; `IsAuthenticated` mints
    // `auth-guarded` on the OTHER route's key and vetoes it. Both routes exist — the difference is the
    // attribute, not extraction.
    assert_eq!(keys, vec!["? /tags"], "{:?}", out.findings);
}

/// Seals the by-NAME join's ambiguity drop against the case the producer's own verdicts CANNOT see: a
/// second app declaring a same-named view that declares no `permission_classes` at all. Absence is not
/// evidence of absence of auth, so that class is correctly absent from the producer's output — which
/// means it never registered as a verdict DISAGREEMENT, and the one app that did declare a guard exempted
/// the other app's route too. Django's default layout makes this ordinary: one `UserDetailView` per app.
///
/// SHAPE FROM CORPUS: `corpus/oss/be-django/conduit/apps/{articles,profiles}/` — the per-app
/// `urls.py` + `views.py` pair this checkout repeats for every app.
#[test]
fn a_same_named_view_in_a_second_app_is_ambiguous_and_clears_nothing() {
    let dir = TempDir::new("zzop-io-scan-tree-django-same-name");
    dir.write(
        "conduit/apps/articles/urls.py",
        concat!(
            "from django.conf.urls import url\n\n",
            "from .views import UserDetailView\n\n",
            "urlpatterns = [\n",
            "    url(r'^articles/author$', UserDetailView.as_view()),\n",
            "]\n"
        ),
    );
    dir.write(
        "conduit/apps/articles/views.py",
        concat!(
            "from rest_framework import generics\n",
            "from rest_framework.permissions import IsAuthenticated\n\n",
            "class UserDetailView(generics.RetrieveAPIView):\n",
            "    permission_classes = (IsAuthenticated,)\n"
        ),
    );
    dir.write(
        "conduit/apps/profiles/urls.py",
        concat!(
            "from django.conf.urls import url\n\n",
            "from .views import UserDetailView\n\n",
            "urlpatterns = [\n",
            "    url(r'^profiles/me$', UserDetailView.as_view()),\n",
            "]\n"
        ),
    );
    // Declares NO permission_classes — so the producer reports nothing for it, and the name is decided
    // only by the OTHER app's class unless the join notices the collision.
    dir.write(
        "conduit/apps/profiles/views.py",
        concat!(
            "from rest_framework import generics\n\n",
            "class UserDetailView(generics.RetrieveAPIView):\n",
            "    queryset = None\n"
        ),
    );
    let out = analyze_tree(dir.path(), &config());
    let mut keys: Vec<&str> = hits(&out, "unguarded-mutation-scan")
        .iter()
        .map(|f| snippet(f))
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["? /articles/author", "? /profiles/me"],
        "{:?}",
        out.findings
    );
}
