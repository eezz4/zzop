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

/// Loads the real `rules/dsl/security/security.json` from the repo, resolved from
/// `CARGO_MANIFEST_DIR` (`crates/engine` -> up two -> repo root -> `rules/dsl/...`), filtered to
/// three Java security-concern LINE-SCAN rules (`sql-string-concat`/`weak-cipher`/`cmd-injection`)
/// that trace back to the dissolved language-named `java-security` pack (v0.15). `weak-crypto` was
/// in this set until 2026-08-09, when its hash half became a six-language call-scan rule (10
/// extensions, no longer `.java`-only) and its cipher arms split out as `weak-cipher` — which is
/// what keeps this fixture a small, fully-`.java`-applicable pack (every rule's `file_pattern`
/// admits only the `.java` fixture file), the property the profiling/degradation tests below rely
/// on. Goes through `zzop_core::parse_dsl_pack` (not a raw `serde_json::from_str`) so this pack's
/// `${NAME}` fragment refs (its shared test-path `file_exclude_pattern`) resolve exactly like they
/// do at real load time.
fn security_java_pack() -> RulePackDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl/security/security.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let mut pack: RulePackDef = zzop_core::parse_dsl_pack(&text).expect("parse security.json");
    pack.rules.retain(|r| {
        matches!(
            r.id.as_str(),
            "sql-string-concat" | "weak-cipher" | "cmd-injection"
        )
    });
    pack
}

/// Builds the shared fixture tree:
/// - `a.ts` <-> `b.ts`: a circular import pair.
/// - `c.ts`: imports a module that does not exist (dangling import — must not panic, must not resolve
///   to an edge).
/// - `db/schema.prisma`: a `User` model.
/// - `legacy/C.java`: a SQL-taint pattern the `security` pack's `sql-string-concat` line-scan rule matches.
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
        packs: vec![security_java_pack()],
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
fn security_java_line_scan_rules_fire_on_the_java_file() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "security/sql-string-concat");
    assert!(
        hit.is_some(),
        "expected a security/sql-string-concat finding, got: {:?}",
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

/// Red step for the degraded-cause self-report: this tree really has TWO degraded files with two
/// DIFFERENT causes, and `coverage.degraded`/`output.degraded` state neither the cause nor the lever.
#[test]
fn degraded_files_self_report_their_count_and_cause_breakdown() {
    let dir = fixture_tree();
    // Cap chosen so `generated/big.ts` is oversized while `broken.ts` (a parse failure) stays under it —
    // both degrade paths live in one run, which is the only way the breakdown can be observed at all.
    let out = analyze_tree(dir.path(), &config(500));
    assert!(out.degraded.contains(&"generated/big.ts".to_string()));
    assert!(out.degraded.contains(&"broken.ts".to_string()));

    let report = out
        .warnings
        .iter()
        .find(|w| w.contains("got NO STRUCTURAL PROJECTION from"))
        .unwrap_or_else(|| {
            panic!(
                "no degraded-cause self-report among the run's warnings: {:?}",
                out.warnings
            )
        });
    // The count, both cause buckets by name, one example from each bucket, and the lever for each.
    assert!(report.contains("2 file(s)"), "{report}");
    assert!(report.contains("over the size cap"), "{report}");
    assert!(report.contains("generated/big.ts"), "{report}");
    assert!(report.contains("failed to parse"), "{report}");
    assert!(report.contains("broken.ts"), "{report}");
    assert!(report.contains("sizeCap"), "{report}");
    // Precision, not overclaim: line-scan rules DID run on both files.
    assert!(report.contains("line-scan"), "{report}");
    // Exactly one aggregate entry, never one per file.
    assert_eq!(
        out.warnings
            .iter()
            .filter(|w| w.contains("got NO STRUCTURAL PROJECTION from"))
            .count(),
        1
    );
}

/// The cause is DERIVED on the warm path (`pipeline::artifact::cached_degrade_cause`) rather than
/// stored in the cached slice, so the two runs agreeing is the whole load-bearing claim of that choice:
/// a warm run must not downgrade an oversized file to "failed to parse" just because the parser verdict
/// is the only thing the cache remembers.
#[test]
fn the_degraded_cause_report_is_identical_cold_and_warm() {
    let dir = fixture_tree();
    let cache_dir = TempDir::new("zzop-engine-degraded-cause-cache");
    let cfg = EngineConfig {
        cache_dir: Some(cache_dir.path().to_path_buf()),
        ..config(500)
    };
    let report = |out: &AnalyzeOutput| {
        out.warnings
            .iter()
            .find(|w| w.contains("got NO STRUCTURAL PROJECTION from"))
            .cloned()
            .unwrap_or_else(|| panic!("no degraded-cause report: {:?}", out.warnings))
    };

    let cold = analyze_tree(dir.path(), &cfg);
    let warm = analyze_tree(dir.path(), &cfg);
    let stats = warm
        .cache
        .as_ref()
        .expect("expected cache stats on warm run");
    assert_eq!(
        stats.hits, warm.file_count,
        "expected every file to hit on the warm rerun"
    );
    let cold_report = report(&cold);
    assert!(cold_report.contains("over the size cap"), "{cold_report}");
    assert!(cold_report.contains("failed to parse"), "{cold_report}");
    assert_eq!(cold_report, report(&warm));
}

/// A tree with no degraded file must not carry the report at all — a self-report that fires on a clean
/// run is the noise the aggregate convention exists to prevent.
#[test]
fn a_tree_with_no_degraded_file_gets_no_degraded_cause_report() {
    let dir = TempDir::new("zzop-engine-clean");
    dir.write("a.ts", "export const a = 1;\n");
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    assert!(out.degraded.is_empty());
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("got NO STRUCTURAL PROJECTION from")),
        "{:?}",
        out.warnings
    );
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
    cfg.rule_config.disabled_rules.push("security".to_string());
    let out = analyze_tree(dir.path(), &cfg);
    assert!(!out
        .findings
        .iter()
        .any(|f| f.rule_id.starts_with("security/")));
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
    // finding's OWN `rule_id`. `pipeline::findings::append_hints` appends TWO sentences and the ORDER is
    // what this test pins the tail of: the suppress-marker sentence first, the disable hint last. Both are
    // appended now — the marker sentence used to be written into the pack message by hand, and 110 packs
    // carried a byte-identical copy of it before the fold.
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "security/sql-string-concat")
        .expect("expected a security/sql-string-concat finding");
    let hint = zzop_core::disable_hint("security/sql-string-concat");
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
        .push("security/sql-string-concat".to_string());
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
    assert_eq!(
        applied.disabled,
        vec!["security/sql-string-concat".to_string()]
    );
    assert_eq!(applied.severity_remapped, vec!["circular".to_string()]);
    assert!(!applied.disabled.contains(&"no-such-rule-typo".to_string()));
    assert!(!applied
        .severity_remapped
        .contains(&"no-such-rule-typo".to_string()));
}

