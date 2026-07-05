//! Exercises `rules/dsl/security/security.json`'s `taint-flow` rule end-to-end via
//! `zpz_engine::analyze_tree` against real swc-parsed TypeScript/TSX fixtures. `taint-flow` is an
//! explicitly coarse approximation (source+sink co-occurrence within a method-scan span, no real
//! per-variable dataflow) — see the rule's `message` for the full list of precision limits.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_core::{load_dsl_packs, RulePackDef};
use zpz_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent).
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

/// Loads the real `rules/dsl/security/security.json` from the repo, filtered to just the `security` pack.
fn security_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "security")
        .expect("security pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "security-fixture".to_string(),
        packs: vec![security_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zpz_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("security/{rule}"))
        .collect()
}

#[test]
fn hono_req_json_into_eval_is_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "handler.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  const body = await c.req.json();\n  eval(body);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn hono_req_query_into_exec_is_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "exec.ts",
        "import type { Context } from \"hono\";\nimport { exec } from \"node:child_process\";\nexport const h = async (c: Context) => {\n  const cmd = c.req.query(\"cmd\");\n  exec(cmd);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn schema_parse_sanitizer_clears_the_finding() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "safe.ts",
        "import type { Context } from \"hono\";\ndeclare const schema: any;\nexport const h = async (c: Context) => {\n  const raw = await c.req.json();\n  const safe = schema.parse(raw);\n  eval(safe);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

#[test]
fn dangerously_set_inner_html_with_tainted_value_is_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "Comp.tsx",
        "import type { Context } from \"hono\";\nexport const renderFromReq = async (c: Context) => {\n  const data = await c.req.json();\n  return <div dangerouslySetInnerHTML={{ __html: data.html }}>x</div>;\n};\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn next_request_json_into_execute_raw_unsafe_is_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "route.ts",
        "declare const prisma: any;\nexport async function POST(request: Request) {\n  const body = await request.json();\n  await prisma.$executeRawUnsafe(body.sql);\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn express_req_query_into_eval_is_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "express.ts",
        "export function handler(req: any, res: any) {\n  const expr = req.query.expr;\n  eval(expr);\n  res.end();\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn search_params_get_into_dangerously_set_inner_html_is_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "Page.tsx",
        "export function Page({ searchParams }: { searchParams: URLSearchParams }) {\n  const html = searchParams.get(\"html\");\n  return <div dangerouslySetInnerHTML={{ __html: html }}>x</div>;\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "taint-flow").len(), 1, "{:?}", out.findings);
}

#[test]
fn taint_ok_marker_directly_above_the_sink_suppresses_the_finding() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "marked.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  const cmd = c.req.query(\"cmd\");\n  // taint-ok: admin only, internal tooling\n  eval(cmd);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

#[test]
fn plain_function_with_no_source_or_sink_is_not_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "plain.ts",
        "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

// --- skip_comment_lines + test-path exclusion ---

#[test]
fn source_and_sink_mentioned_only_in_a_comment_are_not_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "handler.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  // const body = await c.req.json(); eval(body); -- old handler, removed\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}

#[test]
fn source_into_sink_inside_a_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zpz-security");
    dir.write(
        "src/__tests__/handler.ts",
        "import type { Context } from \"hono\";\nexport const h = async (c: Context) => {\n  const body = await c.req.json();\n  eval(body);\n  return c.json({});\n};\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "taint-flow").is_empty(), "{:?}", out.findings);
}
