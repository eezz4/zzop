//! End-to-end test for the 6 `cross-layer/*` native rules (`rules/native/rules-graph::cross_layer`), wired
//! from `zzop_engine::analyze_trees` into `MultiAnalyzeOutput::cross_layer_findings`. Mirrors
//! `analyze_multi_tree.rs`'s FE/BE fixture shapes (real TypeScript `fetch` calls + Hono routes, parsed for
//! real — not hand-built `Finding`s) and exercises at least 3 of the 6 rules end to end:
//! `cross-layer/unconsumed-endpoint`, `cross-layer/method-mismatch`, and `cross-layer/duplicate-route`, plus
//! `cross-layer/version-skew` — 4 of 6, all through one small 3-tree fixture. `crossLayerFindings`
//! serialization casing (camelCase, matching every other output-facing type at the wire boundary — see
//! `Finding`'s own `#[serde(rename_all = "camelCase")]`) is asserted directly on `serde_json::to_value`, and
//! `disabledRules` union gating (disabling a cross-layer rule id in only ONE tree's config still drops that
//! rule from the joint output) is asserted by re-running the same fixture with one tree's `disabled_rules`
//! set.

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

/// FE tree with exactly `MIN_TOTAL_CONSUMES` (5) total `http` consumes, 3 of them dynamic-URL calls
/// (`axios.get(buildUrl(...))`) the egress extractor cannot resolve to a key — majority-unresolved
/// (`unresolved * 2 >= total`: 3*2=6 >= 5) and above the small-sample floor, so this tree is BLIND per
/// `zzop_rules_cross_layer::majority_unresolved_http_sources`.
fn blind_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-blind-fe");
    dir.write(
        "src/Api.tsx",
        "export function a() { return axios.get(buildUrl(x)); }\n\
         export function b() { return axios.get(buildUrl(y)); }\n\
         export function c() { return axios.get(buildUrl(z)); }\n\
         export function d() { return fetch(\"/health\"); }\n\
         export function e() { return fetch(\"/status\"); }\n",
    );
    dir
}

/// BE tree providing a single unconsumed write route (`POST /api/group`) — nobody in the run calls it,
/// driving both `cross-layer/unconsumed-endpoint` and `cross-layer/unconsumed-mutation-endpoint`.
fn write_only_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-write-be");
    dir.write(
        "routes/group.controller.ts",
        "const groupRoutes = new Hono();\ngroupRoutes.post(\"/api/group\", api.createGroup);\n",
    );
    dir
}

/// FE tree with one unmatched write consume (`POST /api/orders`) — nobody in the run provides it, driving
/// `cross-layer/unprovided-mutation-call`.
fn unprovided_write_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-unprovided-write-fe");
    dir.write(
        "src/Api.tsx",
        "export function create() { return fetch(\"/api/orders\", { method: \"POST\" }); }\n",
    );
    dir
}

/// BE tree that imports a server framework (`express`) yet only registers ONE http route — below
/// `MIN_PROVIDES_FLOOR` (3) — the S2 framework-silence tripwire condition
/// (`zzop_engine::framework_silence::server_framework_import_warning`), lifted by
/// `provide_blind_sources` into `cross-layer/unprovided-mutation-call`'s severity gate. Registers `/health`
/// only — never `/api/orders` — so it never satisfies `unprovided_write_fe_tree`'s write call either way;
/// this tree's whole point is to make the RUN provide-blind, not to actually provide the target.
fn provide_blind_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-provide-blind-be");
    dir.write(
        "src/app.ts",
        "import express from \"express\";\n\
         const app = express();\n\
         app.get(\"/health\", () => {});\n",
    );
    dir
}

