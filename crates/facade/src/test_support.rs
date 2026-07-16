//! Shared test fixtures/helpers for this crate's unit-test modules (`*_tests.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(prefix: &str) -> Self {
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

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn write(&self, rel: &str, content: &str) {
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

pub(crate) fn cycle_fixture() -> TempDir {
    let dir = TempDir::new("zzop-facade-fixture");
    dir.write(
        "a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    dir.write(
        "b.ts",
        "import { a } from './a';\nexport function b() { return a(); }\n",
    );
    dir
}

pub(crate) fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A real git repo (same `git init`/`config`/`commit` pattern as
/// `crates/engine/tests/analyze_git.rs`'s `git_fixture_repo`) built to exercise every `HashMap`-typed
/// field reachable from `AnalyzeOutputView` in one fixture:
/// - `ir.dep` gets 2+ keys: `a.ts` imports both `b.ts` and `c.ts`, `b.ts` imports `c.ts`.
/// - `ir.loc` gets 3 keys (one per file).
/// - `a.ts`'s `tag_counts` gets 3 distinct tags (FEAT/FIX/DOCS) from 3 separately-tagged commits, so
///   sorting actually has something to do (a single-key map would trivially "sort").
pub(crate) fn cycle_and_git_fixture() -> TempDir {
    let dir = TempDir::new("zzop-facade-determinism-fixture");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);

    dir.write("c.ts", "export function c() { return 1; }\n");
    run_git(dir.path(), &["add", "c.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add c"]);

    dir.write(
        "b.ts",
        "import { c } from './c';\nexport function b() { return c(); }\n",
    );
    dir.write(
        "a.ts",
        "import { b } from './b';\nimport { c } from './c';\nexport function a() { return b() + c(); }\n",
    );
    run_git(dir.path(), &["add", "a.ts", "b.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add a and b"]);

    dir.write(
        "a.ts",
        "import { b } from './b';\nimport { c } from './c';\nexport function a() { return b() + c() + 1; }\n",
    );
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[FIX] correct a"]);

    dir.write(
        "a.ts",
        "// updated docs\nimport { b } from './b';\nimport { c } from './c';\nexport function a() { return b() + c() + 1; }\n",
    );
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "[DOCS] document a"]);

    dir
}

/// A one-rule DSL pack JSON matching `crates/core/src/pack_loader.rs`'s `valid_pack` shape — field
/// names are the DSL's own snake_case (packs are DSL-authored files, not part of this boundary's
/// camelCase JS-facing config contract). A `line-scan` rule flagging `line_pattern` inside any `.ts`
/// file.
pub(crate) fn dsl_pack_json(pack_id: &str, rule_id: &str, line_pattern: &str) -> String {
    format!(
        r#"{{
            "id": "{pack_id}",
            "framework": "any",
            "rules": [
                {{
                    "id": "{rule_id}",
                    "severity": "warning",
                    "message": "msg",
                    "matcher": {{
                        "type": "line-scan",
                        "file_pattern": "\\.ts$",
                        "line_pattern": "{line_pattern}"
                    }}
                }}
            ]
        }}"#
    )
}

/// A one-rule DSL pack JSON with a `symbol-scan` matcher — the envelope-path counterpart of
/// `dsl_pack_json`: envelope mode evaluates only SymbolScan/IoScan rules (an envelope carries no
/// source text for `line-scan` to see), so the envelope pack tests match on symbol names instead of
/// source lines.
pub(crate) fn symbol_scan_pack_json(pack_id: &str, rule_id: &str, name_pattern: &str) -> String {
    format!(
        r#"{{
            "id": "{pack_id}",
            "framework": "any",
            "rules": [
                {{
                    "id": "{rule_id}",
                    "severity": "warning",
                    "message": "msg",
                    "matcher": {{
                        "type": "symbol-scan",
                        "file_pattern": "\\.jsp$",
                        "name_pattern": "{name_pattern}"
                    }}
                }}
            ]
        }}"#
    )
}

/// A valid v1 envelope whose single file carries the given top-level symbol names — the envelope-mode
/// counterpart of `cycle_fixture` + a marker file, for `symbol_scan_pack_json` rules to fire against.
pub(crate) fn envelope_with_symbols(names: &[&str]) -> String {
    let symbols = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!(
                r#"{{"id": "legacy/a.jsp#{name}", "file": "legacy/a.jsp", "name": "{name}", "kind": "function", "line": {}, "exported": true}}"#,
                i + 1
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"{{
            "format": "zzop-normalized-ast",
            "version": 1,
            "parser": "test/1",
            "source": "legacy",
            "files": [
                {{"path": "legacy/a.jsp", "loc": 10, "symbols": [{symbols}]}}
            ]
        }}"#
    )
}

pub(crate) fn tiny_envelope_json() -> String {
    r#"{
        "format": "zzop-normalized-ast",
        "version": 1,
        "parser": "jsp-lexical/1",
        "source": "legacy",
        "files": [
            {
                "path": "legacy/UserController.jsp",
                "loc": 40,
                "io": {
                    "provides": [
                        {"kind": "http", "key": "GET /legacy/user.jsp", "file": "legacy/UserController.jsp", "line": 5}
                    ],
                    "consumes": []
                }
            }
        ]
    }"#
    .to_string()
}