#[test]
fn rule_overrides_applied_confirms_an_honored_pack_allowlist() {
    // The v0.29.0 release-audit finding, pinned from both sides: `packs.only` suppresses strictly more
    // than `packs.disabled`, so a run that sets it and gets back no acknowledgement is a run whose
    // missing findings have no wire evidence at all. `packsLoaded` cannot stand in — it is a path-match
    // census and is byte-identical under either knob.
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    let a_loaded_pack = cfg
        .packs
        .first()
        .map(|p| p.id.clone())
        .expect("the fixture config loads at least one DSL pack");
    cfg.rule_config.only_packs.push(a_loaded_pack.clone());
    cfg.rule_config
        .only_packs
        .push("no-such-pack-typo".to_string());
    let out = analyze_tree(dir.path(), &cfg);
    let applied = out
        .rule_overrides_applied
        .expect("expected Some — only_packs alone must be enough to open the field");
    assert_eq!(applied.only, vec![a_loaded_pack]);
    assert!(applied.disabled.is_empty());
    assert!(applied.severity_remapped.is_empty());
}

#[test]
fn a_pack_allowlist_naming_no_loaded_pack_reports_an_empty_only_rather_than_nothing() {
    // The dangerous typo: `is_pack_enabled` then admits NO pack and every DSL finding disappears at
    // once, and there is no unknown-id diagnostic for this knob today. `Some` with an EMPTY `only` is
    // the whole signal — collapsing it to `None` would make it indistinguishable from "never set".
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    cfg.rule_config
        .only_packs
        .push("no-such-pack-typo".to_string());
    let out = analyze_tree(dir.path(), &cfg);
    let applied = out
        .rule_overrides_applied
        .expect("an all-typo allowlist WAS a request — it must still be confirmed");
    assert!(
        applied.only.is_empty(),
        "a typo names no loaded pack, so nothing was applied: {:?}",
        applied.only
    );
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

#[test]
fn trpc_leaf_imported_as_a_single_procedure_is_composed_not_dropped() {
    let dir = TempDir::new("zzop-engine-trpc-leaf-import");
    // `getUser.ts`: ONE procedure, exported on its own — no `router({...})` anywhere in the file. This
    // is the per-file-procedure layout (cal.com, many t3 apps) the router below mounts as a leaf.
    dir.write(
        "getUser.ts",
        "import { authedProcedure } from './trpc';\nexport const getUser = authedProcedure.input(z.object({})).query(({ ctx }) => ctx.user);\n",
    );
    // `_router.ts`: mounts the imported procedure as a single leaf under `getUser` (shorthand), next to
    // an inline sibling so a regression here can't be mistaken for "the whole router vanished".
    dir.write(
        "_router.ts",
        "import { getUser } from './getUser';\nexport const userRouter = router({ getUser, ping: publicProcedure.query(() => 1) });\n",
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
        .find(|p| p.kind == "trpc" && p.key == "QUERY getUser")
        .unwrap_or_else(|| panic!("expected a trpc provide, got: {:?}", io.provides));
    assert_eq!(
        provide.file, "getUser.ts",
        "the composed provide must anchor on the procedure's own file, not the mounting router's"
    );
    assert_eq!(provide.line, 2);
    assert!(
        io.provides
            .iter()
            .any(|p| p.kind == "trpc" && p.key == "QUERY ping"),
        "the inline sibling must survive"
    );
}

#[test]
fn trpc_leaf_mount_with_no_static_evidence_stays_silent() {
    let dir = TempDir::new("zzop-engine-trpc-leaf-import-neg");
    // The mounted ident IS defined in a resolvable local file, but not as anything statically
    // recognizable as a tRPC procedure — a re-exported opaque value. Nothing may be invented for it.
    dir.write("opaque.ts", "export const getUser = makeProcedure(cfg);\n");
    // ...and this one is mounted from an external package the resolver cannot reach at all.
    dir.write(
        "_router.ts",
        concat!(
            "import { getUser } from './opaque';\n",
            "import { billing } from '@acme/trpc-procedures';\n",
            "export const userRouter = router({ getUser, billing, ping: publicProcedure.query(() => 1) });\n"
        ),
    );
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            source_id: "fixture".to_string(),
            ..EngineConfig::default()
        },
    );
    let io = out.ir.ir.io.expect("expected io facts");
    let trpc: Vec<_> = io
        .provides
        .iter()
        .filter(|p| p.kind == "trpc")
        .map(|p| p.key.as_str())
        .collect();
    assert_eq!(
        trpc,
        vec!["QUERY ping"],
        "only the inline procedure has static evidence; the two leaf mounts must stay absent"
    );
}