/// A tree that analyzes one real file but extracts ZERO io facts either way (no route registration, no
/// http call) — the extraction-blindness caveat's "contributed no joinable io at all" substrate. Stands in
/// for a tree whose sources failed to parse/extract for real (e.g. an unsupported framework idiom), without
/// needing an actually-broken fixture: zero joinable io is the FACT the caveat gates on, regardless of why.
fn zero_io_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlf-zero-io");
    dir.write(
        "src/util.ts",
        "export function add(a: number, b: number) { return a + b; }\n",
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
// `file_routes` emits ONE `UNKNOWN_VERB` sentinel provide `? /api/trpc/{}` for it, not fabricated GET/POST).

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
    assert!(note.contains("1 tRPC mount route"), "{note}");
    // The sentinel's `?` method is stripped for display — the note shows the path, never `? /api/...`.
    assert!(note.contains("(/api/trpc/{})"), "{note}");
    assert!(
        !note.contains("? /api"),
        "sentinel method must not leak into the note: {note}"
    );
    assert!(note.contains("1 tRPC edge"), "{note}");
    assert!(note.contains("tRPC transport"), "{note}");
    // The verb-unknown tRPC mount is transport (understood via the trpc edge), so it is NOT also disclosed
    // as an unknown-verb-route "inject the method" candidate.
    let unknown_verb = find(&out.cross_layer_findings, "cross-layer/unknown-verb-route");
    assert!(
        unknown_verb
            .iter()
            .all(|f| !f.message.contains("/api/trpc/")),
        "expected the tRPC mount suppressed from unknown-verb-route (transport): {:?}",
        unknown_verb
    );
}

#[test]
fn trpc_mount_route_with_zero_trpc_edges_is_disclosed_as_unknown_verb() {
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

    // Zero trpc edges: the serve-all mount is a verb-unknown route (its handler names no method), not a
    // dead route. A sentinel is never an unconsumed dead-route candidate — it surfaces as
    // `cross-layer/unknown-verb-route` (path served, method unknown, inject to confirm).
    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        unconsumed.iter().all(|f| !f.message.contains("/api/trpc/")),
        "a verb-unknown sentinel is never an unconsumed dead route: {:?}",
        unconsumed
    );
    let unknown_verb = find(&out.cross_layer_findings, "cross-layer/unknown-verb-route");
    assert!(
        unknown_verb
            .iter()
            .any(|f| f.message.contains("/api/trpc/{}")),
        "expected the no-edge tRPC-segment mount disclosed as unknown-verb-route: {:?}",
        unknown_verb
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
/// tree's real tRPC setup). `file_routes`'s content-blind convention scan still emits a verb-unknown
/// sentinel provide for it (any serve-all `pages/api/**` default-export file does), so the
/// `is_trpc_mount_route_path` shape match still fires — the per-tree edge-participation gate is the only
/// thing standing between this coincidence and a wrongful suppression.
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

    let source_of = |f: &&zzop_core::Finding| {
        f.data
            .as_ref()
            .and_then(|d| d.get("source"))
            .and_then(|s| s.as_str())
            .map(str::to_string)
    };
    let unknown_verb = find(&out.cross_layer_findings, "cross-layer/unknown-verb-route");
    // No verb-unknown sentinel is ever an unconsumed dead route (either tree).
    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        unconsumed.iter().all(|f| !f.message.contains("/api/trpc/")),
        "no verb-unknown sentinel is an unconsumed dead route: {:?}",
        unconsumed
    );

    // `web`'s own mount (a real tRPC transport) is suppressed from the disclosure — its trpc edge covers it.
    assert!(
        unknown_verb
            .iter()
            .all(|f| source_of(f).as_deref() != Some("web")),
        "expected web's tRPC mount suppressed from the unknown-verb disclosure (transport): {:?}",
        unknown_verb
    );

    // `other`'s coincidental trpc-segment route is NOT suppressed — zero tRPC edges of its own — so it is
    // disclosed as an honest verb-unknown route (the per-tree gate holds against tree `web`'s edges).
    assert!(
        unknown_verb.iter().any(|f| source_of(f).as_deref() == Some("other")
            && f.message.contains("/api/trpc/{}")),
        "expected tree `other`'s coincidental route disclosed as unknown-verb-route, not suppressed by \
         tree `web`'s edges: {:?}",
        unknown_verb
    );

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

// --- Deliberate error case: verb-unknown route end-to-end (1b) ---

