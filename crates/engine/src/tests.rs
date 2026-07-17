//! End-to-end fixture-tree tests — a hand-rolled `TempDir` (same pattern as
//! `crates/core/src/pack_loader.rs` / `parser/parser-prisma/src/lib.rs`'s test modules; no `tempfile`
//! dependency in this workspace).
use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zzop_core::RulePackDef;

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

/// Loads the real `rules/dsl/be-security/be-security.json` from the repo, resolved from
/// `CARGO_MANIFEST_DIR` (`crates/engine` -> up two -> repo root -> `rules/dsl/...`), filtered to the
/// three Java security-concern rules (`sql-taint`/`weak-crypto`/`cmd-injection`) that moved into
/// `be-security` when the language-named `java-security` pack was dissolved (v0.15). Filtering keeps
/// this fixture a small, fully-`.java`-applicable pack (every rule fires only on the `.java` fixture
/// file), which the profiling/degradation tests below rely on.
fn be_security_java_pack() -> RulePackDef {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl/be-security/be-security.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let mut pack: RulePackDef = serde_json::from_str(&text).expect("parse be-security.json");
    pack.rules
        .retain(|r| matches!(r.id.as_str(), "sql-taint" | "weak-crypto" | "cmd-injection"));
    pack
}

/// Builds the shared fixture tree:
/// - `a.ts` <-> `b.ts`: a circular import pair.
/// - `c.ts`: imports a module that does not exist (dangling import — must not panic, must not resolve
///   to an edge).
/// - `db/schema.prisma`: a `User` model.
/// - `legacy/C.java`: a SQL-taint pattern the `be-security` pack's `sql-taint` line-scan rule matches.
/// - `generated/big.ts`: exceeds `size_cap` -> oversized lexical fallback.
/// - `broken.ts`: unbalanced braces -> structurally-broken lexical fallback.
fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-fixture");
    dir.write(
        "a.ts",
        "import { b } from './b';\nexport function a() { return b(); }\n",
    );
    dir.write(
        "b.ts",
        "import { a } from './a';\nexport function b() { return a(); }\n",
    );
    dir.write(
        "c.ts",
        "import { missing } from './does-not-exist';\nexport const c = missing;\n",
    );
    dir.write(
        "db/schema.prisma",
        "model User {\n  id String @id\n  email String @unique\n}\n",
    );
    dir.write(
        "legacy/C.java",
        "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\");\n  }\n}\n",
    );
    dir.write(
        "generated/big.ts",
        &"const filler = 'generated content line';\n".repeat(40),
    );
    dir.write("broken.ts", "function broken( {\n  return 1;\n");
    dir
}

fn config(size_cap: usize) -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        size_cap,
        packs: vec![be_security_java_pack()],
        ..EngineConfig::default()
    }
}

#[test]
fn circular_ts_import_pair_produces_a_circular_finding() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    let cycle = out.findings.iter().find(|f| f.rule_id == "circular");
    assert!(
        cycle.is_some(),
        "expected a circular finding, got: {:?}",
        out.findings
    );
    let cycle = cycle.unwrap();
    assert!(cycle.file == "a.ts" || cycle.file == "b.ts");
}

#[test]
fn be_security_java_line_scan_rules_fire_on_the_java_file() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "be-security/sql-taint");
    assert!(
        hit.is_some(),
        "expected a be-security/sql-taint finding, got: {:?}",
        out.findings
    );
    assert_eq!(hit.unwrap().file, "legacy/C.java");
}

#[test]
fn oversized_file_degrades_but_loc_is_still_counted() {
    let dir = fixture_tree();
    // Small cap so `generated/big.ts` (~1.5KB) is oversized, but every other fixture file is not.
    let out = analyze_tree(dir.path(), &config(500));
    assert!(out.degraded.contains(&"generated/big.ts".to_string()));
    let loc = out.ir.ir.loc.get("generated/big.ts").copied().unwrap_or(0);
    assert!(
        loc > 0,
        "oversized file's loc should still be lexically counted"
    );
    // A file under the cap must NOT be marked degraded.
    assert!(!out.degraded.contains(&"a.ts".to_string()));
}

