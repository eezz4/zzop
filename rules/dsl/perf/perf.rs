//! Exercises `rules/dsl/perf/perf.json`'s `api-in-loop` method-scan rule end-to-end through
//! `zzop_engine::analyze_tree` against real swc-parsed TypeScript fixtures. A `// api-in-loop-ok` marker on
//! the finding's own line, or the line directly above, suppresses it via `RuleDef::suppress_marker`. Most
//! fixtures use a top-level function rather than a class method to avoid double-counting from overlapping
//! class/method spans; see `overlapping_class_and_method_spans_do_not_double_count` below for that case.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RulePackDef};
use zzop_engine::{analyze_tree, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent, no `tempfile` dependency).
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

/// Loads the real `rules/dsl/perf/perf.json` from the repo, filtered to just the `perf` pack.
fn perf_pack() -> RulePackDef {
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
        .find(|p| p.id == "perf")
        .expect("perf.json pack present")
}

/// Runs the fused engine over a single-file fixture tree, returning just this rule's findings.
fn scan(rel: &str, content: &str) -> Vec<Finding> {
    let dir = TempDir::new("zzop-perf-apiloop");
    dir.write(rel, content);
    let cfg = EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![perf_pack()],
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    out.findings
        .into_iter()
        .filter(|f| f.rule_id == "perf/api-in-loop")
        .collect()
}

fn snippet(f: &Finding) -> String {
    f.data.as_ref().unwrap()["snippet"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn fetch_inside_for_of_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    const r = await fetch(\"/api/\" + id);\n    console.log(r);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("fetch"));
}

#[test]
fn axios_get_inside_traditional_for_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const axios: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (let i = 0; i < ids.length; i++) {\n    await axios.get(\"/u/\" + ids[i]);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("axios.get"));
}

#[test]
fn fetch_inside_foreach_callback_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const items: any[];\nexport function f() {\n  items.forEach((it) => {\n    fetch(\"/track/\" + it.id);\n  });\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn single_fetch_outside_any_loop_is_not_flagged() {
    let f = scan(
        "svc.ts",
        "export async function f(id: string) {\n  return await fetch(\"/api/\" + id);\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn in_memory_map_get_inside_loop_is_not_a_network_call() {
    let f = scan(
        "svc.ts",
        "declare const cacheMap: Map<string, number>;\ndeclare const ids: string[];\nexport function f() {\n  for (const id of ids) {\n    const v = cacheMap.get(id);\n    console.log(v);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// A top-level-function variant of a `class S { async f() { ... this.httpService.get(...) } }` shape,
/// exercising the broadened receiver vocabulary (`httpService.get`) this rule targets.
#[test]
fn broadened_receiver_vocab_httpservice_get_inside_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  const httpService: any = null;\n  for (const id of ids) {\n    await httpService.get(\"/u/\" + id);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("httpService.get"));
}

#[test]
fn ofetch_bare_client_inside_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const ofetch: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    await ofetch(\"/api/\" + id);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

/// Exercises the class-method shape directly: the parser projects both a class symbol span and a nested
/// method sub-symbol span, overlapping. Without innermost-span priority this produces 2 findings; with it, 1.
#[test]
fn overlapping_class_and_method_spans_do_not_double_count() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nclass S {\n  httpService: any;\n  async f() {\n    for (const id of ids) {\n      await this.httpService.get(\"/u/\" + id);\n    }\n  }\n}\nexport { S };\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("httpService.get"));
}

#[test]
fn comment_inside_the_function_body_documenting_a_loop_plus_fetch_example_is_not_flagged() {
    // A comment merely documenting a loop/fetch example (never actually executed) must not fire, even
    // though it textually satisfies both patterns within the span.
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  // Example: for (const id of ids) { fetch(url + id); } -- superseded by batchFetch below\n  return batchFetch(ids);\n}\ndeclare function batchFetch(ids: string[]): unknown;\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn api_in_loop_ok_marker_above_the_call_whitelists_it() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    // api-in-loop-ok: bounded admin list, sequential by design\n    await fetch(\"/api/\" + id);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- test-path exclusion: a mock endpoint hit in a loop in a test harness is not a production risk ---

#[test]
fn fetch_inside_loop_in_a_tests_directory_is_not_flagged() {
    let f = scan(
        "__tests__/svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    const r = await fetch(\"/api/\" + id);\n    console.log(r);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- loop-token anchored to syntax, not a bare `\bdo\b`/`\bfor\b` word ---

#[test]
fn prose_string_literal_mentioning_do_is_not_a_loop() {
    // A bare `\bdo\b` word match would false-positive on ordinary prose like "logged in to do this".
    let f = scan(
        "svc.ts",
        "export async function f() {\n  const msg = \"You must be logged in to do this action\";\n  return fetch(\"/api/message\", { body: msg });\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn template_literal_containing_for_word_is_not_a_loop() {
    // A bare `\bfor\b` word match would false-positive on prose in a template literal like `for ${x} items`.
    let f = scan(
        "svc.ts",
        "declare const x: number;\nexport async function f() {\n  const msg = `waiting for ${x} items`;\n  return fetch(\"/api/status\", { body: msg });\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn real_for_loop_and_while_loop_and_do_while_still_flagged() {
    // The syntax-anchored loop pattern must still catch real `for (`/`while (`/`do {` constructs.
    let for_loop = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (let i = 0; i < ids.length; i++) {\n    await fetch(\"/api/\" + ids[i]);\n  }\n}\n",
    );
    assert_eq!(for_loop.len(), 1, "{for_loop:?}");

    let while_loop = scan(
        "svc2.ts",
        "declare const ids: string[];\nexport async function f() {\n  let i = 0;\n  while (i < ids.length) {\n    await fetch(\"/api/\" + ids[i]);\n    i++;\n  }\n}\n",
    );
    assert_eq!(while_loop.len(), 1, "{while_loop:?}");

    let do_while_loop = scan(
        "svc3.ts",
        "declare const ids: string[];\nexport async function f() {\n  let i = 0;\n  do {\n    await fetch(\"/api/\" + ids[i]);\n    i++;\n  } while (i < ids.length);\n}\n",
    );
    assert_eq!(do_while_loop.len(), 1, "{do_while_loop:?}");
}

// --- retry-shape veto ---

#[test]
fn fetch_in_a_bounded_retry_loop_is_not_flagged() {
    // A bounded retry loop around one call is not the N+1 this rule targets.
    let f = scan(
        "svc.ts",
        "export async function f(url: string) {\n  const maxRetries = 3;\n  for (let attempt = 0; attempt < maxRetries; attempt++) {\n    const r = await fetch(url);\n    if (r.ok) return r;\n  }\n  throw new Error(\"exhausted retries\");\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}