#[test]
fn verb_unknown_route_is_disclosed_and_suppresses_a_would_be_false_finding() {
    // A serve-all `pages/api` handler (names no method literal) is a verb-unknown route. A FE consuming a
    // SPECIFIC method on that path must NOT be reported unprovided/near-miss (the path IS served, only the
    // verb is unknown), the route must NOT be an unconsumed dead route, and NO fabricated GET/POST — nor the
    // internal `?` sentinel — may appear anywhere. Only the honest `cross-layer/unknown-verb-route` fires.
    let api = TempDir::new("zzop-engine-xlf-verbunknown-api");
    api.write(
        "pages/api/widgets.ts",
        "export default function handler(req, res) {}\n",
    );
    let fe = TempDir::new("zzop-engine-xlf-verbunknown-fe");
    fe.write(
        "app.ts",
        "import axios from 'axios';\naxios.delete('/api/widgets');\n",
    );
    let trees = vec![
        (api.path().to_path_buf(), config("api")),
        (fe.path().to_path_buf(), config("fe")),
    ];
    let out = analyze_trees(&trees);

    let unknown_verb = find(&out.cross_layer_findings, "cross-layer/unknown-verb-route");
    assert!(
        unknown_verb
            .iter()
            .any(|f| f.message.contains("/api/widgets")),
        "expected /api/widgets disclosed as unknown-verb-route: {:?}",
        unknown_verb
    );
    for f in &out.cross_layer_findings {
        assert!(
            !(f.message.contains("GET /api/widgets") || f.message.contains("POST /api/widgets")),
            "no fabricated verb may appear for a verb-unknown route: {}",
            f.message
        );
        assert!(
            !f.message.contains("? /api/widgets"),
            "sentinel `?` must not leak into a finding: {}",
            f.message
        );
    }
    for rule in [
        "cross-layer/unprovided-mutation-call",
        "cross-layer/route-near-miss",
        "cross-layer/path-near-miss",
        "cross-layer/unconsumed-endpoint",
    ] {
        let hits = find(&out.cross_layer_findings, rule);
        assert!(
            hits.iter().all(|f| !f.message.contains("/api/widgets")),
            "a verb-unknown served path must suppress {rule} on /api/widgets: {:?}",
            hits
        );
    }
}

// --- Deliberate error case: unconfigured deployment-prefix blind spot ---

#[test]
fn unconfigured_deployment_prefix_surfaces_a_near_miss_naming_the_topology_remedy() {
    // A gateway/ingress adds an `/api` prefix at deploy time that neither repo's source carries. Without a
    // `mountedAt`/`mounts` injection zzop cannot see it, so the FE `/api/users` call and the BE `/users`
    // route land one prefix apart — a route-near-miss. The finding must name deployment topology as a
    // possible cause and point at the `mounts` remedy (the honest "verify against your topology" caveat).
    let be = TempDir::new("zzop-engine-xlf-deploy-be");
    be.write(
        "routes.ts",
        "const app = new Hono();\napp.get(\"/users\", handler);\n",
    );
    let fe = TempDir::new("zzop-engine-xlf-deploy-fe");
    fe.write(
        "app.ts",
        "import axios from 'axios';\naxios.get('/api/users');\n",
    );
    let trees = vec![
        (be.path().to_path_buf(), config("be")),
        (fe.path().to_path_buf(), config("fe")),
    ];
    let out = analyze_trees(&trees);

    let near_miss = find(&out.cross_layer_findings, "cross-layer/route-near-miss");
    let hit = near_miss
        .iter()
        .find(|f| f.message.contains("/api/users"))
        .unwrap_or_else(|| {
            panic!(
                "expected a route-near-miss for the unconfigured /api prefix: {:?}",
                near_miss
            )
        });
    assert!(
        hit.message.contains("deployment topology") && hit.message.contains("mounts"),
        "near-miss must name the deployment-topology remedy: {}",
        hit.message
    );
}

// --- Field test: a user AI injects the dynamic deployment prefix, and the join RESOLVES ---

#[test]
fn injecting_the_deployment_prefix_via_mounts_resolves_the_near_miss_into_an_exact_edge() {
    // The remedy path of the blind-spot test above: a user (or their AI) knows the gateway mounts the BE
    // tree at `/api` — a fact no source file carries — and injects it via `mounts`. zzop then prepends the
    // topology prefix to the BE provide's key, so the FE `/api/users` call and the (now-rewritten) BE
    // `/api/users` route join as an EXACT edge, and the route-near-miss disappears entirely. This proves
    // the disclosure -> inject -> resolve loop closes, not just that the gap is named.
    let be = TempDir::new("zzop-engine-xlf-deploy-be-fix");
    be.write(
        "routes.ts",
        "const app = new Hono();\napp.get(\"/users\", handler);\n",
    );
    let fe = TempDir::new("zzop-engine-xlf-deploy-fe-fix");
    fe.write(
        "app.ts",
        "import axios from 'axios';\naxios.get('/api/users');\n",
    );
    // The injected dynamic fact: the whole BE tree is mounted at `/api` by the deployment gateway.
    let be_config = EngineConfig {
        source_id: "be".to_string(),
        mounts: vec![MountRule {
            dir: String::new(),
            at: "api".to_string(),
        }],
        ..EngineConfig::default()
    };
    let trees = vec![
        (be.path().to_path_buf(), be_config),
        (fe.path().to_path_buf(), config("fe")),
    ];
    let out = analyze_trees(&trees);

    // The join now resolves as an exact edge at the mounted key...
    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "GET /api/users"),
        "injecting the /api mount must make the call and route join as an exact edge: {:?}",
        out.cross_layer.edges
    );
    // ...and the near-miss the un-injected run disclosed is gone.
    let near_miss = find(&out.cross_layer_findings, "cross-layer/route-near-miss");
    assert!(
        !near_miss.iter().any(|f| f.message.contains("/api/users")),
        "the injected mount must eliminate the deployment-prefix near-miss: {near_miss:?}"
    );
}

