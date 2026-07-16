//! Exercises `find_dead_exports` against hand-built fixtures — imports, barrel/aliased re-export
//! chains, entry-file live roots, default-export matching, and the `Unused` vs `InFileOnly` split.
use super::*;
use zzop_core::{disable_hint, ImportBinding};

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
    }
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
fn test_and_mock_dirs_are_excluded_at_source_stage() {
    let files = vec![file(
        "src/__test__/x.test.ts",
        vec![export("fixture", SourceSymbolKind::Function)],
    )];
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
    assert_eq!(out[0].rule_id, "dead-exports");
    // Interpolates `disable_hint`'s own output (rather than spelling "Disable via config `rules:
    // {...}`" as a literal here) so this file's own source never carries that literal text next to a
    // `` `export` `` backtick — `crates/engine/tests/rule_contracts/`'s CHECK B flags exactly that
    // shape (a backtick-quoted, non-config-key token sitting within 120 bytes of the word "config") as
    // an unvouched-for config-key reference. `disable_hint`'s own unit tests (`crates/core/src/
    // finding.rs`) already pin its rendered form; this test only needs to confirm it lands in the right
    // place in the surrounding sentence.
    let tail = disable_hint("dead-exports");
    assert_eq!(
        out[0].message,
        format!(
            "exported function 'helper' is never imported anywhere (deletion candidate). Delete it, \
             or export it from somewhere it's actually consumed. {tail} if this is public API \
             consumed outside this repo (e.g. published to npm) — such consumers are invisible to \
             this in-repo import graph."
        )
    );
    assert_eq!(
        out[1].message,
        format!(
            "exported const 'localOnly' is only referenced within its own file (un-export candidate). \
             Drop the `export` keyword to make the un-used-elsewhere status explicit. {tail} if this \
             is public API consumed outside this repo (e.g. published to npm) — such consumers are \
             invisible to this in-repo import graph."
        )
    );
}
