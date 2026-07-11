//! End-to-end test for the 6 `cross-layer/*` native rules (`rules/native/rules-graph::cross_layer`), wired
//! from `zzop_engine::analyze_trees` into `MultiAnalyzeOutput::cross_layer_findings`. Mirrors
//! `analyze_multi_tree.rs`'s FE/BE fixture shapes (real TypeScript `fetch` calls + Hono routes, parsed for
//! real — not hand-built `Finding`s) and exercises at least 3 of the 6 rules end to end:
//! `cross-layer/unconsumed-endpoint`, `cross-layer/method-mismatch`, and `cross-layer/duplicate-route`, plus
//! `cross-layer/version-skew` — 4 of 6, all through one small 3-tree fixture. `crossLayerFindings`
//! serialization casing (camelCase, matching every other output-facing type at the napi boundary — see
//! `Finding`'s own `#[serde(rename_all = "camelCase")]`) is asserted directly on `serde_json::to_value`, and
//! `disabledRules` union gating (disabling a cross-layer rule id in only ONE tree's config still drops that
//! rule from the joint output) is asserted by re-running the same fixture with one tree's `disabled_rules`
//! set.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_trees, EngineConfig};

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

/// FE tree: a correctly-joining consume (`GET /authen/getUserInfo`), a method-mismatch consume
/// (`POST /api/v1/orders` — the BE only provides it as `PUT`), and a version-skew consume
/// (`GET /api/v1/accounts` — the BE only provides `GET /api/v2/accounts`).
fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-fe");
    dir.write(
        "src/Ctx.tsx",
        "export function ok() { return fetch(\"/authen/getUserInfo\"); }\n\
         export function mismatch() { return fetch(\"/api/v1/orders\", { method: \"POST\" }); }\n\
         export function skew() { return fetch(\"/api/v1/accounts\"); }\n",
    );
    dir
}

/// BE tree 1: provides the route the FE correctly calls, PLUS `PUT /api/v1/orders` (method-mismatch target)
/// and `GET /api/v2/accounts` (version-skew target), PLUS a dead endpoint nobody calls
/// (`GET /authen/getGoogleRedirect` — drives `cross-layer/unconsumed-endpoint`), PLUS
/// `DELETE /api/legacy/purge` — also provided by BE tree 2, driving cross-tree `duplicate-route`.
fn be1_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-be1");
    dir.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\n\
         apiRoutes.get(\"/authen/getUserInfo\", api.getUserInfo);\n\
         apiRoutes.put(\"/api/v1/orders\", api.updateOrder);\n\
         apiRoutes.get(\"/api/v2/accounts\", api.getAccounts);\n\
         apiRoutes.get(\"/authen/getGoogleRedirect\", api.googleRedirect);\n\
         apiRoutes.delete(\"/api/legacy/purge\", api.purge1);\n",
    );
    dir
}

/// BE tree 2: independently provides the SAME `DELETE /api/legacy/purge` route as BE tree 1 — a genuine
/// cross-tree route duplicate nobody in this fixture consumes (so it surfaces via `unconsumed_provides`).
fn be2_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-be2");
    dir.write(
        "routes/legacy.ts",
        "const legacyRoutes = new Hono();\nlegacyRoutes.delete(\"/api/legacy/purge\", api.purge2);\n",
    );
    dir
}

/// FE tree for the near-miss cross-reference case: consumes `/articles`, missing the `/api` base prefix the
/// BE actually registers — an unprovided consume that near-misses `GET /api/articles` on the `prefix`
/// dimension.
fn near_miss_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-nearmiss-fe");
    dir.write(
        "src/Api.tsx",
        "export function list() { return fetch(\"/articles\"); }\n",
    );
    dir
}

/// BE tree for the near-miss cross-reference case: provides `GET /api/articles` only. Nobody in this
/// fixture calls it under its actual registered path, so this route is simultaneously an
/// `unconsumed-endpoint` finding AND the `route-near-miss` target of the FE's drifted consume — the
/// dogfood-round-8 scenario the cross-reference note exists for.
fn near_miss_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-nearmiss-be");
    dir.write(
        "routes/articles.controller.ts",
        "const articlesRoutes = new Hono();\narticlesRoutes.get(\"/api/articles\", api.list);\n",
    );
    dir
}

/// FE tree with THREE consumes all omitting the `/api` base prefix the BE registers — the prefix-drift
/// aggregation case (3 == `MIN_PREFIX_DRIFT_GROUP`). Each is individually a prefix-dimension route-near-miss;
/// together they must collapse into ONE `cross-layer/prefix-drift`.
fn prefix_drift_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-prefixdrift-fe");
    dir.write(
        "src/Api.tsx",
        "export function a() { return fetch(\"/articles\"); }\n\
         export function b() { return fetch(\"/comments\"); }\n\
         export function c() { return fetch(\"/profiles\"); }\n",
    );
    dir
}