// --- Severity calibration (mono-hub field review, first external v0.14.0 reviews) ---
//
// `cross-layer/unconsumed-mutation-endpoint` used to fire Warning unconditionally, even when the run's own
// consume side was mostly unresolved (83% of one tree's `http` consumes in the field case) — the
// highest-severity cross-layer finding was the least trustworthy. These pins lock in the fix: the rule
// downgrades to Info (naming the blind source + unresolved count) when the run has a blind consume-side
// source, and stays Warning otherwise.

/// Downgrade pin: a majority-unresolved (blind) source + an unconsumed write provide -> the
/// `unconsumed-mutation-endpoint` finding is `Severity::Info`, and its message names the blind source and
/// the unresolved count.
#[test]
fn unconsumed_mutation_endpoint_downgrades_to_info_when_the_run_has_a_blind_source() {
    let fe = blind_fe_tree();
    let be = write_only_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    // Sanity: `fe` is really blind per the shared predicate (self-reported by its own rule).
    let ratio = find(
        &out.cross_layer_findings,
        "cross-layer/unresolved-consume-ratio",
    );
    assert_eq!(
        ratio.len(),
        1,
        "expected fe to be majority-unresolved: {:?}",
        out.cross_layer_findings
    );

    let mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unconsumed-mutation-endpoint",
    );
    assert_eq!(mutation.len(), 1, "{:?}", mutation);
    assert_eq!(mutation[0].severity, zzop_core::Severity::Info);
    assert!(mutation[0].message.contains("POST /api/group"));
    assert!(
        mutation[0].message.contains("`fe`"),
        "expected the blind source to be named: {}",
        mutation[0].message
    );
    assert!(
        mutation[0].message.contains("3 unresolved"),
        "expected the quantified caveat: {}",
        mutation[0].message
    );
    let data = mutation[0].data.as_ref().unwrap();
    assert_eq!(data["unresolvedHttpConsumeCount"], 3);
}

/// No-blind pin: the same unconsumed write provide, but no blind source anywhere in the run (no other tree
/// at all, so `unresolved_consumes` is empty) -> stays `Severity::Warning` with the attack-surface wording.
#[test]
fn unconsumed_mutation_endpoint_stays_warning_when_the_run_has_no_blind_source() {
    let be = write_only_be_tree();
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);

    assert!(find(
        &out.cross_layer_findings,
        "cross-layer/unresolved-consume-ratio"
    )
    .is_empty());

    let mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unconsumed-mutation-endpoint",
    );
    assert_eq!(mutation.len(), 1, "{:?}", mutation);
    assert_eq!(mutation[0].severity, zzop_core::Severity::Warning);
    assert!(mutation[0].message.contains("standing attack surface"));
    assert!(!mutation[0].message.contains("severity here is reduced"));
}

/// Class-parity guard (the recurrence guard): given the same unconsumed write provide + the same unresolved
/// http consumes, BOTH `unconsumed-endpoint` and `unconsumed-mutation-endpoint` carry
/// `data.unresolvedHttpConsumeCount` with the SAME value — so a future edit can never let the warning-
/// severity variant silently drop the blind-spot disclosure its info sibling makes.
#[test]
fn unconsumed_endpoint_and_unconsumed_mutation_endpoint_disclose_the_same_unresolved_count() {
    let fe = blind_fe_tree();
    let be = write_only_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    let unconsumed_write = unconsumed
        .iter()
        .find(|f| f.message.contains("POST /api/group"))
        .unwrap_or_else(|| {
            panic!(
                "expected unconsumed-endpoint to also co-fire on the write route: {:?}",
                unconsumed
            )
        });

    let mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unconsumed-mutation-endpoint",
    );
    assert_eq!(mutation.len(), 1, "{:?}", mutation);

    let info_count = unconsumed_write.data.as_ref().unwrap()["unresolvedHttpConsumeCount"].clone();
    let warning_count = mutation[0].data.as_ref().unwrap()["unresolvedHttpConsumeCount"].clone();
    assert_eq!(info_count, warning_count);
    assert_eq!(info_count, serde_json::json!(3));
}

