//! e2e coverage for the manual pathname-dispatch HTTP provide extractor
//! (`zzop_parser_typescript::extract_pathname_dispatch_provides`), wired into the engine's per-file
//! IO pass in `packages/engine/src/io.rs`. Fixture shapes mirror a real-world Cloudflare Workers
//! corpus: a `fetch`-handler entrypoint that delegates to a cross-file `dispatch` function which
//! receives `url: URL` as a typed PARAM (injected by the wrapper, not constructed locally), plus a
//! Durable Object class whose own `fetch` routes must NOT be surfaced as public HTTP provides (an
//! edge request can only reach a DO via `stub.fetch`, so emitting `kind:"http"` for it would
//! over-claim public surface — see the DO-veto section of the extractor's module doc).
//!
//! Test 1 drives a single BE tree end-to-end through `analyze_tree` and asserts on the assembled
//! `IoFacts::provides`. Test 2 adds an FE tree with a keyed root-relative `fetch` consume and drives
//! `analyze_trees`, asserting the cross-layer join actually fires (mirrors
//! `analyze_multi_tree.rs`'s `fe_fetch_joins_to_be_hono_route_across_trees`). Test 3 adds a second FE
//! tree whose consume is the real-world `${BASE_URL}/...` leading-interpolation template shape — since
//! `base-carrier-drop-v1` (parser-typescript's `egress.rs`), that shape KEYS the same way (the opaque
//! base is dropped, not valued) and joins across trees identically to the root-relative shape in test 2.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, analyze_trees, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (same pattern as `analyze_routes_hono.rs`/`analyze_multi_tree.rs`).
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

/// The liberation-shaped BE corpus: an `index.ts` entrypoint delegating to a cross-file
/// `handleRequest`, whose own `dispatch` receives `url: URL` as a typed param (the real-world
/// wrapper-injection shape the extractor's module doc calls out), plus a Durable Object class whose
/// `/apply` and `/get` routes must be vetoed entirely.
fn be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-pathname-dispatch-be");
    dir.write(
        "src/index.ts",
        concat!(
            "import { handleRequest } from \"./handleRequest\";\n",
            "export default {\n",
            "  async fetch(request: Request, env: Env): Promise<Response> {\n",
            "    return handleRequest(request, env);\n",
            "  },\n",
            "};\n"
        ),
    );
    dir.write(
        "src/handleRequest.ts",
        concat!(
            "export async function handleRequest(request: Request, env: Env): Promise<Response> {\n",
            "  return dispatch(request, env, new URL(request.url));\n",
            "}\n",
            "\n",
            "async function dispatch(request: Request, env: Env, url: URL): Promise<Response> {\n",
            "  const { pathname } = url;\n",
            "  const method = request.method;\n",
            "  if (pathname === \"/me/achievements\") {\n",
            "    if (method === \"GET\") {\n",
            "      return getAchievements(request, env);\n",
            "    }\n",
            "    if (method === \"POST\") {\n",
            "      return postAchievement(request, env);\n",
            "    }\n",
            "  }\n",
            "  return new Response(\"not_found\", { status: 404 });\n",
            "}\n"
        ),
    );
    dir.write(
        "src/PlayerDO.ts",
        concat!(
            "export class PlayerDO {\n",
            "  constructor(state: DurableObjectState, _env: unknown) {}\n",
            "\n",
            "  async fetch(request: Request): Promise<Response> {\n",
            "    const url = new URL(request.url);\n",
            "    if (url.pathname === \"/apply\" && request.method === \"POST\") {\n",
            "      return new Response(\"applied\");\n",
            "    }\n",
            "    if (url.pathname === \"/get\" && request.method === \"GET\") {\n",
            "      return new Response(\"got\");\n",
            "    }\n",
            "    return new Response(\"not_found\", { status: 404 });\n",
            "  }\n",
            "}\n"
        ),
    );
    dir
}