/// BE tree registering the same three routes under the `/api` global prefix — every FE consume above
/// near-misses one of these on the `prefix` dimension.
fn prefix_drift_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-prefixdrift-be");
    dir.write(
        "routes/api.controller.ts",
        "const r = new Hono();\n\
         r.get(\"/api/articles\", api.a);\n\
         r.get(\"/api/comments\", api.b);\n\
         r.get(\"/api/profiles\", api.c);\n",
    );
    dir
}

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

fn find<'a>(findings: &'a [zzop_core::Finding], rule_id: &str) -> Vec<&'a zzop_core::Finding> {
    findings.iter().filter(|f| f.rule_id == rule_id).collect()
}

#[test]
fn cross_layer_findings_cover_at_least_four_of_the_six_rules_end_to_end() {
    let fe = fe_tree();
    let be1 = be1_tree();
    let be2 = be2_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be1.path().to_path_buf(), config("be1")),
        (be2.path().to_path_buf(), config("be2")),
    ];
    let out = analyze_trees(&trees);

    // Sanity: the correctly-matching route still joins as a normal edge, not a cross-layer finding source.
    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "GET /authen/getUserInfo"),
        "expected the correctly-matching route to still join: {:?}",
        out.cross_layer.edges
    );

    // 1. cross-layer/unconsumed-endpoint — the dead Google-redirect route.
    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        unconsumed
            .iter()
            .any(|f| f.message.contains("GET /authen/getGoogleRedirect")),
        "expected an unconsumed-endpoint finding for the dead route: {:?}",
        unconsumed
    );

    // 2. cross-layer/method-mismatch — FE calls POST, BE only provides PUT, same path.
    let mismatch = find(&out.cross_layer_findings, "cross-layer/method-mismatch");
    assert_eq!(mismatch.len(), 1, "{:?}", mismatch);
    assert_eq!(mismatch[0].file, "src/Ctx.tsx");
    assert!(mismatch[0].message.contains("/api/v1/orders"));
    assert!(mismatch[0].message.contains("PUT"));

    // 3. cross-layer/version-skew — FE calls v1, BE only provides v2, rest of the path identical.
    let skew = find(&out.cross_layer_findings, "cross-layer/version-skew");
    assert_eq!(skew.len(), 1, "{:?}", skew);
    assert_eq!(skew[0].file, "src/Ctx.tsx");
    assert!(skew[0].message.contains("`v1`"));
    assert!(skew[0].message.contains("`v2`"));

    // 4. cross-layer/duplicate-route — DELETE /api/legacy/purge provided by both be1 and be2.
    let dup = find(&out.cross_layer_findings, "cross-layer/duplicate-route");
    assert_eq!(dup.len(), 1, "{:?}", dup);
    assert!(dup[0].message.contains("DELETE /api/legacy/purge"));
    assert!(dup[0].message.contains("be1"));
    assert!(dup[0].message.contains("be2"));

    // Deterministic (severity, file, line, ruleId) sort — the same order `merge_findings` gives per-tree
    // findings. Every one of these 4 rules is `Warning` except `unconsumed-endpoint` (`Info`), so `Info`
    // entries must all sort after every `Warning` entry.
    let mut saw_info = false;
    for f in &out.cross_layer_findings {
        if f.severity == zzop_core::Severity::Info {
            saw_info = true;
        } else {
            assert!(
                !saw_info,
                "a non-info finding appeared after an info finding — severity sort violated: {:?}",
                out.cross_layer_findings
            );
        }
    }
}

#[test]
fn cross_layer_findings_serialize_camel_case_like_every_other_output_type() {
    let fe = fe_tree();
    let be1 = be1_tree();
    let be2 = be2_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be1.path().to_path_buf(), config("be1")),
        (be2.path().to_path_buf(), config("be2")),
    ];
    let out = analyze_trees(&trees);
    assert!(!out.cross_layer_findings.is_empty());

    let value = serde_json::to_value(&out.cross_layer_findings).unwrap();
    let first = value.as_array().unwrap().first().unwrap();
    let obj = first.as_object().unwrap();
    assert!(obj.contains_key("ruleId"), "{obj:?}");
    assert!(!obj.contains_key("rule_id"), "{obj:?}");
    assert!(obj.contains_key("severity"));
    assert!(obj.contains_key("file"));
    assert!(obj.contains_key("line"));
    assert!(obj.contains_key("message"));
}