#[test]
fn syntactically_broken_ts_file_degrades_without_panicking() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    assert!(out.degraded.contains(&"broken.ts".to_string()));
    let loc = out.ir.ir.loc.get("broken.ts").copied().unwrap_or(0);
    assert!(loc > 0);
}

#[test]
fn dangling_import_resolves_to_no_edge_without_panicking() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    let edges = out.ir.ir.dep.get("c.ts").cloned().unwrap_or_default();
    assert!(edges.is_empty());
}

#[test]
fn prisma_model_symbols_are_present_in_the_ir() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    let user = out
        .ir
        .ir
        .symbols
        .iter()
        .find(|s| s.name == "User" && s.file == "db/schema.prisma");
    assert!(
        user.is_some(),
        "expected a User model symbol, got: {:?}",
        out.ir.ir.symbols
    );
    assert!(user.unwrap().exported);
}

#[test]
fn file_count_covers_every_fixture_file() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    assert_eq!(out.file_count, 7); // a.ts, b.ts, c.ts, schema.prisma, C.java, big.ts, broken.ts
}

#[test]
fn skip_dirs_are_never_walked() {
    let dir = fixture_tree();
    dir.write("node_modules/vendor/index.ts", "export const x = 1;\n");
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    assert_eq!(out.file_count, 7); // vendor file under node_modules/ must not be counted
    assert!(!out.ir.ir.loc.contains_key("node_modules/vendor/index.ts"));
}

#[test]
fn yarn_dir_is_never_walked() {
    // `.yarn` (vendored Yarn Berry bundles) must be skipped the same way `node_modules` is.
    let dir = fixture_tree();
    dir.write(
        ".yarn/releases/yarn-4.0.0.cjs",
        "process.env.SOME_TOKEN; const x = 1;\n",
    );
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    assert_eq!(out.file_count, 7); // vendored file under .yarn/ must not be counted
    assert!(!out.ir.ir.loc.contains_key(".yarn/releases/yarn-4.0.0.cjs"));
}

#[test]
fn disabling_a_pack_removes_its_findings() {
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    cfg.rule_config
        .disabled_rules
        .push("be-security".to_string());
    let out = analyze_tree(dir.path(), &cfg);
    assert!(!out
        .findings
        .iter()
        .any(|f| f.rule_id.starts_with("be-security/")));
}

#[test]
fn disabling_circular_removes_the_circular_finding() {
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    cfg.rule_config.disabled_rules.push("circular".to_string());
    let out = analyze_tree(dir.path(), &cfg);
    assert!(!out.findings.iter().any(|f| f.rule_id == "circular"));
}

#[test]
fn dsl_finding_message_carries_the_config_disable_hint_for_its_own_id() {
    // D13①: every DSL finding's message must end with `zzop_core::disable_hint`'s fragment for that
    // finding's OWN `rule_id` — appended by `pipeline::findings::append_disable_hints` after the pack's
    // own suppress-marker sentence (already present in the raw pack message).
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "be-security/sql-taint")
        .expect("expected a be-security/sql-taint finding");
    let hint = zzop_core::disable_hint("be-security/sql-taint");
    assert!(
        hit.message.ends_with(&hint),
        "expected the DSL finding's message to end with disable_hint's fragment {hint:?}, got: {:?}",
        hit.message
    );
}

