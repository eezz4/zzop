//! Exercises `find_dead_exports` against hand-built fixtures — imports, barrel/aliased re-export
//! chains, entry-file live roots, default-export matching, and the `Unused` vs `InFileOnly` split.
use super::*;
use zzop_core::{disable_hint, ImportBinding, ImportMap, ReExport};

fn resolve(spec: &str, _from: &str) -> Option<String> {
    Some(spec.strip_prefix("./").unwrap_or(spec).to_string())
}

fn resolve_relative_only(spec: &str, _from: &str) -> Option<String> {
    if spec.starts_with('.') {
        Some(spec.strip_prefix("./").unwrap_or(spec).to_string())
    } else {
        None
    }
}

fn export(name: &str, kind: SourceSymbolKind) -> DeadExportCandidate {
    DeadExportCandidate {
        name: name.to_string(),
        kind,
        is_default: false,
    }
}

fn default_export(name: &str, kind: SourceSymbolKind) -> DeadExportCandidate {
    DeadExportCandidate {
        name: name.to_string(),
        kind,
        is_default: true,
    }
}

fn file(name: &str, exports: Vec<DeadExportCandidate>) -> DeadExportInputFile {
    DeadExportInputFile {
        file: name.to_string(),
        exports,
        imports: ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: HashSet::new(),
        exported_signature_names: HashSet::new(),
        export_aliases: Vec::new(),
        is_generated: false,
        auto_import_referenced_names: HashSet::new(),
    }
}

fn alias(local: &str, public: &str) -> (String, String) {
    (local.to_string(), public.to_string())
}

fn import_of(specifier: &str, original: &str) -> ImportMap {
    let mut m = ImportMap::new();
    m.insert(
        "local".to_string(),
        ImportBinding {
            specifier: specifier.to_string(),
            original: original.to_string(),
            deferred: false,
            type_only: false,
        },
    );
    m
}

fn reexport(specifier: &str, original: &str, local_alias: &str) -> ReExport {
    ReExport {
        specifier: specifier.to_string(),
        original: original.to_string(),
        local_alias: local_alias.to_string(),
        type_only: false,
    }
}