#[test]
fn disabling_a_cross_layer_rule_in_only_one_tree_drops_it_from_the_union() {
    let fe = fe_tree();
    let be1 = be1_tree();
    let be2 = be2_tree();

    let mut fe_config = config("fe");
    fe_config.rule_config.disabled_rules = vec!["cross-layer/method-mismatch".to_string()];

    let trees = vec![
        (fe.path().to_path_buf(), fe_config),
        (be1.path().to_path_buf(), config("be1")), // does NOT disable it itself
        (be2.path().to_path_buf(), config("be2")),
    ];
    let out = analyze_trees(&trees);

    assert!(
        find(&out.cross_layer_findings, "cross-layer/method-mismatch").is_empty(),
        "one tree disabling a cross-layer rule must drop it from the joint output: {:?}",
        out.cross_layer_findings
    );
    // Sibling rules untouched by the disable — still present.
    assert!(!find(&out.cross_layer_findings, "cross-layer/version-skew").is_empty());
    assert!(!find(&out.cross_layer_findings, "cross-layer/duplicate-route").is_empty());
}

/// Dogfood 2026-07-11 (fe-axios x be-nest, 19 `/api`-drifted consumes): 3+ prefix near-misses sharing one
/// base prefix must collapse into ONE `cross-layer/prefix-drift` aggregate, and the per-route
/// `route-near-miss` findings they subsume must be replaced (not co-reported).
#[test]
fn prefix_drift_aggregates_and_replaces_per_route_near_misses() {
    let fe = prefix_drift_fe_tree();
    let be = prefix_drift_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let drift = find(&out.cross_layer_findings, "cross-layer/prefix-drift");
    assert_eq!(drift.len(), 1, "{:?}", out.cross_layer_findings);
    assert_eq!(drift[0].data.as_ref().unwrap()["routeCount"], 3);
    assert!(drift[0].message.contains("/api"));
    assert!(drift[0].message.contains("missing"));

    // All three per-route near-misses are subsumed by the aggregate — none survive in the output.
    let near_miss = find(&out.cross_layer_findings, "cross-layer/route-near-miss");
    assert!(
        near_miss.is_empty(),
        "prefix-drift must replace the subsumed route-near-misses: {near_miss:?}"
    );
}

/// Disabling `cross-layer/prefix-drift` alone (aggregate off) must restore the per-route `route-near-miss`
/// findings — the suppression is the aggregate's doing, not a standalone drop.
#[test]
fn disabling_prefix_drift_restores_the_per_route_near_misses() {
    let fe = prefix_drift_fe_tree();
    let be = prefix_drift_be_tree();
    let mut fe_config = config("fe");
    fe_config.rule_config.disabled_rules = vec!["cross-layer/prefix-drift".to_string()];
    let trees = vec![
        (fe.path().to_path_buf(), fe_config),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    assert!(find(&out.cross_layer_findings, "cross-layer/prefix-drift").is_empty());
    let near_miss = find(&out.cross_layer_findings, "cross-layer/route-near-miss");
    assert_eq!(near_miss.len(), 3, "{near_miss:?}");
}

/// Dogfood round 8: a route-near-miss target that is also an unconsumed provide must carry a cross-reference
/// note back to the `cross-layer/route-near-miss` finding, so agent-facing output stops calling the same
/// route both "not called by any source" and "did you mean this route" without ever linking the two.
#[test]
fn unconsumed_endpoint_cross_references_its_route_near_miss_finding() {
    let fe = near_miss_fe_tree();
    let be = near_miss_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let near_miss = find(&out.cross_layer_findings, "cross-layer/route-near-miss");
    assert_eq!(near_miss.len(), 1, "{:?}", near_miss);
    assert!(near_miss[0].message.contains("GET /api/articles"));

    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert_eq!(unconsumed.len(), 1, "{:?}", unconsumed);
    assert!(
        unconsumed[0]
            .message
            .contains("cross-layer/route-near-miss"),
        "expected the unconsumed-endpoint finding to cross-reference route-near-miss: {}",
        unconsumed[0].message
    );
    assert!(unconsumed[0].message.contains("src/Api.tsx"));

    let data = unconsumed[0].data.as_ref().unwrap();
    assert_eq!(data["nearMissConsumeCount"], 1);
    assert_eq!(data["nearMissConsumeExample"], "src/Api.tsx:1");
}

// --- tRPC mount-route suppression (dogfood round 9) ---
//
// A tRPC starter's `pages/api/trpc/[trpc].ts` file-convention mount route always looks unconsumed to the
// http-provide/consume join — nothing calls "GET/POST /api/trpc/{}" as an ordinary fetch. But when the SAME
// analysis composed the router into `trpc`-kind PROVIDEs and joined at least one client CONSUME to them, that
// mount route IS the transport those edges flow through, so reporting it as a dead endpoint is tone noise.
// `web_tree_with_trpc_edge` composes a full router (`viewer.ts` -> `trpc.ts`) consumed by `page.tsx` (>= 1
// `trpc` edge) AND registers the `pages/api/trpc/[trpc].ts` mount file (no verb checks in its body, so
// `file_routes` emits both the GET and POST fallback verbs for it).

fn web_tree_with_trpc_edge() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-trpc-web");
    dir.write(
        "viewer.ts",
        "export const viewerRouter = router({ me: publicProcedure.query(() => 1) });\n",
    );
    dir.write(
        "trpc.ts",
        "import { viewerRouter } from './viewer';\nexport const appRouter = router({ viewer: viewerRouter });\n",
    );
    dir.write(
        "page.tsx",
        "import { client } from './trpc-client';\nclient.viewer.me.useQuery();\n",
    );
    dir.write(
        "pages/api/trpc/[trpc].ts",
        "export default createNextApiHandler({ router: appRouter });\n",
    );
    dir
}