#[test]
fn rule_overrides_applied_lists_only_ids_that_actually_matched() {
    // D13③: a typo'd `disabled_rules` entry must appear in NEITHER list — only the existing
    // unknown-id diagnostic names it (covered elsewhere) — while a correct disable/remap shows up here
    // as the positive "this actually took effect" confirmation.
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    cfg.rule_config
        .disabled_rules
        .push("be-security/sql-taint".to_string());
    cfg.rule_config
        .disabled_rules
        .push("no-such-rule-typo".to_string());
    cfg.rule_config
        .severity_overrides
        .insert("circular".to_string(), zzop_core::Severity::Info);
    let out = analyze_tree(dir.path(), &cfg);
    let applied = out
        .rule_overrides_applied
        .expect("expected Some — disabled_rules/severity_overrides were both non-empty");
    assert_eq!(applied.disabled, vec!["be-security/sql-taint".to_string()]);
    assert_eq!(applied.severity_remapped, vec!["circular".to_string()]);
    assert!(!applied.disabled.contains(&"no-such-rule-typo".to_string()));
    assert!(!applied
        .severity_remapped
        .contains(&"no-such-rule-typo".to_string()));
}

#[test]
fn rule_overrides_applied_is_none_when_nothing_was_requested() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    assert!(out.rule_overrides_applied.is_none());
}

#[test]
fn two_runs_over_the_same_tree_are_byte_for_byte_identical() {
    let dir = fixture_tree();
    let cfg = config(500); // exercise the oversized path too
    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);
    assert_eq!(
        serde_json::to_value(&out1.ir).unwrap(),
        serde_json::to_value(&out2.ir).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&out1.findings).unwrap(),
        serde_json::to_value(&out2.findings).unwrap()
    );
    assert_eq!(out1.degraded, out2.degraded);
    assert_eq!(out1.file_count, out2.file_count);
}

// --- late consume resolution: cross-file constant indirection (crate::io's module doc / analyze::
// late_resolve_cross_file_consumes) ---

#[test]
fn cross_file_constant_indirection_resolves_via_late_consume_resolution() {
    let dir = TempDir::new("zzop-engine-late-resolve");
    dir.write(
        "ControlKey.ts",
        "export const ControlKey = { AUTHEN: { getUserInfo: '/api/auth/user' } };\n",
    );
    dir.write(
        "Ctx.tsx",
        "import { ControlKey } from './ControlKey';\naxios.get(ControlKey.AUTHEN.getUserInfo);\n",
    );
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "fixture".to_string(),
            ..EngineConfig::default()
        },
    );
    let io = out.ir.ir.io.expect("expected io facts");
    let consume = io
        .consumes
        .iter()
        .find(|c| c.file == "Ctx.tsx")
        .expect("expected a consume from Ctx.tsx");
    assert_eq!(
        consume.key.as_deref(),
        Some("GET /api/auth/user"),
        "cross-file constant indirection should now resolve at assembly time: {consume:?}"
    );
    // Provenance is kept, not cleared, on a late-resolved consume.
    assert_eq!(
        consume.raw.as_deref(),
        Some("ControlKey.AUTHEN.getUserInfo")
    );
}

#[test]
fn duplicate_const_key_across_two_files_resolves_to_the_lexicographically_first_file() {
    let dir = TempDir::new("zzop-engine-late-resolve-dup");
    // Both files declare the SAME dotted constant key with different values — "a-consts.ts" sorts
    // before "z-consts.ts", so its value must win regardless of file-walk/rayon scheduling order.
    dir.write("a-consts.ts", "export const K = { path: '/from/a' };\n");
    dir.write("z-consts.ts", "export const K = { path: '/from/z' };\n");
    dir.write("Ctx.tsx", "axios.get(K.path);\n");
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "fixture".to_string(),
            ..EngineConfig::default()
        },
    );
    let io = out.ir.ir.io.expect("expected io facts");
    let consume = io
        .consumes
        .iter()
        .find(|c| c.file == "Ctx.tsx")
        .expect("expected a consume from Ctx.tsx");
    assert_eq!(consume.key.as_deref(), Some("GET /from/a"));
}

// --- tRPC: assembly-time PROVIDE composition (analyze::compose_trpc_provides) joined to a client CONSUME
// (crate::io's TS branch / trpc_consume) ---