/// The FE tree half of the cross-tree join: a keyed root-relative `fetch` consume of the same
/// `/me/achievements` route.
fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-pathname-dispatch-fe");
    dir.write(
        "src/api.ts",
        concat!(
            "export async function getAchievements(token: string) {\n",
            "  const res = await fetch(\"/me/achievements\", { headers: { Authorization: `Bearer ${token}` } });\n",
            "  return res.json();\n",
            "}\n"
        ),
    );
    dir
}

#[test]
fn manual_dispatch_provides_join_verbs_with_do_fetch_vetoed() {
    let dir = be_tree();
    let out: AnalyzeOutput = analyze_tree(dir.path(), &config("routes-pathname-dispatch-fixture"));

    let provides = &out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected aggregated io facts")
        .provides;

    let achievement_provides: Vec<_> = provides
        .iter()
        .filter(|p| p.key == "GET /me/achievements" || p.key == "POST /me/achievements")
        .collect();
    let mut keys: Vec<&str> = achievement_provides
        .iter()
        .map(|p| p.key.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["GET /me/achievements", "POST /me/achievements"],
        "expected both dispatch verbs as provides: {:?}",
        provides
    );
    assert!(
        achievement_provides.iter().all(|p| p.kind == "http"),
        "{:?}",
        achievement_provides
    );
    assert!(
        achievement_provides
            .iter()
            .all(|p| p.file == "src/handleRequest.ts"),
        "expected the dispatch function's own file as the provide anchor: {:?}",
        achievement_provides
    );
    assert!(
        achievement_provides
            .iter()
            .all(|p| p.symbol.as_deref() == Some("dispatch")),
        "expected the enclosing dispatch function as the provide symbol: {:?}",
        achievement_provides
    );

    // Durable Object veto: PlayerDO's own /apply and /get routes are reachable only via
    // `stub.fetch`, never as a public HTTP surface, so neither key may appear anywhere.
    assert!(
        provides
            .iter()
            .all(|p| !p.key.contains("/apply") && !p.key.contains("/get")),
        "DO-owned routes must be vetoed entirely: {:?}",
        provides
    );
}

/// The FE tree half of the base-carrier-head-drop join: a `BASE_URL`-prefixed template-literal `fetch`
/// consume of the same `/me/achievements` route — the real liberation-shaped call site
/// (`base-carrier-drop-v1`) that used to dead-end unresolved because of the opaque `${BASE_URL}` head.
fn fe_tree_template_base_carrier() -> TempDir {
    let dir = TempDir::new("zzop-engine-pathname-dispatch-fe-base-carrier");
    dir.write(
        "src/api.ts",
        concat!(
            "const BASE_URL = process.env.NEXT_PUBLIC_BE_URL ?? \"http://localhost:8790\";\n",
            "export async function getAchievements(token: string) {\n",
            "  const res = await fetch(`${BASE_URL}/me/achievements`, { headers: { Authorization: `Bearer ${token}` } });\n",
            "  return res.json();\n",
            "}\n"
        ),
    );
    dir
}

#[test]
fn fe_fetch_joins_to_be_manual_dispatch_route_across_trees() {
    let fe = fe_tree();
    let be = be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http" && e.key == "GET /me/achievements")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected exactly one cross-layer http edge for GET /me/achievements, got: {:?}",
        out.cross_layer.edges
    );
    let edge = http_edges[0];
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/api.ts");
    assert_eq!(edge.to.source, "be");
    assert_eq!(edge.to.file, "src/handleRequest.ts");
    assert!(edge.cross_source, "FE and BE are different sources");
}

#[test]
fn fe_template_base_carrier_joins_be_manual_dispatch() {
    let fe = fe_tree_template_base_carrier();
    let be = be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http" && e.key == "GET /me/achievements")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected exactly one cross-layer http edge for GET /me/achievements, got: {:?}",
        out.cross_layer.edges
    );
    let edge = http_edges[0];
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/api.ts");
    assert_eq!(edge.to.source, "be");
    assert_eq!(edge.to.file, "src/handleRequest.ts");
    assert!(edge.cross_source, "FE and BE are different sources");
}