#[test]
fn exported_symbol_that_is_imported_is_not_dead() {
    let files = vec![
        file("a.ts", vec![export("foo", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            imports: import_of("./a.ts", "foo"),
            ..file("b.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn function_const_not_imported_anywhere_is_dead() {
    let files = vec![
        file(
            "a.ts",
            vec![
                export("used", SourceSymbolKind::Function),
                export("unused", SourceSymbolKind::Function),
            ],
        ),
        DeadExportInputFile {
            imports: import_of("./a.ts", "used"),
            ..file("b.ts", vec![])
        },
    ];
    let dead = find_dead_exports(&files, resolve);
    assert_eq!(
        dead,
        vec![DeadExport {
            file: "a.ts".to_string(),
            name: "unused".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        }]
    );
}

#[test]
fn type_interface_are_also_dead_candidates() {
    let files = vec![file(
        "a.ts",
        vec![
            export("MyType", SourceSymbolKind::Type),
            export("MyShape", SourceSymbolKind::Interface),
            export("myFn", SourceSymbolKind::Function),
        ],
    )];
    let mut names: Vec<String> = find_dead_exports(&files, resolve)
        .into_iter()
        .map(|d| d.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["MyShape", "MyType", "myFn"]);
}

#[test]
fn type_export_is_alive_when_imported_at_least_once() {
    let files = vec![
        file("a.ts", vec![export("MyType", SourceSymbolKind::Type)]),
        DeadExportInputFile {
            imports: import_of("./a.ts", "MyType"),
            ..file("b.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn ambient_declaration_is_excluded_from_dead_candidates() {
    let files = vec![file(
        "globals.d.ts",
        vec![export("MyAmbient", SourceSymbolKind::Type)],
    )];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn tool_config_files_default_export_is_excluded_from_dead_candidates() {
    // Loaded directly by its own tool, never imported — the default export must not read as dead.
    let files = vec![file(
        "vite.config.ts",
        vec![default_export("config", SourceSymbolKind::Const)],
    )];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

/// THIS rule is a second consumer of `unreachable::tool_config_patterns`, and the one the widening did
/// not name: it was priced against `dead-candidates` alone, but `is_entry_or_test` reads the same
/// predicate, so every shape admitted there goes silent here too.
///
/// Both newly-admitted shapes ride with a CONTROL carrying a byte-equivalent export, because "these
/// two are excluded" and "everything is excluded" are the same assertion without one.
///
/// ⚠ **The qualifier row is `webpack.config.prod.js`, not `vite.config.sw.js`, and the difference is
/// load-bearing.** A `vite.`/`vitest.`/`jest.`/`cypress.`/`playwright.` stem is ALREADY matched by
/// `zzop_core::is_test_file` through the shared `test-paths` fragment, which `is_entry_or_test`
/// consults on its own — so a `vite.config.sw.js` row stays silent even with the widening reverted and
/// proves nothing about it. That was this test's first shape, and a refutation pass caught it supplying
/// half the evidence for a claim it could not support. Verified the repair the same way it was caught:
/// with `tool_config_patterns` reverted to its single pre-widening row, this assertion FAILS on both
/// remaining rows rather than one.
#[test]
fn the_widened_config_shapes_silence_this_rule_too_and_only_them() {
    let files = vec![
        // A SECOND config for the same tool — reached only through that tool's `--config` flag, and a
        // stem the test-path fragment does not carry, so only the widening can silence it.
        file(
            "webpack.config.prod.js",
            vec![default_export("config", SourceSymbolKind::Const)],
        ),
        // `config.<ext>` directly inside a tool-owned DOT-directory (VitePress).
        file(
            "docs/.vitepress/config.mts",
            vec![default_export("config", SourceSymbolKind::Const)],
        ),
        // The control is one level DEEPER — ordinary source under a tool directory. It was a sibling
        // at depth 1 until 2026-08-21, when the dot-directory arm widened from the `config` stem to
        // depth: `.storybook/main.mjs` is as tool-owned as `.storybook/config.js`, so a depth-1
        // sibling stopped being outside the exemption and stopped being a control.
        file(
            "docs/.vitepress/theme/other.ts",
            vec![default_export("config", SourceSymbolKind::Const)],
        ),
    ];
    let found = find_dead_exports(&files, resolve);
    let reported: Vec<&str> = found.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(
        reported,
        vec!["docs/.vitepress/theme/other.ts"],
        "the two config shapes are silent here and the control is not: {reported:?}"
    );
}

#[test]
fn namespace_import_treats_all_exports_of_that_file_as_alive() {
    let files = vec![
        file(
            "a.ts",
            vec![
                export("x", SourceSymbolKind::Function),
                export("y", SourceSymbolKind::Function),
            ],
        ),
        DeadExportInputFile {
            imports: import_of("./a.ts", "*"),
            ..file("b.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn entry_files_are_not_dead_candidates() {
    let files = vec![
        file(
            "src/index.ts",
            vec![export("x", SourceSymbolKind::Function)],
        ),
        file(
            "pages/HomePage.tsx",
            vec![export("HomePage", SourceSymbolKind::Function)],
        ),
        file("App.tsx", vec![export("App", SourceSymbolKind::Function)]),
        file(
            "api/apiRoutes.ts",
            vec![export("routes", SourceSymbolKind::Const)],
        ),
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn nextjs_app_router_convention_files_are_framework_entries() {
    let files = vec![
        file(
            "app/(lang)/[lang]/about/page.tsx",
            vec![
                default_export("AboutPage", SourceSymbolKind::Function),
                export("generateMetadata", SourceSymbolKind::Function),
                export("generateStaticParams", SourceSymbolKind::Function),
                export("dynamicParams", SourceSymbolKind::Const),
            ],
        ),
        file(
            "app/(lang)/[lang]/error.tsx",
            vec![default_export("ErrorPage", SourceSymbolKind::Function)],
        ),
        file(
            "app/api/x/route.ts",
            vec![export("GET", SourceSymbolKind::Function)],
        ),
        file(
            "app/sitemap.ts",
            vec![default_export("sitemap", SourceSymbolKind::Function)],
        ),
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn sveltekit_route_and_hook_convention_files_are_framework_entries() {
    // SvelteKit invokes `load`/`actions` (`+page(.server)`/`+layout(.server)`), `handle`/`handleError`
    // (`hooks.{server,client}`), and `GET`/`POST` (`+server`) by EXACT name via its file-based routing +
    // hooks contract — zero in-repo importers, so they must not read as dead (dogfood fe-svelte: these
    // were the dominant dead-export FP class). `.js` and `.ts` both.
    let files = vec![
        file(
            "src/routes/+page.server.js",
            vec![
                export("load", SourceSymbolKind::Function),
                export("actions", SourceSymbolKind::Const),
            ],
        ),
        file(
            "src/routes/+layout.server.ts",
            vec![export("load", SourceSymbolKind::Function)],
        ),
        file(
            "src/hooks.server.js",
            vec![
                export("handle", SourceSymbolKind::Function),
                export("handleError", SourceSymbolKind::Function),
            ],
        ),
        file(
            "src/routes/api/articles/+server.ts",
            vec![
                export("GET", SourceSymbolKind::Function),
                export("POST", SourceSymbolKind::Function),
            ],
        ),
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn test_and_mock_dirs_are_excluded_at_source_stage() {
    let files = vec![file(
        "src/__test__/x.test.ts",
        vec![export("fixture", SourceSymbolKind::Function)],
    )];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn test_runner_directory_and_setup_files_are_excluded_at_source_stage() {
    // `playwright/`-dir files (shared `zzop_core::is_test_file` SSOT) and config-loaded setup entries
    // (`is_tool_entry_file`) are runner/config-loaded, never imported — their exports are not dead.
    let files = vec![
        file(
            "playwright/global.setup.ts",
            vec![export("globalSetup", SourceSymbolKind::Function)],
        ),
        file(
            "playwright/utils/test-decorators.ts",
            vec![export("step", SourceSymbolKind::Function)],
        ),
        file(
            "src/setup-tests.ts",
            vec![export("setupVitest", SourceSymbolKind::Function)],
        ),
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn default_export_is_tracked() {
    let files = vec![
        file("a.ts", vec![export("default", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            imports: import_of("./a.ts", "default"),
            ..file("b.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn external_module_import_is_ignored() {
    let files = vec![
        file("a.ts", vec![export("foo", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            imports: import_of("react", "foo"),
            ..file("b.ts", vec![])
        },
    ];
    let dead = find_dead_exports(&files, resolve_relative_only);
    assert_eq!(
        dead,
        vec![DeadExport {
            file: "a.ts".to_string(),
            name: "foo".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        }]
    );
}

#[test]
fn barrel_re_export_chain_resolves_source_as_alive() {
    let files = vec![
        file("a.ts", vec![export("Foo", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            re_exports: vec![reexport("./a.ts", "Foo", "Foo")],
            ..file("barrel/index.ts", vec![])
        },
        DeadExportInputFile {
            imports: import_of("./barrel/index.ts", "Foo"),
            ..file("consumer.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn aliased_re_export_consumer_imports_alias_source_is_alive() {
    let files = vec![
        file("a.ts", vec![export("Orig", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            re_exports: vec![reexport("./a.ts", "Orig", "Alias")],
            ..file("barrel/index.ts", vec![])
        },
        DeadExportInputFile {
            imports: import_of("./barrel/index.ts", "Alias"),
            ..file("consumer.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn star_re_export_wildcards_the_whole_source_file() {
    let files = vec![
        file(
            "a.ts",
            vec![
                export("x", SourceSymbolKind::Function),
                export("y", SourceSymbolKind::Const),
            ],
        ),
        DeadExportInputFile {
            re_exports: vec![reexport("./a.ts", "*", "*")],
            ..file("barrel/index.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn dynamic_import_wildcards_the_whole_target_file() {
    let files = vec![
        file(
            "a.ts",
            vec![
                export("x", SourceSymbolKind::Function),
                export("y", SourceSymbolKind::Const),
            ],
        ),
        DeadExportInputFile {
            dynamic_imports: vec!["./a.ts".to_string()],
            ..file("consumer.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn named_default_export_is_alive_via_default_import() {
    let files = vec![
        file(
            "a.ts",
            vec![default_export("Foo", SourceSymbolKind::Function)],
        ),
        DeadExportInputFile {
            imports: import_of("./a.ts", "default"),
            ..file("b.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn reason_in_file_only_when_referenced_only_within_the_file() {
    let files = vec![DeadExportInputFile {
        used_names: HashSet::from(["HELPER".to_string()]),
        ..file("a.ts", vec![export("HELPER", SourceSymbolKind::Const)])
    }];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "a.ts".to_string(),
            name: "HELPER".to_string(),
            kind: SourceSymbolKind::Const,
            reason: DeadExportReason::InFileOnly,
        }]
    );
}

#[test]
fn reason_unused_when_referenced_nowhere() {
    let files = vec![file(
        "a.ts",
        vec![export("HELPER", SourceSymbolKind::Const)],
    )];
    let dead = find_dead_exports(&files, resolve);
    assert_eq!(dead[0].reason, DeadExportReason::Unused);
}

#[test]
fn named_default_export_without_any_default_import_is_dead() {
    let files = vec![file(
        "a.ts",
        vec![default_export("Foo", SourceSymbolKind::Function)],
    )];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "a.ts".to_string(),
            name: "Foo".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        }]
    );
}

#[test]
fn entry_re_export_is_a_live_root_even_with_no_consumer() {
    // An entry file re-exporting `impl` with no in-repo importer is still public API, not dead.
    let files = vec![
        file("impl.ts", vec![export("impl", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            re_exports: vec![reexport("./impl.ts", "impl", "impl")],
            ..file("index.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn entry_re_export_root_propagates_across_a_deeper_barrel_hop() {
    let files = vec![
        file("impl.ts", vec![export("impl", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            re_exports: vec![reexport("./impl.ts", "impl", "impl")],
            ..file("mid.ts", vec![])
        },
        DeadExportInputFile {
            re_exports: vec![reexport("./mid.ts", "impl", "impl")],
            ..file("index.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn re_export_from_a_non_entry_file_is_not_a_live_root_by_itself() {
    // A non-entry re-exporter alone isn't a live root; a real import must exist somewhere in the chain.
    let files = vec![
        file("impl.ts", vec![export("impl", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            re_exports: vec![reexport("./impl.ts", "impl", "impl")],
            ..file("reexporter.ts", vec![])
        },
    ];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "impl.ts".to_string(),
            name: "impl".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        }]
    );
}

#[test]
fn re_export_chain_propagates_across_2_hops() {
    let files = vec![
        file("a.ts", vec![export("Foo", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            re_exports: vec![reexport("./a.ts", "Foo", "Foo")],
            ..file("mid.ts", vec![])
        },
        DeadExportInputFile {
            re_exports: vec![reexport("./mid.ts", "Foo", "Foo")],
            ..file("barrel/index.ts", vec![])
        },
        DeadExportInputFile {
            imports: import_of("./barrel/index.ts", "Foo"),
            ..file("consumer.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn storybook_config_dir_export_is_excluded_from_dead_candidates() {
    // `.storybook/preview.tsx`'s `decorators` is consumed by Storybook's own builder, never imported.
    let files = vec![file(
        ".storybook/preview.tsx",
        vec![export("decorators", SourceSymbolKind::Const)],
    )];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn nextjs_pages_router_data_fetching_export_is_not_dead() {
    // Pages Router files have arbitrary filenames (unlike App Router's `page.tsx` convention), so
    // this relies on the framework-contract-export allowlist rather than file-level exclusion.
    let files = vec![file(
        "pages/blog/[slug].tsx",
        vec![export("getServerSideProps", SourceSymbolKind::Function)],
    )];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn nextjs_middleware_convention_file_exports_are_not_dead() {
    // Root `middleware.ts` (and a monorepo app's `apps/web/middleware.ts`) export `middleware` +
    // `config`, both read by Next.js by exact name — never imported.
    let files = vec![
        file(
            "middleware.ts",
            vec![
                export("middleware", SourceSymbolKind::Function),
                export("config", SourceSymbolKind::Const),
            ],
        ),
        file(
            "apps/web/middleware.ts",
            vec![export("middleware", SourceSymbolKind::Function)],
        ),
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn nextjs_pages_api_route_config_export_is_not_dead() {
    // MEASURED (cal.com, 2026-08-26): five payment-webhook routes under `apps/web/pages/api/**`
    // carry `export const config = { api: { bodyParser: false } }`. Next.js reads that object from
    // the FILE PATH at build time; no module imports it. Deleting it turns the body parser back on,
    // the raw body the HMAC signature check needs is consumed, and every payment webhook fails to
    // verify. Same name+path scoping as `is_middleware_convention_file`, one directory over.
    let files = vec![
        file(
            "apps/web/pages/api/integrations/btcpayserver/webhook.ts",
            vec![export("config", SourceSymbolKind::Const)],
        ),
        file(
            "src/pages/api/webhook.ts",
            vec![export("config", SourceSymbolKind::Const)],
        ),
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn other_exports_in_a_pages_api_route_are_still_dead_candidates() {
    // Name-scoped, not a wholesale directory exclusion: the ten `handler` exports this corpus has
    // under `pages/api/**` must keep reporting.
    let files = vec![file(
        "apps/web/pages/api/stripe/webhook.ts",
        vec![export("handler", SourceSymbolKind::Function)],
    )];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "apps/web/pages/api/stripe/webhook.ts".to_string(),
            name: "handler".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        }]
    );
}

#[test]
fn config_export_outside_a_pages_api_route_is_still_dead() {
    // The path scoping must not leak into a global `config` exemption — `config` is a plausible
    // domain symbol. Both rows are MEASURED cal.com shapes that must survive:
    // `packages/app-store/salesforce/codegen.ts:3`, and a `pages/` file outside `pages/api/`.
    let files = vec![
        file(
            "packages/app-store/salesforce/codegen.ts",
            vec![export("config", SourceSymbolKind::Const)],
        ),
        file(
            "apps/web/pages/settings/billing.tsx",
            vec![export("config", SourceSymbolKind::Const)],
        ),
    ];
    assert_eq!(find_dead_exports(&files, resolve).len(), 2);
}

#[test]
fn other_exports_in_a_middleware_file_are_still_dead_candidates() {
    // The exemption is name-scoped (`middleware`/`config` only), not a wholesale file exclusion.
    let files = vec![file(
        "middleware.ts",
        vec![export("helper", SourceSymbolKind::Function)],
    )];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "middleware.ts".to_string(),
            name: "helper".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        }]
    );
}

#[test]
fn middleware_named_export_outside_a_middleware_file_is_still_dead() {
    // Regression guard: the filename scoping must not leak into a global name exemption.
    let files = vec![file(
        "src/utils.ts",
        vec![export("middleware", SourceSymbolKind::Function)],
    )];
    assert_eq!(find_dead_exports(&files, resolve).len(), 1);
}

#[test]
fn ordinary_never_imported_export_in_a_normal_file_is_still_dead() {
    // Regression guard: the framework-contract allowlist must not over-broaden to arbitrary symbols.
    let files = vec![file(
        "src/utils.ts",
        vec![export("helper", SourceSymbolKind::Function)],
    )];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "src/utils.ts".to_string(),
            name: "helper".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        }]
    );
}

#[test]
fn generated_file_export_is_not_a_dead_candidate() {
    // A file the engine flagged `is_generated` (author-declared `@generated`/"Code generated by …"
    // banner — a bare "DO NOT EDIT" does NOT qualify; see `is_generated`'s own doc) is
    // regenerated, never hand-edited — an un-export finding there is non-actionable, so it's skipped whole.
    let files = vec![DeadExportInputFile {
        is_generated: true,
        ..file(
            "src/client/sdk.gen.ts",
            vec![
                export("getUser", SourceSymbolKind::Function),
                export("createUser", SourceSymbolKind::Function),
            ],
        )
    }];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn generated_files_imports_still_keep_a_normal_files_export_alive() {
    // The skip is dead-check-only: a generated file's imports must still count, or a symbol used solely
    // by generated code would look dead. Here `a.ts#Foo` is imported only by the generated file.
    let files = vec![
        file("a.ts", vec![export("Foo", SourceSymbolKind::Function)]),
        DeadExportInputFile {
            is_generated: true,
            imports: import_of("./a.ts", "Foo"),
            ..file("gen.ts", vec![export("gened", SourceSymbolKind::Function)])
        },
    ];
    // Neither `a.ts#Foo` (imported by the generated file) nor `gen.ts#gened` (generated) is reported.
    assert!(find_dead_exports(&files, resolve).is_empty());
}

/// Pins the exact rendered message — regression coverage for the `disable_hint` splice
/// `dead_export_to_finding` went through during the 2026-07-10 dialect-consolidation sweep. Covers both
/// `DeadExportReason` variants, since each selects different fixed text around the shared hint.
#[test]
fn finding_message_is_byte_identical_to_the_pre_sweep_text() {
    let dead = vec![
        DeadExport {
            file: "src/utils.ts".to_string(),
            name: "helper".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        },
        DeadExport {
            file: "src/utils.ts".to_string(),
            name: "localOnly".to_string(),
            kind: SourceSymbolKind::Const,
            reason: DeadExportReason::InFileOnly,
        },
    ];
    let out = dead_export_findings(dead, &HashMap::new());
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].rule_id, "unimported-export");
    // Interpolates `disable_hint`'s own output (rather than spelling "Disable via config `rules:
    // {...}`" as a literal here) so this file's own source never carries that literal text next to a
    // `` `export` `` backtick — `crates/engine/tests/rule_contracts/`'s CHECK B flags exactly that
    // shape (a backtick-quoted, non-config-key token sitting within 120 bytes of the word "config") as
    // an unvouched-for config-key reference. `disable_hint`'s own unit tests (`crates/core/src/
    // finding.rs`) already pin its rendered form; this test only needs to confirm it lands in the right
    // place in the surrounding sentence.
    let tail = disable_hint("unimported-export");
    // The convention clause is spelled once and interpolated into both arms: it is the SAME sentence
    // in both, and a copy per arm is how the two drift. It also keeps this pin honest about what it
    // is pinning — the surrounding fixed text and the ORDER, not the clause's own wording, which
    // `framework_convention_clause_precedes_the_imperative_in_both_arms` is the owner of.
    let convention = "Zero in-repo importers is BY DESIGN for two shapes, and these are examples \
                      rather than a list to match yourself against: in both, deleting the export \
                      changes runtime behavior with no import left to break. (1) A framework reads \
                      this name from the file's own path rather than importing it — `export const \
                      config` in a Next.js Pages Router route, `export const prerender` in an Astro \
                      page, `export function load` in a SvelteKit route. (2) A build tool injects \
                      a whole DIRECTORY's exports as globals, so every consumer writes the bare \
                      identifier and no import line exists anywhere — Nuxt auto-imports the \
                      directories its `nuxt.config.*` nominates, and `unplugin-auto-import` does \
                      the same from a Vite or Webpack setup. Nothing is reserved about the NAME in (2), so \
                      the question to ask is about the PATH: does a build tool in this tree \
                      nominate the directory this symbol lives in as an injection source? A \
                      `nuxt.config.*` that nominates it is already read and resolved here, name by \
                      name; a table declared anywhere else, or inherited by a Nuxt layer through \
                      `extends`, is not.";
    assert_eq!(
        out[0].message,
        format!(
            "exported function 'helper' is never imported anywhere (deletion candidate). \
             {convention} Delete it, \
             or export it from somewhere it's actually consumed. A file carrying a machine-generated \
             banner in its first 8 lines is skipped by this rule already \
             (`vocabulary.generatedFileMarkers` picks the banner vocabulary); a generator that \
             stamps NO banner is invisible to that, and the answer for it is an `exclude` entry for \
             its path — deleting the `export` there is undone by the next regeneration. {tail} if \
             this is public API consumed outside this repo (e.g. published to npm) — such consumers \
             are invisible to this in-repo import graph."
        )
    );
    assert_eq!(
        out[1].message,
        format!(
            "exported const 'localOnly' is only referenced within its own file (un-export candidate). \
             {convention} \
             Drop the `export` keyword to make the un-used-elsewhere status explicit. A file carrying \
             a machine-generated banner in its first 8 lines is skipped by this rule already \
             (`vocabulary.generatedFileMarkers` picks the banner vocabulary); a generator that \
             stamps NO banner is invisible to that, and the answer for it is an `exclude` entry for \
             its path — deleting the `export` there is undone by the next regeneration. {tail} if \
             this is public API consumed outside this repo (e.g. published to npm) — such consumers \
             are invisible to this in-repo import graph."
        )
    );
}

/// §27 pin — POSITION, not existence. The framework-convention clause must sit BEFORE the
/// imperative in BOTH arms: a reader who acts on the first instruction never reaches a caveat
/// placed after it, and the act here is deleting the `export const config` that keeps a payment
/// webhook's raw body intact. Moving the clause after the verb leaves every token present and must
/// still turn this test red.
///
/// BOTH shapes are pinned, and the discriminating question with them. A reader who matches none of
/// shape (1)'s three examples has to be told the list is open and handed a question they can answer
/// about their own tree — that is the whole repair for the Nuxt reader who read three path-convention
/// examples, matched none, and deleted a live composable. A pin on shape (1) alone would let shape
/// (2) or the question drift below the verb with this test still green.
#[test]
fn framework_convention_clause_precedes_the_imperative_in_both_arms() {
    let dead = vec![
        DeadExport {
            file: "apps/web/pages/api/x.ts".to_string(),
            name: "helper".to_string(),
            kind: SourceSymbolKind::Function,
            reason: DeadExportReason::Unused,
        },
        DeadExport {
            file: "apps/web/pages/api/x.ts".to_string(),
            name: "localOnly".to_string(),
            kind: SourceSymbolKind::Const,
            reason: DeadExportReason::InFileOnly,
        },
    ];
    let out = dead_export_findings(dead, &HashMap::new());
    // Every one of these must sit before the verb: the "these are examples" disclaimer that opens the
    // list, each of the two shapes, and the question a reader answers about their own tree.
    let needles = [
        "these are examples rather than a list to match yourself against",
        "A framework reads this name from the file's own path",
        "A build tool injects a whole DIRECTORY's exports as globals",
        "does a build tool in this tree nominate the directory this symbol lives in",
    ];
    for (finding, imperative) in out.iter().zip(["Delete it,", "Drop the `export` keyword"]) {
        let verb = finding
            .message
            .find(imperative)
            .expect("imperative is missing from the message");
        for needle in needles {
            let clause = finding
                .message
                .find(needle)
                .unwrap_or_else(|| panic!("missing from the message: {needle}"));
            assert!(
                clause < verb,
                "'{needle}' at {clause} must precede the imperative at {verb}: {}",
                finding.message
            );
        }
    }
}

// ---- Public-signature exemption (module doc "Public-signature exemption") --------------------
// The measured shape: `export interface XState {…}` + `export function useX(): XState`. The type
// has no in-repo importer, so it used to report as `InFileOnly` ("un-export me") even though it is
// part of `useX`'s public API. The two TRUE positives below share every signal the old rule could
// see and must keep firing — which is the whole reason `exported_signature_names` is a separate,
// position-aware fact rather than a heuristic over `used_names`.

#[test]
fn type_in_an_exported_signature_is_exempt() {
    let files = vec![DeadExportInputFile {
        used_names: HashSet::from(["XState".to_string()]),
        exported_signature_names: HashSet::from(["XState".to_string()]),
        ..file(
            "hook.ts",
            vec![export("XState", SourceSymbolKind::Interface)],
        )
    }];
    assert!(
        find_dead_exports(&files, resolve).is_empty(),
        "a type named in an exported declaration's signature is public API"
    );
}

#[test]
fn type_alias_in_an_exported_signature_is_exempt() {
    let files = vec![DeadExportInputFile {
        used_names: HashSet::from(["Result".to_string()]),
        exported_signature_names: HashSet::from(["Result".to_string()]),
        ..file("api.ts", vec![export("Result", SourceSymbolKind::Type)])
    }];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn body_only_type_still_reports_in_file_only() {
    // TRUE POSITIVE 1: used only as an internal `useState<T>` generic in a hook with no annotated
    // return type. `used_names` sees it (identical to the exempt case above); the parser's
    // signature set does NOT — that difference is the entire point of the new fact.
    let files = vec![DeadExportInputFile {
        used_names: HashSet::from(["XState".to_string()]),
        exported_signature_names: HashSet::new(),
        ..file(
            "hook.ts",
            vec![export("XState", SourceSymbolKind::Interface)],
        )
    }];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "hook.ts".to_string(),
            name: "XState".to_string(),
            kind: SourceSymbolKind::Interface,
            reason: DeadExportReason::InFileOnly,
        }]
    );
}

#[test]
fn type_annotating_only_an_unexported_declaration_still_reports() {
    // TRUE POSITIVE 2: only annotates an UNEXPORTED `Props` field. The parser never walks an
    // unexported declaration, so the name never reaches the signature set.
    let files = vec![DeadExportInputFile {
        used_names: HashSet::from(["XThing".to_string()]),
        exported_signature_names: HashSet::from(["Props".to_string()]),
        ..file(
            "card.tsx",
            vec![export("XThing", SourceSymbolKind::Interface)],
        )
    }];
    assert_eq!(
        find_dead_exports(&files, resolve)
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["XThing"]
    );
}

#[test]
fn exemption_does_not_apply_to_value_kinds() {
    // A VALUE's name reaching a type position would need `typeof`, which this evidence does not
    // model — so a same-named const/function/class is never exempted by it.
    for kind in [
        SourceSymbolKind::Const,
        SourceSymbolKind::Function,
        SourceSymbolKind::Class,
    ] {
        let files = vec![DeadExportInputFile {
            used_names: HashSet::from(["Thing".to_string()]),
            exported_signature_names: HashSet::from(["Thing".to_string()]),
            ..file("a.ts", vec![export("Thing", kind)])
        }];
        assert_eq!(
            find_dead_exports(&files, resolve).len(),
            1,
            "value kind {kind:?} must not be exempted"
        );
    }
}

#[test]
fn exemption_also_covers_a_never_referenced_type() {
    // `Unused` (not even in `used_names`) but present in an exported signature: an exported type
    // that annotates a re-exported wrapper's return without being mentioned elsewhere. Still public.
    let files = vec![DeadExportInputFile {
        used_names: HashSet::new(),
        exported_signature_names: HashSet::from(["XState".to_string()]),
        ..file(
            "hook.ts",
            vec![export("XState", SourceSymbolKind::Interface)],
        )
    }];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn empty_signature_set_preserves_pre_existing_behavior() {
    // Graceful degrade: a non-TypeScript parser (or a degraded file) produces no signature names,
    // which must yield exactly the findings the rule produced before this fact existed.
    let files = vec![DeadExportInputFile {
        used_names: HashSet::from(["Helper".to_string()]),
        exported_signature_names: HashSet::new(),
        ..file(
            "a.go",
            vec![
                export("Helper", SourceSymbolKind::Interface),
                export("Other", SourceSymbolKind::Type),
            ],
        )
    }];
    assert_eq!(
        find_dead_exports(&files, resolve)
            .iter()
            .map(|d| (d.name.as_str(), d.reason))
            .collect::<Vec<_>>(),
        vec![
            ("Helper", DeadExportReason::InFileOnly),
            ("Other", DeadExportReason::Unused),
        ]
    );
}

// ---- Local export renames (module doc "Local renames") ---------------------------------------
// `export { X as Y }` with no from-clause: the candidate is named `X` (its declaration), every
// importer's key is `{file}#Y`. This is an extra KEY, never an exemption — the negative fixtures
// below are the guard that a renamed-but-unimported export stays dead.

#[test]
fn local_rename_imported_under_its_public_name_is_alive() {
    // The measured mono-hub shape: `interface State` + `export type { State as MortgageState }`,
    // imported as `MortgageState` by four files. Before `export_aliases`, `State` reported
    // `in-file-only` because `a.ts#MortgageState` never met the candidate named `State`.
    let files = vec![
        DeadExportInputFile {
            used_names: HashSet::from(["State".to_string()]),
            export_aliases: vec![alias("State", "MortgageState")],
            ..file("a.ts", vec![export("State", SourceSymbolKind::Interface)])
        },
        DeadExportInputFile {
            imports: import_of("./a.ts", "MortgageState"),
            ..file("b.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn local_rename_that_nobody_imports_is_still_dead() {
    // NEGATIVE FIXTURE: the whole risk of tracking renames is resurrecting genuinely dead exports.
    // A rename is a public NAME, not evidence of a consumer — with no importer this must still fire.
    let files = vec![DeadExportInputFile {
        used_names: HashSet::from(["State".to_string()]),
        export_aliases: vec![alias("State", "MortgageState")],
        ..file("a.ts", vec![export("State", SourceSymbolKind::Interface)])
    }];
    assert_eq!(
        find_dead_exports(&files, resolve),
        vec![DeadExport {
            file: "a.ts".to_string(),
            name: "State".to_string(),
            kind: SourceSymbolKind::Interface,
            reason: DeadExportReason::InFileOnly,
        }]
    );
}

#[test]
fn a_renames_public_name_does_not_keep_a_different_export_alive() {
    // NEGATIVE FIXTURE: the mapping is per-declaration. `Other` shares the file with a live rename
    // but has no importer of its own, so it must still report.
    let files = vec![
        DeadExportInputFile {
            export_aliases: vec![alias("State", "MortgageState")],
            ..file(
                "a.ts",
                vec![
                    export("State", SourceSymbolKind::Interface),
                    export("Other", SourceSymbolKind::Interface),
                ],
            )
        },
        DeadExportInputFile {
            imports: import_of("./a.ts", "MortgageState"),
            ..file("b.ts", vec![])
        },
    ];
    assert_eq!(
        find_dead_exports(&files, resolve)
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Other"]
    );
}

#[test]
fn importing_a_public_name_no_declaration_carries_keeps_nothing_alive() {
    // NEGATIVE FIXTURE: an unrelated `{file}#Y` key must not spill onto a candidate just because the
    // file happens to rename something else.
    let files = vec![
        DeadExportInputFile {
            export_aliases: vec![alias("State", "MortgageState")],
            ..file("a.ts", vec![export("State", SourceSymbolKind::Interface)])
        },
        DeadExportInputFile {
            imports: import_of("./a.ts", "SomethingElse"),
            ..file("b.ts", vec![])
        },
    ];
    assert_eq!(find_dead_exports(&files, resolve).len(), 1);
}

#[test]
fn local_rename_reaches_through_a_barrel_re_export_chain() {
    // The barrel re-exports the PUBLIC name; chain propagation seeds `a.ts#MortgageState`, which the
    // alias map then connects to the declaration named `State`.
    let files = vec![
        DeadExportInputFile {
            export_aliases: vec![alias("State", "MortgageState")],
            ..file("a.ts", vec![export("State", SourceSymbolKind::Interface)])
        },
        DeadExportInputFile {
            re_exports: vec![reexport("./a.ts", "MortgageState", "MortgageState")],
            ..file("barrel/index.ts", vec![])
        },
        DeadExportInputFile {
            imports: import_of("./barrel/index.ts", "MortgageState"),
            ..file("consumer.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn rename_to_default_is_alive_via_a_default_import() {
    // `export { Foo as default }` — the parser's alias map spells `default` explicitly, so the link
    // holds even for a candidate whose `is_default` flag was never set.
    let files = vec![
        DeadExportInputFile {
            export_aliases: vec![alias("Foo", "default")],
            ..file("a.ts", vec![export("Foo", SourceSymbolKind::Function)])
        },
        DeadExportInputFile {
            imports: import_of("./a.ts", "default"),
            ..file("b.ts", vec![])
        },
    ];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

// ---- Auto-import tables (module doc "Auto-import tables") ------------------------------------
// A build config injecting a whole directory's exports as globals leaves NO import statement for the
// graph to read. The engine resolves those bare references and hands each file the subset of its own
// export names that some other file in the same app actually writes. The pair below is the whole
// design: resolution, not directory exemption.

#[test]
fn an_auto_import_referenced_export_is_alive_with_no_importer_anywhere() {
    // nocodb `composables/useRowComments.ts#useProvideRowComments`: two consumers (one `.ts`, one
    // `.vue`) write it bare, zero `import` statements exist for it in the whole repo, and the rule
    // told a blind auditor to delete it — which breaks the expanded-form Discussion sidebar.
    let files = vec![DeadExportInputFile {
        auto_import_referenced_names: HashSet::from(["useProvideRowComments".to_string()]),
        ..file(
            "app/composables/useRowComments.ts",
            vec![export("useProvideRowComments", SourceSymbolKind::Function)],
        )
    }];
    assert!(find_dead_exports(&files, resolve).is_empty());
}

#[test]
fn an_unreferenced_export_in_the_same_auto_import_file_still_reports() {
    // The direction that makes this a RESOLUTION rather than an exemption, and the reason a
    // file-level fan-in count cannot stand in for the name set: `snapshotFilter` sits in the same
    // tree, the same app and the same auto-import directory as the 487 silenced names, and it is the
    // one symbol a blind auditor's 14-name sample found referenced nowhere. Exempting the directory
    // — or trusting the file's fan-in — kills exactly the finding this rule exists to make.
    let files = vec![DeadExportInputFile {
        auto_import_referenced_names: HashSet::from(["isFilterOp".to_string()]),
        ..file(
            "app/utils/filterUtils.ts",
            vec![
                export("isFilterOp", SourceSymbolKind::Function),
                export("snapshotFilter", SourceSymbolKind::Function),
            ],
        )
    }];
    let dead = find_dead_exports(&files, resolve);
    assert_eq!(dead.len(), 1, "{dead:?}");
    assert_eq!(dead[0].name, "snapshotFilter");
    assert_eq!(dead[0].reason, DeadExportReason::Unused);
}

#[test]
fn auto_import_names_do_not_leak_across_files() {
    // The set is per-FILE, keyed by the file that is reached. A name resolved for one auto-import
    // file must not vouch for a same-named export in another — the engine's scan pairs every name
    // with the candidate that publishes it, and this pins that the rule never widens that pairing.
    let files = vec![
        DeadExportInputFile {
            auto_import_referenced_names: HashSet::from(["useThing".to_string()]),
            ..file(
                "app/composables/a.ts",
                vec![export("useThing", SourceSymbolKind::Function)],
            )
        },
        file(
            "app/composables/b.ts",
            vec![export("useThing", SourceSymbolKind::Function)],
        ),
    ];
    let dead = find_dead_exports(&files, resolve);
    assert_eq!(dead.len(), 1, "{dead:?}");
    assert_eq!(dead[0].file, "app/composables/b.ts");
}