#[test]
fn trpc_router_composes_across_files_and_joins_to_a_client_consume() {
    let dir = TempDir::new("zzop-engine-trpc");
    // `viewer.ts`: the leaf procedure's own router fragment.
    dir.write(
        "viewer.ts",
        "export const viewerRouter = router({ me: publicProcedure.query(() => 1) });\n",
    );
    // `trpc.ts`: mounts `viewerRouter` (imported from another file) under the `viewer` key — the
    // cross-file `Ref` `compose_trpc_provides` must resolve via the same import-resolution machinery
    // the TS dep graph itself uses.
    dir.write(
        "trpc.ts",
        "import { viewerRouter } from './viewer';\nexport const appRouter = router({ viewer: viewerRouter });\n",
    );
    // `page.tsx`: a client bound from a `"trpc"`-named specifier (the import-specifier client-detection
    // route `trpc_consume` documents), calling the composed procedure.
    dir.write(
        "page.tsx",
        "import { client } from './trpc-client';\nclient.viewer.me.useQuery();\n",
    );
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "fixture".to_string(),
            ..EngineConfig::default()
        },
    );
    let io = out.ir.ir.io.expect("expected io facts");
    let provide = io
        .provides
        .iter()
        .find(|p| p.kind == "trpc" && p.key == "QUERY viewer.me")
        .unwrap_or_else(|| panic!("expected a trpc provide, got: {:?}", io.provides));
    assert_eq!(
        provide.file, "viewer.ts",
        "the composed provide must anchor on the leaf's own originating file, not the `Ref`'s"
    );
    let consume = io
        .consumes
        .iter()
        .find(|c| c.kind == "trpc" && c.key.as_deref() == Some("QUERY viewer.me"))
        .unwrap_or_else(|| panic!("expected a trpc consume, got: {:?}", io.consumes));
    assert_eq!(consume.file, "page.tsx");
}

// --- Positive pack-load confirmation (`AnalyzeOutput::packs_loaded`) ---

#[test]
fn packs_loaded_reports_every_pack_sorted_with_provenance_and_inline_default() {
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    // A second pack whose id sorts BEFORE `be-security` — output order must be id-sorted, not load
    // order. It gets an explicit `Dir` provenance entry; `be-security` gets none, so it must report
    // the documented `"inline"` default.
    let extra: RulePackDef = serde_json::from_str(
        r#"{"id":"aaa-extra","framework":"any","rules":[{"id":"r1","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.ts$","line_pattern":"NEVER_MATCHES"}}]}"#,
    )
    .unwrap();
    cfg.packs.push(extra);
    cfg.pack_sources
        .insert("aaa-extra".to_string(), PackSource::Dir);
    let out = analyze_tree(dir.path(), &cfg);

    let summary: Vec<(&str, usize, &str)> = out
        .packs_loaded
        .iter()
        .map(|p| (p.id.as_str(), p.rules, p.source.as_str()))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("aaa-extra", 1, "dir"),
            ("be-security", 3, "inline"), // no pack_sources entry -> inline default
        ],
        "packs_loaded must be sorted by id and carry per-pack provenance"
    );
}

#[test]
fn packs_loaded_counts_rules_as_loaded_even_when_the_pack_is_disabled() {
    // `disabled_rules` gating is the caller's own explicit config, not a load failure — the pack still
    // LOADED, so the positive confirmation must keep reporting it (with its full loaded rule count).
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    cfg.rule_config.disabled_rules = vec!["be-security".to_string()];
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id.starts_with("be-security/")),
        "the disabled pack must not fire"
    );
    assert_eq!(out.packs_loaded.len(), 1);
    assert_eq!(out.packs_loaded[0].id, "be-security");
    assert_eq!(out.packs_loaded[0].rules, 3);
}

