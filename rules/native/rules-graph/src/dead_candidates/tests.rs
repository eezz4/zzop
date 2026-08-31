//! Exercises `find_dead_candidates`: zero-fan-in low-change-count files are flagged; files with
//! incoming edges, high change counts, or entry/test patterns are excluded; non-source files never
//! linked in the dep graph are excluded from candidacy entirely; a non-TS file that does participate in
//! the dep graph (envelope-ingested source, e.g. `.jsp`) is still a candidate on zero fan-in.
use super::*;
use std::collections::HashMap;

fn n(path: &str, fan_in: u32, change_count: u32) -> FileNode {
    FileNode {
        id: path.into(),
        path: path.into(),
        change_count,
        churn: 0,
        last_modified: Some("2026-01-01".into()),
        author_count: 1,
        loc: 50,
        tag_counts: HashMap::new(),
        fan_in,
        fan_out: 0,
        total_connections: fan_in,
        risk_score: 0.0,
        ..Default::default()
    }
}

fn empty_dep() -> DepGraph {
    DepGraph::new()
}

fn no_extra_entries() -> HashSet<String> {
    HashSet::new()
}

#[test]
fn fan_in_zero_low_change_count_is_candidate() {
    let r = find_dead_candidates(
        &[n("features/x/Orphan.tsx", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/Orphan.tsx"]);
}

#[test]
fn fan_in_positive_is_excluded() {
    let r = find_dead_candidates(
        &[n("x.tsx", 2, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty());
}

#[test]
fn public_js_asset_with_bumped_fan_in_is_not_a_candidate() {
    // A `public/`-served `.js` worklet is dead-candidate-ELIGIBLE (a TS-dispatch extension), so at
    // fan_in 0 it is the exact FP mono-hub's `rnnoiseWorklet.js` was. The engine's asset-ref fan-in bump
    // (`merge_asset_ref_fan_in`, from an `audioWorklet.addModule("/…")` string) is the SOLE lever that
    // clears it — no entry/tool/exclude pattern independently exempts a `public/*.js` file, so this pins
    // that the bump alone is both necessary (fan_in 0 still flags) and sufficient (fan_in 1 clears).
    let asset = "ai-hub-fe/public/noise-suppressor/rnnoiseWorklet.js";
    let flagged = find_dead_candidates(
        &[n(asset, 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert_eq!(
        flagged.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        vec![asset],
        "public/*.js worklet at fan_in 0 must still be a candidate (the FP the bump fixes)"
    );
    let cleared = find_dead_candidates(
        &[n(asset, 1, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(
        cleared.is_empty(),
        "a bumped fan_in must clear the public/*.js worklet from dead-candidates"
    );
}

#[test]
fn entry_patterns_are_excluded() {
    let r = find_dead_candidates(
        &[
            n("pages/HomePage.tsx", 0, 1),
            n("App.tsx", 0, 1),
            n("features/x/index.ts", 0, 1),
            // Vite React/JS entries — `main.jsx`/`App.jsx`/`main.js` are entry points too, loaded by
            // index.html/the bundler, never imported (the `.jsx`/`.js` forms the pattern missed before).
            n("src/main.jsx", 0, 1),
            n("src/App.jsx", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty());
}

#[test]
fn nextjs_app_router_convention_files_are_not_dead_candidates() {
    // Framework-loaded by filename (never imported) so fan_in == 0 is expected — must not be flagged.
    // A genuinely orphaned sibling in the same set still fires.
    let r = find_dead_candidates(
        &[
            n("app/(lang)/[lang]/about/page.tsx", 0, 1),
            n("app/dashboard/layout.tsx", 0, 1),
            n("app/api/users/route.ts", 0, 1),
            n("app/not-found.tsx", 0, 1),
            n("app/sitemap.ts", 0, 1),
            n("app/robots.ts", 0, 1),
            n("features/x/old-helper.ts", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/old-helper.ts"], "{r:?}");
}

#[test]
fn test_files_are_excluded() {
    let r = find_dead_candidates(
        &[n("features/x/__test__/x.test.ts", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty());
}

#[test]
fn test_runner_directory_and_setup_files_are_excluded() {
    // Files under a test-runner DIRECTORY (`playwright/`, `e2e/`) are runner-loaded, not imported —
    // covered by the shared `zzop_core::is_test_file` SSOT. Config-loaded setup entries (`setup-tests.ts`
    // via vitest `setupFiles`) are `is_tool_entry_file`. Both had zero fan-in and were false dead
    // candidates before. A genuinely orphaned sibling still fires.
    let r = find_dead_candidates(
        &[
            n("playwright/global.setup.ts", 0, 1),
            n("playwright/utils/test-decorators.ts", 0, 1),
            n("src/setup-tests.ts", 0, 1),
            n("features/x/old-helper.ts", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/old-helper.ts"], "{r:?}");
}

#[test]
fn high_change_count_is_excluded() {
    let r = find_dead_candidates(
        &[n("features/x/Hot.ts", 0, 10)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty());
}

#[test]
fn non_source_files_never_in_the_dep_graph_are_never_candidates() {
    // These extensions never participate in the dep graph and aren't TS-dispatch extensions, so
    // fan_in == 0 on them is not a "no importers" signal — it's just "untracked".
    let r = find_dead_candidates(
        &[
            n("data/config.json", 0, 1),
            n("styles/app.css", 0, 1),
            n("docs/README.md", 0, 1),
            n("assets/logo.svg", 0, 1),
            n("schema.prisma", 0, 1),
            n("Service.java", 0, 1),
            // `.cs` is language-excluded like `.java`: ASP.NET controllers / MediatR handlers /
            // `Program.cs` are framework-invoked with no `using`-import edge, so fan_in == 0 is not dead
            // evidence.
            n("Features/Favorites/FavoritesController.cs", 0, 1),
            n("Program.cs", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn source_file_dead_still_fires_alongside_excluded_non_source_files() {
    // Non-source files with equally zero fan-in don't suppress a genuine dead file in the same set.
    let r = find_dead_candidates(
        &[
            n("features/x/Orphan.tsx", 0, 1),
            n("data/config.json", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/Orphan.tsx"]);
}

#[test]
fn finding_message_renders_single_braces_in_the_disable_hint() {
    // Regression: this message is a PLAIN string literal (not `format!`), so a `{{` written for
    // format-escaping renders literally as `{{` in user output — the 2026-07-10 dialect sweep
    // shipped exactly that. Pin the rendered form.
    let out = dead_candidate_findings(
        &[n("features/x/Orphan.tsx", 0, 1)],
        &empty_dep(),
        &no_extra_entries(),
    );
    assert_eq!(out.len(), 1);
    assert!(
        out[0]
            .message
            .contains("`rules: { \"dead-candidates\": \"off\" }`"),
        "{}",
        out[0].message
    );
    assert!(!out[0].message.contains("{{"), "{}", out[0].message);
}

#[test]
fn all_import_eligible_extensions_are_candidates_when_dead() {
    let nodes: Vec<FileNode> = [
        "a.ts", "b.tsx", "c.js", "d.jsx", "e.mjs", "f.cjs", "g.mts", "h.cts",
    ]
    .iter()
    .map(|p| n(p, 0, 1))
    .collect();
    let r = find_dead_candidates(&nodes, &empty_dep(), DEAD_MAX_CHANGES, &no_extra_entries());
    assert_eq!(
        r.len(),
        8,
        "expected all 8 import-eligible extensions to be candidates, got: {r:?}"
    );
}

#[test]
fn ts_extension_match_is_case_insensitive() {
    let r = find_dead_candidates(
        &[n("features/x/Orphan.TSX", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert_eq!(r.len(), 1, "{r:?}");
}

#[test]
fn non_ts_file_present_as_a_dep_key_with_zero_fan_in_is_a_candidate() {
    // Envelope-ingested source inserted as a `dep` key is a real graph node, so fan_in == 0 here is
    // real "no importers" signal, not "untracked".
    let mut dep = empty_dep();
    dep.insert("legacy/UserController.jsp".to_string(), Vec::new());
    let r = find_dead_candidates(
        &[n("legacy/UserController.jsp", 0, 1)],
        &dep,
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert_eq!(r.len(), 1, "{r:?}");
}

#[test]
fn non_ts_file_present_only_as_an_edge_target_with_zero_fan_in_is_still_evaluated() {
    // A file that appears only as a target in another file's edge list (never as its own `dep` key)
    // still participates in the graph — branch (a) checks both positions.
    let mut dep = empty_dep();
    dep.insert(
        "legacy/Controller.jsp".to_string(),
        vec!["legacy/util.jsp".to_string()],
    );
    // fan_in 1 means something imports it, so it correctly is NOT a candidate here.
    let r = find_dead_candidates(
        &[n("legacy/util.jsp", 1, 1)],
        &dep,
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty());
}

#[test]
fn non_ts_file_absent_from_the_dep_graph_entirely_is_never_a_candidate() {
    // Never appears in `dep` at all, so it doesn't participate in the graph fan_in was computed from —
    // fan_in == 0 here is "untracked", not "no importers".
    let r = find_dead_candidates(
        &[n("legacy/Orphaned.jsp", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty());
}

#[test]
fn ts_file_absent_from_the_dep_graph_is_still_a_candidate_via_extension_fallback() {
    // Never made it into `dep` at all, but still falls back to branch (b) — a `.ts` file missing from
    // the graph reads as an ingestion gap, not "outside the import graph".
    let r = find_dead_candidates(
        &[n("features/x/Isolated.ts", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert_eq!(r.len(), 1, "{r:?}");
}

#[test]
fn tool_entry_files_are_never_dead_candidates() {
    // These are all zero-fan-in because they're loaded by a tool, not imported by app code.
    let r = find_dead_candidates(
        &[
            n(".eslintrc.cjs", 0, 1),
            n(".prettierrc.cjs", 0, 1),
            n("vite.config.ts", 0, 1),
            n("vite-env.d.ts", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn genuinely_orphaned_source_file_still_fires_alongside_tool_entry_files() {
    let r = find_dead_candidates(
        &[
            n("vite.config.ts", 0, 1),
            n("features/x/old-helper.ts", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/old-helper.ts"]);
}

#[test]
fn python_file_with_zero_fan_in_is_never_a_dead_candidate_even_as_a_dep_graph_participant() {
    // F1: `.py`/`.pyi` participate in the `DepGraph` (a `dep`-map key here), but Python's
    // filename-convention loading (`main.py`, `manage.py`, `settings.py`, `conftest.py`, migrations,
    // `test_*.py`, ...) means fan_in == 0 is never "no importers" evidence for them — excluded from
    // eligibility entirely. A sibling `.ts` with equally zero fan-in still fires.
    let mut dep = empty_dep();
    dep.insert("app/main.py".to_string(), Vec::new());
    dep.insert("app/models/__init__.pyi".to_string(), Vec::new());
    let r = find_dead_candidates(
        &[
            n("app/main.py", 0, 1),
            n("app/models/__init__.pyi", 0, 1),
            n("features/x/Orphan.ts", 0, 1),
        ],
        &dep,
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/Orphan.ts"], "{r:?}");
}

#[test]
fn python_extension_match_is_case_insensitive() {
    let r = find_dead_candidates(
        &[n("app/Main.PY", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn rust_file_with_zero_fan_in_is_never_a_dead_candidate_even_as_a_dep_graph_participant() {
    // `.rs` participates in the `DepGraph` (a `dep`-map key here), but trait impls/derive expansion/
    // fully-qualified calls give a `pub` item real uses the import graph structurally cannot see, so
    // fan_in == 0 is never "no importers" evidence for it — excluded from eligibility entirely. A
    // sibling `.ts` with equally zero fan-in still fires.
    let mut dep = empty_dep();
    dep.insert("src/lib.rs".to_string(), Vec::new());
    dep.insert("src/handlers.rs".to_string(), Vec::new());
    let r = find_dead_candidates(
        &[
            n("src/lib.rs", 0, 1),
            n("src/handlers.rs", 0, 1),
            n("features/x/Orphan.ts", 0, 1),
        ],
        &dep,
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/Orphan.ts"], "{r:?}");
}

#[test]
fn rust_extension_match_is_case_insensitive() {
    let r = find_dead_candidates(
        &[n("src/Main.RS", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn go_file_with_zero_fan_in_is_never_a_dead_candidate_even_as_a_dep_graph_participant() {
    // `.go` participates in the `DepGraph` (a `dep`-map key here), but same-package files share every
    // top-level symbol with NO import statement between them at all, so fan_in == 0 is never "no
    // importers" evidence for it — excluded from eligibility entirely. A sibling `.ts` with equally zero
    // fan-in still fires.
    let mut dep = empty_dep();
    dep.insert("internal/db/db.go".to_string(), Vec::new());
    dep.insert("internal/db/helpers.go".to_string(), Vec::new());
    let r = find_dead_candidates(
        &[
            n("internal/db/db.go", 0, 1),
            n("internal/db/helpers.go", 0, 1),
            n("features/x/Orphan.ts", 0, 1),
        ],
        &dep,
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/Orphan.ts"], "{r:?}");
}

#[test]
fn go_extension_match_is_case_insensitive() {
    let r = find_dead_candidates(
        &[n("main.GO", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn java_file_with_zero_fan_in_is_never_a_dead_candidate_even_as_a_dep_graph_participant() {
    // `.java` now participates in the `DepGraph` (a `dep`-map key here, via `merge_java_dep_edges`), but
    // Java's own package-visibility model lets a same-package sibling use a type with NO import statement
    // pointing at it at all, so fan_in == 0 is never "no importers" evidence for it — excluded from
    // eligibility entirely, same exclusion class as `.rs`/`.go`. A sibling `.ts` with equally zero fan-in
    // still fires.
    let mut dep = empty_dep();
    dep.insert("com/example/a/A.java".to_string(), Vec::new());
    dep.insert("com/example/a/B.java".to_string(), Vec::new());
    let r = find_dead_candidates(
        &[
            n("com/example/a/A.java", 0, 1),
            n("com/example/a/B.java", 0, 1),
            n("features/x/Orphan.ts", 0, 1),
        ],
        &dep,
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/Orphan.ts"], "{r:?}");
}

#[test]
fn java_extension_match_is_case_insensitive() {
    let r = find_dead_candidates(
        &[n("Main.JAVA", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn package_json_referenced_file_is_never_a_dead_candidate() {
    // A path present in `extra_entries` (e.g. a package.json `main` target) is excluded, same as an
    // entry-pattern or tool-entry file; a genuinely orphaned file not in `extra_entries` still fires.
    let extra: HashSet<String> = ["src/cli.ts".to_string()].into_iter().collect();
    let r = find_dead_candidates(
        &[n("src/cli.ts", 0, 1), n("features/x/old-helper.ts", 0, 1)],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &extra,
    );
    let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["features/x/old-helper.ts"]);
}

// ---- Framework path-convention roots (module doc "Framework path conventions") ----------------
// MEASURED (2026-08-26, 13 trees): the whole population these four tests cover is 75 findings —
// cal.com 64 (62 under `apps/web/pages/**` + `example-apps/credential-sync/pages/**` +
// `packages/platform/examples/base/src/pages/**`, plus 2 `instrumentation*`) and astro 11 (five
// example/benchmark projects' `src/pages/**`). The control rows in each test are equally measured
// and must keep reporting.

#[test]
fn nextjs_pages_router_files_are_not_dead_candidates() {
    // The HARMFUL finding this gate exists for: `apps/web/pages/api/trpc/auth/[trpc].ts` is four
    // lines of `export default createNextApiHandler(authRouter)`, routed by Next.js from its PATH.
    // Deleting it 404s password change, email verification and 27 sibling tRPC namespaces.
    let r = find_dead_candidates(
        &[
            // The DECLARATION that makes `apps/web/` a Next.js app root. Exempt on its own account
            // (`is_tool_entry_file` carries `<name>.config.<ext>`), so it never appears in output.
            n("apps/web/next.config.ts", 0, 1),
            n("apps/web/pages/api/trpc/auth/[trpc].ts", 0, 1),
            n("apps/web/pages/api/auth/[...nextauth].ts", 0, 1),
            n("apps/web/pages/api/stripe/webhook.ts", 0, 1),
            n("apps/web/pages/_app.tsx", 0, 1),
            n("apps/web/pages/_document.tsx", 0, 1),
            n("apps/web/pages/router/embed.tsx", 0, 1),
            n("apps/web/instrumentation.ts", 0, 1),
            n("apps/web/instrumentation-client.ts", 0, 1),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn src_pages_router_under_a_next_app_root_is_covered_too() {
    // `packages/platform/examples/base/` holds its Pages Router under `src/` — 18 of cal.com's 63.
    let r = find_dead_candidates(
        &[
            n("packages/platform/examples/base/next.config.js", 0, 1),
            n("packages/platform/examples/base/src/pages/_app.tsx", 0, 1),
            n(
                "packages/platform/examples/base/src/pages/api/refresh.ts",
                0,
                1,
            ),
            n(
                "packages/platform/examples/base/src/pages/booking.tsx",
                0,
                1,
            ),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn astro_src_pages_under_an_astro_app_root_is_not_a_dead_candidate() {
    // Same mechanism, a second framework, so the gate is not cal.com-shaped: MEASURED on
    // `corpus/frameworks/astro`, where five example/benchmark projects each carry an
    // `astro.config.*` beside a routed `src/pages/**` — 11 findings.
    let r = find_dead_candidates(
        &[
            n("examples/ssr/astro.config.mjs", 0, 1),
            n("examples/ssr/src/pages/api/cart.ts", 0, 1),
            n("examples/ssr/src/pages/login.form.ts", 0, 1),
            n(
                "benchmark/static-projects/build-server/astro.config.js",
                0,
                1,
            ),
            n(
                "benchmark/static-projects/build-server/src/pages/api/stats.json.js",
                0,
                1,
            ),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    assert!(r.is_empty(), "expected no candidates, got: {r:?}");
}

#[test]
fn a_pages_directory_with_no_framework_config_beside_it_still_reports() {
    // NEGATIVE CANARY, and the whole reason the gate is anchored on a declaration rather than on the
    // directory name. Every row is a MEASURED corpus path that a bare `(^|/)pages/` — or a bare
    // `plugins/`/`middleware/`/`composables/` — would have erased:
    //   - cal.com `packages/app-store/paypal/pages/setup/_getStaticProps.tsx` has ZERO references
    //     repo-wide and no `next.config.*` above it. It is a genuinely dead file.
    //   - typeorm's `docs/` is Docusaurus (`docusaurus.config.ts`), not Next or Astro.
    //   - grafana `plugins/`, nest `plugins/`+`middleware/`, nocodb `composables/` are ordinary
    //     directories that happen to be named after some other framework's convention (21+6+6
    //     findings between them).
    let paths = [
        "packages/app-store/paypal/pages/setup/_getStaticProps.tsx",
        "docs/src/pages/maintainers.tsx",
        "e2e-playwright/test-plugins/app/plugins/example1-app/module.tsx",
        "sample/01-cats-app/src/common/middleware/logger.middleware.ts",
        "packages/nc-gui/composables/useServerConfig.ts",
        "apps/web/instrumentation.ts",
    ];
    let nodes: Vec<FileNode> = paths.iter().map(|p| n(p, 0, 1)).collect();
    let r = find_dead_candidates(&nodes, &empty_dep(), DEAD_MAX_CHANGES, &no_extra_entries());
    let got: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    let mut want: Vec<&str> = paths.to_vec();
    want.sort_unstable();
    let mut got_sorted = got.clone();
    got_sorted.sort_unstable();
    assert_eq!(got_sorted, want, "{r:?}");
}

#[test]
fn a_next_app_roots_declaration_does_not_exempt_a_sibling_packages_pages_dir() {
    // The anchor is the declaring directory, not "somewhere in this tree there is a Next app".
    // cal.com carries both shapes at once and they must land on opposite sides.
    let r = find_dead_candidates(
        &[
            n("apps/web/next.config.ts", 0, 1),
            n("apps/web/pages/api/book/event.ts", 0, 1),
            n(
                "packages/app-store/paypal/pages/setup/_getStaticProps.tsx",
                0,
                1,
            ),
            n(
                "apps/web/app/(use-page-wrapper)/(main-nav)/members/actions.ts",
                0,
                1,
            ),
        ],
        &empty_dep(),
        DEAD_MAX_CHANGES,
        &no_extra_entries(),
    );
    let mut got: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
    got.sort_unstable();
    assert_eq!(
        got,
        vec![
            "apps/web/app/(use-page-wrapper)/(main-nav)/members/actions.ts",
            "packages/app-store/paypal/pages/setup/_getStaticProps.tsx",
        ],
        "{r:?}"
    );
}

/// §27 pin — POSITION, not existence. The framework-convention clause must sit BEFORE the
/// imperative: a reader who acts on the first instruction never reaches a caveat placed after it,
/// and the act this rule prescribes is DELETING A WHOLE FILE. Before this pin the only hedge in the
/// message was the rule's own off switch at offset 1139, 401 characters past `Delete the file` at
/// 738, and it named a bundler entry and a dynamic import — neither of which a reader holding a
/// path-routed framework file would try on for size. Moving the clause after the verb leaves every
/// token present and must still turn this test red.
#[test]
fn framework_convention_clause_precedes_the_imperative() {
    let out = dead_candidate_findings(
        &[n("features/x/Orphan.tsx", 0, 1)],
        &empty_dep(),
        &no_extra_entries(),
    );
    assert_eq!(out.len(), 1);
    let clause = out[0]
        .message
        .find("A framework may load a whole file from its own path")
        .expect("framework-convention clause is missing from the message");
    let verb = out[0]
        .message
        .find("Delete the file")
        .expect("imperative is missing from the message");
    assert!(
        clause < verb,
        "clause at {clause} must precede the imperative at {verb}: {}",
        out[0].message
    );
}

/// §27 pin — POSITION, not existence, for the HALF-A-PRODUCT clause (2026-08-27). The sibling
/// `unimported-export` closes with the same warning ("if this is public API consumed outside this
/// repo (e.g. published to npm)") but places it AFTER its own imperative, and that placement cannot
/// be ported: the act this rule prescribes is deleting a whole FILE. MEASURED (nocodb, 2026-08-27):
/// `packages/nocodb/src/models/CommentReaction.ts` is told to delete a model whose only caller is
/// the closed-source enterprise half of the same product — `document-comments.service.ts:58`
/// `toggleReaction` is the CE stub of that call site, body `return null`, and the CE migrations
/// still create `nc_comment_reactions`. INVALIDATION PROBE: move the clause behind the imperative
/// and every token of it is still present in the message — only this assertion goes red, which is
/// the whole reason it asserts an ordering rather than a `contains`.
#[test]
fn ce_ee_consumer_clause_precedes_the_imperative() {
    let out = dead_candidate_findings(
        &[n("features/x/Orphan.tsx", 0, 1)],
        &empty_dep(),
        &no_extra_entries(),
    );
    assert_eq!(out.len(), 1);
    let clause = out[0]
        .message
        .find("Zero in-repo importers is also what HALF A PRODUCT looks like")
        .expect("half-a-product clause is missing from the message");
    let verb = out[0]
        .message
        .find("Delete the file")
        .expect("imperative is missing from the message");
    assert!(
        clause < verb,
        "clause at {clause} must precede the imperative at {verb}: {}",
        out[0].message
    );
}

/// The `32919f9` lesson, pinned: a closed enumeration reads as a checklist, and a reader who matches
/// none of its items proceeds with the imperative. This clause must therefore SAY it is examples and
/// must hand over the discriminating question — the same shape `unimported-export` landed in
/// `f891570`. Deleting either half leaves a list a reader can fall off the end of.
#[test]
fn ce_ee_consumer_clause_is_open_and_hands_over_a_question() {
    let out = dead_candidate_findings(
        &[n("features/x/Orphan.tsx", 0, 1)],
        &empty_dep(),
        &no_extra_entries(),
    );
    let m = &out[0].message;
    assert!(
        m.contains("these are examples rather than a list to match yourself against"),
        "the enumeration must declare itself open: {m}"
    );
    let q = m
        .find("is any shipping part of it built from a repository you are not looking at?")
        .expect("the discriminating question is missing: {m}");
    let clause = m
        .find("Zero in-repo importers is also what HALF A PRODUCT looks like")
        .expect("half-a-product clause is missing");
    assert!(q > clause, "the question belongs inside the clause: {m}");
}

/// The clause must NOT kill the prescription: a file with no importers and no out-of-tree half is
/// still worth deleting, and the sentence has to end by saying so or a reader learns to disbelieve
/// every finding this rule makes (`rule-quality.md` §32's withdrawn-draft lesson, applied to the
/// disclosure's own tail).
#[test]
fn ce_ee_consumer_clause_returns_the_reader_to_the_prescription() {
    let out = dead_candidate_findings(
        &[n("features/x/Orphan.tsx", 0, 1)],
        &empty_dep(),
        &no_extra_entries(),
    );
    let m = &out[0].message;
    let restore = m
        .find("If it is not, no importers means what it says.")
        .expect("the clause must return the reader to the prescription");
    let verb = m.find("Delete the file").expect("imperative is missing");
    assert!(
        restore < verb,
        "the restoring sentence must lead into the imperative: {m}"
    );
}