// --- Severity calibration, symmetric sibling (opus-reviewer class-extrapolation) ---
//
// `cross-layer/unprovided-mutation-call` used to fire Warning unconditionally, even when a framework-
// bearing tree in the run extracted almost no routes (the S2 framework-silence tripwire condition) — a
// confident "no provider anywhere" verdict is not warranted when the provide side itself is near-blind.
// These pins mirror `unconsumed_mutation_endpoint_downgrades_to_info_when_the_run_has_a_blind_source` above
// exactly, on the provide side: the rule downgrades to Info (naming the blind source) when the run has a
// provide-blind source, and stays Warning otherwise.

/// Downgrade pin: a provide-blind source (a tree importing `express` with fewer than `MIN_PROVIDES_FLOOR`
/// http provides) + an unmatched write consume in another tree -> the `unprovided-mutation-call` finding is
/// `Severity::Info`, and its message names the blind source.
#[test]
fn unprovided_mutation_call_downgrades_to_info_when_the_run_has_a_provide_blind_source() {
    let fe = unprovided_write_fe_tree();
    let be = provide_blind_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-express")),
    ];
    let out = analyze_trees(&trees);

    // Sanity: `be-express` is really provide-blind per the S2 tripwire (self-reported on its own tree's
    // warnings) — the same condition `provide_blind_sources` lifts into a reusable set.
    let (_, _, be_output) = out
        .trees
        .iter()
        .find(|(_, source, _)| source == "be-express")
        .expect("be-express tree present in output");
    assert!(
        be_output
            .warnings
            .iter()
            .any(|w| w.contains("server-framework package(s) imported")),
        "expected be-express to self-report S2 framework-silence: {:?}",
        be_output.warnings
    );

    let unprovided = find(
        &out.cross_layer_findings,
        "cross-layer/unprovided-mutation-call",
    );
    assert_eq!(unprovided.len(), 1, "{:?}", unprovided);
    assert_eq!(unprovided[0].severity, zzop_core::Severity::Info);
    assert!(unprovided[0].message.contains("POST /api/orders"));
    assert!(
        unprovided[0].message.contains("`be-express`"),
        "expected the blind source to be named: {}",
        unprovided[0].message
    );
    assert!(
        unprovided[0].message.contains("provider-side blind spot"),
        "{}",
        unprovided[0].message
    );
    let data = unprovided[0].data.as_ref().unwrap();
    assert_eq!(data["provideBlindSourceCount"], 1);
}

/// No-blind pin: the same unmatched write consume, but no source in the run imports a server framework at
/// all -> `cross-layer/unprovided-mutation-call` stays `Severity::Warning` with today's framing unchanged.
#[test]
fn unprovided_mutation_call_stays_warning_when_the_run_has_no_provide_blind_source() {
    let fe = unprovided_write_fe_tree();
    let be = write_only_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let unprovided = find(
        &out.cross_layer_findings,
        "cross-layer/unprovided-mutation-call",
    );
    assert_eq!(unprovided.len(), 1, "{:?}", unprovided);
    assert_eq!(unprovided[0].severity, zzop_core::Severity::Warning);
    assert!(!unprovided[0].message.contains("provider-side blind spot"));
    let data = unprovided[0].data.as_ref().unwrap();
    assert_eq!(data["provideBlindSourceCount"], 0);
}