#[test]
fn packs_loaded_reports_per_pack_files_in_scope_zero_vs_nonzero() {
    // The D16 per-pack applicability signal: a loaded pack whose rules' `file_pattern`s match no
    // analyzed file reports `files_in_scope: 0` (its zero findings mean "out of scope", not "clean"),
    // while a pack whose scope matches reports the exact matching-file count.
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    // A pack scoped to an extension the fixture tree does not contain at all.
    let out_of_scope: RulePackDef = serde_json::from_str(
        r#"{"id":"zz-python-only","framework":"any","rules":[{"id":"r1","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.py$","line_pattern":"NEVER_MATCHES"}}]}"#,
    )
    .unwrap();
    cfg.packs.push(out_of_scope);
    let out = analyze_tree(dir.path(), &cfg);

    let by_id = |id: &str| {
        out.packs_loaded
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("expected pack {id} in packs_loaded: {:?}", out.packs_loaded))
    };
    assert_eq!(
        by_id("zz-python-only").files_in_scope,
        0,
        "a pack whose scope matches no analyzed file must report filesInScope 0"
    );
    // `be-security`'s rules include a `(?i)\.java$`-scoped rule — exactly one fixture file
    // (`legacy/C.java`) is in scope, and the count is exact per-file, not extension-bucket-wide.
    assert!(
        by_id("be-security").files_in_scope >= 1,
        "the matching pack must report a nonzero in-scope file count, got: {:?}",
        out.packs_loaded
    );
}

#[test]
fn packs_loaded_is_empty_when_no_packs_are_configured() {
    // The empty vec IS the signal (the positive complement of the zero-packs warning) — never absent,
    // never a stand-in default.
    let dir = fixture_tree();
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "fixture".to_string(),
            ..EngineConfig::default()
        },
    );
    assert!(out.packs_loaded.is_empty());
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("no DSL rule packs loaded")),
        "the zero-packs warning and the empty packs_loaded must agree, got: {:?}",
        out.warnings
    );
}

#[test]
fn fetch_wrapper_tree_fires_the_builtin_fetch_lexical_self_report() {
    // The blind-live-fire shape: HTTP calls ride hand-rolled helpers over builtin `fetch` with
    // computed URLs — extraction keys (near-)none of them, and before S5 nothing said so (builtin
    // fetch has no import for the S4 http-client tripwire to anchor on).
    let dir = TempDir::new("zzop-engine-fetch-wrapper");
    dir.write(
        "src/http.ts",
        "const BASE = '/api';\n\
         export const get = (p: string) => fetch(BASE + p);\n\
         export const post = (p: string, b: unknown) => fetch(BASE + p, { method: 'POST', body: JSON.stringify(b) });\n\
         export const put = (p: string, b: unknown) => fetch(BASE + p, { method: 'PUT', body: JSON.stringify(b) });\n\
         export const patch = (p: string, b: unknown) => fetch(BASE + p, { method: 'PATCH', body: JSON.stringify(b) });\n\
         export const del = (p: string) => fetch(BASE + p, { method: 'DELETE' });\n",
    );
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "fe".to_string(),
            ..EngineConfig::default()
        },
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("builtin `fetch(` call site(s)")),
        "the fetch-lexical self-report must fire, got: {:?}",
        out.warnings
    );
}

#[test]
fn directly_extracted_literal_fetch_calls_do_not_fire_the_lexical_self_report() {
    // Every fetch call carries a literal URL, so each one extracts as a KEYED http consume — the
    // near-zero gate clears and the lexical census never fires (a healthy FE tree pays no warning
    // for using builtin fetch directly).
    let dir = TempDir::new("zzop-engine-fetch-literal");
    let body: String = (0..5)
        .map(|i| format!("export const c{i} = () => fetch('/api/things/{i}');\n"))
        .collect();
    dir.write("src/api.ts", &body);
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "fe".to_string(),
            ..EngineConfig::default()
        },
    );
    assert!(
        out.ir.ir.io.as_ref().is_some_and(|io| io
            .consumes
            .iter()
            .filter(|c| c.key.is_some())
            .count()
            >= 3),
        "fixture must actually extract keyed consumes for the gate to be meaningful"
    );
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("builtin `fetch(` call site(s)")),
        "a directly-extracted tree must stay silent, got: {:?}",
        out.warnings
    );
}