/// Same router + mount file as [`web_tree_with_trpc_edge`], but WITHOUT `page.tsx` — nothing in the run
/// consumes the composed `trpc` procedure, so `cross_layer.edges` carries zero `trpc`-kind edges and the
/// mount route's "unconsumed" reading is not tone noise; it must stay reported.
fn web_tree_without_trpc_edge() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-trpc-web-noedge");
    dir.write(
        "viewer.ts",
        "export const viewerRouter = router({ me: publicProcedure.query(() => 1) });\n",
    );
    dir.write(
        "trpc.ts",
        "import { viewerRouter } from './viewer';\nexport const appRouter = router({ viewer: viewerRouter });\n",
    );
    dir.write(
        "pages/api/trpc/[trpc].ts",
        "export default createNextApiHandler({ router: appRouter });\n",
    );
    dir
}

#[test]
fn trpc_mount_route_unconsumed_findings_are_suppressed_when_the_run_has_a_trpc_edge() {
    let web = web_tree_with_trpc_edge();
    let trees = vec![(web.path().to_path_buf(), config("web"))];
    let out = analyze_trees(&trees);

    assert_eq!(
        out.cross_layer
            .edges
            .iter()
            .filter(|e| e.kind == "trpc")
            .count(),
        1,
        "expected exactly one trpc edge: {:?}",
        out.cross_layer.edges
    );

    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        unconsumed.iter().all(|f| !f.message.contains("/api/trpc/")),
        "expected the tRPC mount route to be suppressed from unconsumed-endpoint: {:?}",
        unconsumed
    );
    let unconsumed_mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unconsumed-mutation-endpoint",
    );
    assert!(
        unconsumed_mutation
            .iter()
            .all(|f| !f.message.contains("/api/trpc/")),
        "expected the tRPC mount route's POST verb to be suppressed from unconsumed-mutation-endpoint: {:?}",
        unconsumed_mutation
    );

    // Disclosure: the suppression must surface on the owning tree's `warnings`, never silently.
    let (_, _, web_output) = out
        .trees
        .iter()
        .find(|(_, source, _)| source == "web")
        .expect("expected the web tree in the output");
    let note = web_output
        .warnings
        .iter()
        .find(|w| w.contains("tRPC mount"))
        .unwrap_or_else(|| {
            panic!(
                "expected a tRPC mount suppression note, got: {:?}",
                web_output.warnings
            )
        });
    assert!(note.contains("2 tRPC mount routes"), "{note}");
    assert!(note.contains("GET /api/trpc/{}"), "{note}");
    assert!(note.contains("POST /api/trpc/{}"), "{note}");
    assert!(note.contains("1 tRPC edge"), "{note}");
    assert!(note.contains("tRPC transport"), "{note}");
}