// --- Positive pack-load confirmation (`AnalyzeOutput::packs_loaded`) ---

#[test]
fn packs_loaded_reports_every_pack_sorted_with_provenance_and_inline_default() {
    let dir = fixture_tree();
    let mut cfg = config(DEFAULT_SIZE_CAP);
    // A second pack whose id sorts BEFORE `security` — output order must be id-sorted, not load
    // order. It gets an explicit `Dir` provenance entry; `security` gets none, so it must report
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
            ("security", 3, "inline"), // no pack_sources entry -> inline default
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
    cfg.rule_config.disabled_rules = vec!["security".to_string()];
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id.starts_with("security/")),
        "the disabled pack must not fire"
    );
    assert_eq!(out.packs_loaded.len(), 1);
    assert_eq!(out.packs_loaded[0].id, "security");
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
    // `security`'s rules include a `(?i)\.java$`-scoped rule — exactly one fixture file
    // (`legacy/C.java`) is in scope, and the count is exact per-file, not extension-bucket-wide.
    assert!(
        by_id("security").files_in_scope >= 1,
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
    // Computed URLs the extractor keys (near-)none of — a template literal whose base is a runtime
    // param, so it stays unresolved (keyed 0) while still carrying an internal-relative literal the
    // S5 intent filter counts (a bare-var `fetch(BASE + p)` shape carries no literal and no longer
    // counts under the intent filter).
    dir.write(
        "src/http.ts",
        "export const get = (base: string, p: string) => fetch(`${base}${p}`);\n\
         export const post = (base: string, p: string, b: unknown) => fetch(`${base}${p}`, { method: 'POST', body: JSON.stringify(b) });\n\
         export const put = (base: string, p: string, b: unknown) => fetch(`${base}${p}`, { method: 'PUT', body: JSON.stringify(b) });\n\
         export const patch = (base: string, p: string, b: unknown) => fetch(`${base}${p}`, { method: 'PATCH', body: JSON.stringify(b) });\n\
         export const del = (base: string, p: string) => fetch(`${base}${p}`, { method: 'DELETE' });\n",
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

/// Every `.rs` file under this crate's `src/`, recursively — the haystack the gate-coverage pin below
/// scans. Resolved from `CARGO_MANIFEST_DIR` like `security_java_pack` above.
fn engine_src_lines() -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = fs::read_to_string(&path) {
                    out.extend(text.lines().map(str::to_string));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

/// Seals the invariant that every REGISTERED native analysis id is actually gated by a literal
/// `is_enabled(..., "<id>")` call somewhere in this crate — i.e. that `disabledRules` really disables it.
///
/// This is the invariant a `{rule_id, runner}` dispatch table was once proposed to protect, recorded as
/// long-term cleanup for the ~14 inline `if is_enabled` checks in `analyze::assemble::rules`,
/// `native_rules::callgraph` and `native_rules::schema_join`. The table was NOT built, for two reasons
/// worth keeping written down: (1) the runners have irreconcilable signatures — `circular(cycles)` vs
/// `dead_export_findings(8 args)` vs `unreachable` with 20 lines of entry-set construction and
/// `dead-candidates` with a generated-banner post-filter — so a uniform `runner` column would need a
/// god-context struct passed to every closure, adding indirection rather than removing it; and (2) the
/// one drift this class has ACTUALLY suffered (v0.24.0: the five `zzop-metrics` ids were accepted by
/// `disabledRules`, reported back as applied, and kept running) was a MISSING gate, which a table cannot
/// catch on its own — only a registry-vs-gate cross-check like this test can. So the check is the fix and
/// the table is not; a comment promising work nobody intends to do would be the worse residue.
///
/// Deliberately a source-text check, not a behavioral one: it proves the gate EXISTS for every id, which
/// is exactly the drift shape observed. It cannot prove the gate wraps the right call — that stays the
/// job of each analysis's own `disabling_X_removes_the_X_finding` test.
///
/// ONE family is gated by a DERIVED id instead of a literal, and is accounted for explicitly rather than
/// waved through: the 12 `schema/<label>` ids all flow through the same
/// `is_enabled(rule_config, &finding.rule_id)` filter in `pipeline::findings`, because writing 12 literal
/// gates would be 12 chances to forget one. [`DERIVED_SCHEMA_GATE`] is asserted to still exist in the
/// source, so deleting the derived gate turns this test red exactly as deleting a literal one would; the
/// behavioral half (that it drops the right findings) is
/// `crates/engine/tests/pack_prisma_schema.rs`'s `disabling_one_*_issue_id_drops_only_that_rule`.
const DERIVED_SCHEMA_GATE: &str = "is_enabled(rule_config, &finding.rule_id)";

#[test]
fn every_registered_native_analysis_id_has_an_is_enabled_gate_in_this_crate() {
    let mut registry = zzop_core::RuleRegistry::new();
    register_all_native(&mut registry);
    // Windowed over the joined source, not per LINE: `rustfmt` wraps a long-enough call across lines, so a
    // line-scoped search silently stops seeing the gate for any id long enough to trigger the wrap
    // (`cross-layer/untraced-client-import-no-visible-consume` did exactly that the moment it was renamed —
    // the gate was right there, one line below its own `is_enabled(`).
    let text = engine_src_lines().join("\n");
    let gate_windows: Vec<&str> = text
        .match_indices("is_enabled(")
        .map(|(i, _)| &text[i..(i + 240).min(text.len())])
        .collect();

    // Counted in the ONE file that owns both schema call sites, not across `engine_src_lines()` — this
    // test file quotes the same literal in its own constant and doc comment, which would inflate the count.
    let findings_rs =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pipeline/findings.rs"))
            .expect("crates/engine/src/pipeline/findings.rs must exist");
    let derived_gate_sites = findings_rs.matches(DERIVED_SCHEMA_GATE).count();
    assert_eq!(
        derived_gate_sites, 2,
        "the derived per-issue schema gate `{DERIVED_SCHEMA_GATE}` must exist at BOTH schema call sites \
         (`schema_findings`, `schema_usage_findings`) — found {derived_gate_sites}. If it moved, move this \
         constant with it; if it is gone, the 12 `schema/*` ids are silently ungated again"
    );
    let derived: std::collections::BTreeSet<String> =
        zzop_rules_schema::SCHEMA_STRUCTURAL_ISSUE_LABELS
            .iter()
            .chain(zzop_rules_schema::SCHEMA_USAGE_ISSUE_LABELS.iter())
            .map(|label| zzop_rules_schema::schema_issue_rule_id(label))
            .collect();

    let ungated: Vec<&str> = registry
        .ids()
        .iter()
        .map(String::as_str)
        .filter(|id| {
            let needle = format!("\"{id}\"");
            !gate_windows.iter().any(|w| w.contains(&needle)) && !derived.contains(*id)
        })
        .collect();
    assert!(
        ungated.is_empty(),
        "registered native analysis ids with no `is_enabled` gate in crates/engine/src: {ungated:?}"
    );
    assert!(
        registry.ids().len() >= 57,
        "registry shrank unexpectedly: {} ids",
        registry.ids().len()
    );
}