/// Class-seal pin (recurrence guard): the SAME run drives both mutation rules' confident-zero downgrade
/// simultaneously — `unconsumed-mutation-endpoint` downgrades on its consume-blind source, and
/// `unprovided-mutation-call` downgrades on its provide-blind source — proving the symmetric invariant
/// holds together, not just individually. A future edit that fixes/touches one downgrade while silently
/// dropping the other would fail exactly this test, even if each rule's own dedicated pin above still
/// passed in isolation.
#[test]
fn both_mutation_rules_downgrade_together_when_their_respective_side_is_blind() {
    let fe = blind_fe_tree(); // consume-blind: majority-unresolved http consumes
    let write_fe = unprovided_write_fe_tree(); // drives unprovided-mutation-call's unmatched write
    let write_be = write_only_be_tree(); // unconsumed write route: drives unconsumed-mutation-endpoint
    let provide_blind_be = provide_blind_be_tree(); // provide-blind: framework import, near-zero provides
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (write_fe.path().to_path_buf(), config("write-fe")),
        (write_be.path().to_path_buf(), config("write-be")),
        (
            provide_blind_be.path().to_path_buf(),
            config("provide-blind-be"),
        ),
    ];
    let out = analyze_trees(&trees);

    let unconsumed_mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unconsumed-mutation-endpoint",
    );
    assert_eq!(unconsumed_mutation.len(), 1, "{:?}", unconsumed_mutation);
    assert_eq!(
        unconsumed_mutation[0].severity,
        zzop_core::Severity::Info,
        "unconsumed-mutation-endpoint must downgrade on its own consume-blind source: {:?}",
        unconsumed_mutation[0]
    );

    let unprovided_mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unprovided-mutation-call",
    );
    assert_eq!(unprovided_mutation.len(), 1, "{:?}", unprovided_mutation);
    assert_eq!(
        unprovided_mutation[0].severity,
        zzop_core::Severity::Info,
        "unprovided-mutation-call must downgrade on its own provide-blind source: {:?}",
        unprovided_mutation[0]
    );
}

/// Extraction-blindness caveat (item 3, blind-round-3 fix batch): a sibling tree that contributed ZERO
/// joinable io (0 provides AND 0 keyed consumes) means an "unconsumed" verdict elsewhere in the run may be
/// that sibling's own extraction blindness, not a genuinely dead endpoint — both `unconsumed-endpoint` and
/// `unconsumed-mutation-endpoint` must carry the shared caveat, naming the blind tree's source id.
#[test]
fn unconsumed_findings_carry_the_extraction_blindness_caveat_when_a_sibling_tree_has_zero_joinable_io(
) {
    let blind = zero_io_tree();
    let be = write_only_be_tree();
    let trees = vec![
        (blind.path().to_path_buf(), config("blind-fe")),
        (be.path().to_path_buf(), config("write-be")),
    ];
    let out = analyze_trees(&trees);

    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        !unconsumed.is_empty(),
        "expected an unconsumed-endpoint finding for POST /api/group"
    );
    for f in &unconsumed {
        assert!(
            f.message.contains("'blind-fe'")
                && f.message.contains("contributed no joinable io facts")
                && f.message
                    .contains("extraction blindness rather than a dead endpoint"),
            "expected the extraction-blindness caveat naming 'blind-fe', got: {}",
            f.message
        );
    }

    let unconsumed_mutation = find(
        &out.cross_layer_findings,
        "cross-layer/unconsumed-mutation-endpoint",
    );
    assert!(
        !unconsumed_mutation.is_empty(),
        "expected an unconsumed-mutation-endpoint finding for POST /api/group"
    );
    for f in &unconsumed_mutation {
        assert!(
            f.message.contains("'blind-fe'")
                && f.message.contains("contributed no joinable io facts"),
            "expected the extraction-blindness caveat naming 'blind-fe', got: {}",
            f.message
        );
    }
}

/// Control: when EVERY tree in the join contributed joinable io, `unconsumed-endpoint`/
/// `unconsumed-mutation-endpoint` findings must NOT carry the caveat — it is only for the specific
/// zero-joinable-io case, never a blanket addition.
#[test]
fn unconsumed_findings_carry_no_caveat_when_every_tree_contributes_joinable_io() {
    let fe = fe_tree();
    let be = write_only_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("write-be")),
    ];
    let out = analyze_trees(&trees);

    let unconsumed = find(&out.cross_layer_findings, "cross-layer/unconsumed-endpoint");
    assert!(
        !unconsumed.is_empty(),
        "expected an unconsumed-endpoint finding for POST /api/group"
    );
    for f in &unconsumed {
        assert!(
            !f.message.contains("contributed no joinable io facts"),
            "unexpected extraction-blindness caveat when every tree contributes io: {}",
            f.message
        );
    }
}