#[test]
fn trpc_mount_route_stays_reported_when_the_run_has_zero_trpc_edges() {
    let web = web_tree_without_trpc_edge();
    let trees = vec![(web.path().to_path_buf(), config("web"))];
    let out = analyze_trees(&trees);

    assert_eq!(
        out.cross_layer
            .edges
            .iter()
            .filter(|e| e.kind == "trpc")
            .count(),
        0,
        "expected zero trpc edges: {:?}",
        out.cross_layer.edges
    );

    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        unconsumed
            .iter()
            .any(|f| f.message.contains("GET /api/trpc/{}")),
        "expected the tRPC mount route to stay reported with zero trpc edges: {:?}",
        unconsumed
    );
    let unconsumed_mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unconsumed-mutation-endpoint",
    );
    assert!(
        unconsumed_mutation
            .iter()
            .any(|f| f.message.contains("POST /api/trpc/{}")),
        "expected the tRPC mount route's POST verb to stay reported with zero trpc edges: {:?}",
        unconsumed_mutation
    );

    let (_, _, web_output) = out
        .trees
        .iter()
        .find(|(_, source, _)| source == "web")
        .expect("expected the web tree in the output");
    assert!(
        web_output
            .warnings
            .iter()
            .all(|w| !w.contains("tRPC mount")),
        "expected no suppression note when nothing was suppressed: {:?}",
        web_output.warnings
    );
}

/// A tree with NO tRPC router of its own — just an http route whose path coincidentally carries a literal
/// `trpc` segment (e.g. a REST endpoint left over from a retired tRPC installation, unrelated to any OTHER
/// tree's real tRPC setup). `file_routes`'s content-blind convention scan still emits GET/POST provides for
/// it (any `pages/api/**` default-export file does), so [`super::is_trpc_mount_route_key`]-equivalent
/// matching still fires on the path shape alone — the per-tree edge-participation gate is the only thing
/// standing between this coincidence and a wrongful suppression.
fn other_tree_with_coincidental_trpc_route() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-trpc-other");
    dir.write(
        "pages/api/trpc/[trpc].ts",
        "export default function handler(req, res) {}\n",
    );
    dir
}

#[test]
fn trpc_mount_route_in_a_tree_with_no_trpc_edges_of_its_own_stays_reported_even_when_another_tree_has_trpc_edges(
) {
    // Class A regression: a run-global `trpc_edge_count` gate would suppress tree `other`'s literal
    // trpc-segment route purely because tree `web` (a DIFFERENT tree in the same run) has real tRPC
    // edges — the mount-IS-transport justification only holds for the tree whose OWN edges actually flow
    // through that route. `other` has zero tRPC edges of its own, so its route must stay reported.
    let web = web_tree_with_trpc_edge();
    let other = other_tree_with_coincidental_trpc_route();
    let trees = vec![
        (web.path().to_path_buf(), config("web")),
        (other.path().to_path_buf(), config("other")),
    ];
    let out = analyze_trees(&trees);

    assert_eq!(
        out.cross_layer
            .edges
            .iter()
            .filter(|e| e.kind == "trpc")
            .count(),
        1,
        "expected exactly one trpc edge, owned by `web`: {:?}",
        out.cross_layer.edges
    );

    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    let source_of = |f: &&zzop_core::Finding| {
        f.data
            .as_ref()
            .and_then(|d| d.get("source"))
            .and_then(|s| s.as_str())
            .map(str::to_string)
    };

    // `web`'s own mount routes are still suppressed (unchanged from the single-tree test above).
    assert!(
        unconsumed
            .iter()
            .all(|f| source_of(f).as_deref() != Some("web") || !f.message.contains("/api/trpc/")),
        "expected web's own tRPC mount routes to stay suppressed: {:?}",
        unconsumed
    );

    // `other`'s coincidental trpc-segment route is NOT suppressed — it has zero tRPC edges of its own.
    let other_finding = unconsumed
        .iter()
        .find(|f| source_of(f).as_deref() == Some("other"))
        .unwrap_or_else(|| {
            panic!(
                "expected tree `other`'s coincidental trpc-segment route to stay reported, not \
                 suppressed by tree `web`'s edges: {:?}",
                unconsumed
            )
        });
    assert!(other_finding.message.contains("GET /api/trpc/{}"));

    // Disclosure must be per-tree too: `web` still gets its suppression note, `other` gets none (nothing
    // of its own was suppressed — a note there would be a phantom disclosure).
    let (_, _, web_output) = out
        .trees
        .iter()
        .find(|(_, source, _)| source == "web")
        .expect("expected the web tree in the output");
    assert!(
        web_output.warnings.iter().any(|w| w.contains("tRPC mount")),
        "expected web to still get its own suppression note: {:?}",
        web_output.warnings
    );
    let (_, _, other_output) = out
        .trees
        .iter()
        .find(|(_, source, _)| source == "other")
        .expect("expected the other tree in the output");
    assert!(
        other_output
            .warnings
            .iter()
            .all(|w| !w.contains("tRPC mount")),
        "expected no suppression note on `other` — nothing of its own was suppressed: {:?}",
        other_output.warnings
    );
}
