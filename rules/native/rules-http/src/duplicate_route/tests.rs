//! Unit tests for `duplicate_route_findings`'s grouping logic (e2e coverage: `crates/engine/tests/integration/analyze_routes_hono.rs`).

use super::*;

fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
    zzop_core::IoProvide {
        response: None,
        body: None,
        kind: "http".to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
        symbol: None,
        ..Default::default()
    }
}

fn provide_sym(key: &str, file: &str, line: u32, symbol: &str) -> zzop_core::IoProvide {
    zzop_core::IoProvide {
        symbol: Some(symbol.to_string()),
        ..provide(key, file, line)
    }
}

/// A site whose controller declared a NON-URI version scope (`route-version-v1`) — the header-versioned
/// shape the URL never carries, so the key is identical on both sides and only this field separates them.
fn provide_ver(key: &str, file: &str, line: u32, ver: &str) -> zzop_core::IoProvide {
    zzop_core::IoProvide {
        route_version: Some(ver.to_string()),
        ..provide(key, file, line)
    }
}

#[test]
fn single_registration_of_a_route_is_not_flagged() {
    let provides = vec![provide("GET /api/users", "a.ts", 3)];
    assert!(duplicate_route_findings(&provides, &Default::default()).is_empty());
}

#[test]
fn two_distinct_routes_are_not_flagged() {
    let provides = vec![
        provide("GET /api/users", "a.ts", 3),
        provide("POST /api/users", "b.ts", 5),
    ];
    assert!(duplicate_route_findings(&provides, &Default::default()).is_empty());
}

#[test]
fn same_route_registered_twice_flags_only_the_later_site() {
    let provides = vec![
        provide("GET /api/users", "b.ts", 10),
        provide("GET /api/users", "a.ts", 3),
    ];
    let found = duplicate_route_findings(&provides, &Default::default());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file, "b.ts");
    assert_eq!(found[0].line, 10);
    assert_eq!(found[0].rule_id, "duplicate-route");
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
    assert!(found[0].message.contains("a.ts:3"));
}

#[test]
fn three_registrations_of_one_route_flag_the_two_later_sites() {
    let provides = vec![
        provide("GET /api/users", "c.ts", 1),
        provide("GET /api/users", "a.ts", 9),
        provide("GET /api/users", "a.ts", 2),
    ];
    // sorted by (file, line): a.ts:2 (first/canonical), a.ts:9, c.ts:1
    let found = duplicate_route_findings(&provides, &Default::default());
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].file.as_str(), found[0].line), ("a.ts", 9));
    assert_eq!((found[1].file.as_str(), found[1].line), ("c.ts", 1));
    for f in &found {
        assert!(f.message.contains("a.ts:2"));
    }
}

#[test]
fn duplicate_with_one_site_in_a_test_file_is_not_flagged() {
    let provides = vec![
        provide("GET /api/users", "routes/__tests__/api.test.ts", 12),
        provide("GET /api/users", "src/routes/users.ts", 3),
    ];
    // only one real (non-test) site remains, so < 2 sites -> no finding
    assert!(duplicate_route_findings(&provides, &Default::default()).is_empty());
}

#[test]
fn duplicate_with_both_sites_in_test_files_is_not_flagged() {
    let provides = vec![
        provide("GET /api/users", "src/routes/__tests__/a.test.ts", 1),
        provide("GET /api/users", "src/routes/__tests__/b.test.ts", 2),
    ];
    assert!(duplicate_route_findings(&provides, &Default::default()).is_empty());
}

#[test]
fn duplicate_across_two_prod_files_still_fires_alongside_a_coincidental_test_file_provide() {
    let provides = vec![
        provide("GET /api/users", "src/routes/legacy.ts", 20),
        provide("GET /api/users", "src/routes/users.ts", 3),
        provide("GET /api/users", "src/routes/__tests__/users.test.ts", 99),
    ];
    // two real (non-test) sites remain; the test-file provide must not be counted or anchor the finding.
    let found = duplicate_route_findings(&provides, &Default::default());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file, "src/routes/users.ts");
    assert_eq!(found[0].line, 3);
    assert!(found[0].message.contains("src/routes/legacy.ts:20"));
}

/// BOTH directions of the same-handler skip, in ONE call so neither half can pass vacuously: the
/// idiom the skip exists for stays silent WHILE a production-shaped shadow fires. A test that only
/// proved the silence would still pass if the rule had stopped reporting anything at all.
///
/// - Idiom (silent): gin's `router.POST("", h)` + `router.POST("/", h)` — one handler, one file, two
///   registrations that normalize to one key.
/// - Shadow (fires): two Spring controllers in different modules whose handlers merely share the bare
///   method name `list`. `IoProvide.symbol` is a bare declaration name, so a symbol-only skip read
///   those two as one handler and dropped the finding — measured on macrozheng/mall, `GET /order/list`
///   (`OmsOrderController.list` + `OmsPortalOrderController.list`) was the duplicated key that never
///   reached the report.
#[test]
fn the_trailing_slash_idiom_stays_silent_while_a_cross_file_same_name_pair_fires() {
    let provides = vec![
        provide_sym(
            "POST /api/articles",
            "articles/routers.go",
            15,
            "ArticleCreate",
        ),
        provide_sym(
            "POST /api/articles",
            "articles/routers.go",
            16,
            "ArticleCreate",
        ),
        provide_sym(
            "GET /order/list",
            "mall-admin/src/main/java/com/macro/mall/controller/OmsOrderController.java",
            27,
            "list",
        ),
        provide_sym(
            "GET /order/list",
            "mall-portal/src/main/java/com/macro/mall/portal/controller/OmsPortalOrderController.java",
            72,
            "list",
        ),
    ];
    let found = duplicate_route_findings(&provides, &Default::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].file.as_str(), found[0].line),
        (
            "mall-portal/src/main/java/com/macro/mall/portal/controller/OmsPortalOrderController.java",
            72
        )
    );
    assert!(found[0]
        .message
        .contains("mall-admin/src/main/java/com/macro/mall/controller/OmsOrderController.java:27"));
}

/// The skip needs BOTH halves of its key to say something. Same file + same name is the idiom
/// (silent, pinned above); same file + EMPTY name proves nothing and must keep the warning — an empty
/// symbol is a producer that had no name to report, not evidence that the handler is the same one.
#[test]
fn an_empty_symbol_on_both_sides_is_not_proof_of_the_same_handler() {
    let provides = vec![
        provide_sym("GET /api/users", "src/routes.ts", 3, ""),
        provide_sym("GET /api/users", "src/routes.ts", 9, ""),
    ];
    assert_eq!(
        duplicate_route_findings(&provides, &Default::default()).len(),
        1
    );
}

#[test]
fn same_key_with_different_handlers_still_flags_the_shadow() {
    // Two DIFFERENT handlers on one normalized key is the genuine "which handler wins?" ambiguity.
    let provides = vec![
        provide_sym("GET /api/users", "a.ts", 3, "listUsers"),
        provide_sym("GET /api/users", "b.ts", 9, "legacyListUsers"),
    ];
    let found = duplicate_route_findings(&provides, &Default::default());
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].file.as_str(), found[0].line), ("b.ts", 9));
}

#[test]
fn same_key_with_unknown_symbol_on_one_side_stays_conservative() {
    // Can't prove same handler when a symbol is missing -> keep the warning.
    let provides = vec![
        provide_sym("GET /api/users", "a.ts", 3, "listUsers"),
        provide("GET /api/users", "b.ts", 9),
    ];
    assert_eq!(
        duplicate_route_findings(&provides, &Default::default()).len(),
        1
    );
}

#[test]
fn a_third_divergent_handler_is_flagged_while_the_same_handler_pair_is_not() {
    // Sorted by (file, line): a_routers.go:27 is canonical (A); a_routers.go:28 (A) matches it and is
    // skipped as the tolerance pair; z_legacy.go:5 (B) diverges from the canonical handler and flags.
    let provides = vec![
        provide_sym("GET /api/articles", "a_routers.go", 27, "ArticleList"),
        provide_sym("GET /api/articles", "a_routers.go", 28, "ArticleList"),
        provide_sym("GET /api/articles", "z_legacy.go", 5, "OldArticleList"),
    ];
    let found = duplicate_route_findings(&provides, &Default::default());
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].file.as_str(), found[0].line), ("z_legacy.go", 5));
}

/// The condition-first wording is load-bearing, not cosmetic — see the module doc for the measured
/// monorepo case it exists for. Two things must survive any future edit: the claim must be stated as
/// a CONDITION on same-process registration, and the remedy for the separate-deployment-unit layout
/// must be the tree split (which routes the question to a rule that KNOWS the sources), not
/// suppression. A reader who follows a flat "merge the handlers" across deployment units breaks
/// every service but one.
#[test]
fn the_message_conditions_the_shadowing_claim_and_names_the_tree_split_remedy() {
    let provides = vec![
        provide("GET /api/users", "services/a/routes.ts", 3),
        provide("GET /api/users", "services/b/routes.ts", 9),
    ];
    let found = duplicate_route_findings(&provides, &Default::default());
    assert_eq!(found.len(), 1);
    let m = &found[0].message;
    assert!(
        m.contains("IF both registrations") && m.contains("same running process"),
        "the shadowing claim must be conditional: {m}"
    );
    assert!(
        m.contains("a tree is a directory, not a deployment unit"),
        "the rule must name the evidence it lacks: {m}"
    );
    assert!(
        m.contains("separate `trees[]` entries") && m.contains("cross-layer/duplicate-route"),
        "the remedy for the separate-unit layout must be the tree split, not suppression: {m}"
    );
    assert!(
        m.contains("shared") && m.contains("410"),
        "the tree split's own measured cost must be stated beside it: {m}"
    );
}

fn dirs(items: &[&str]) -> std::collections::BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The mall 0504e86b shape and its control, in ONE call so neither half can pass vacuously. A pair
/// straddling two deployment manifests is reported at `info` — it no longer moves `--fail-on warning`
/// — WHILE a pair inside ONE manifest stays `warning`. A blanket demotion would pass the first half
/// and fail the second; a rule that ignored boundaries entirely would fail the first.
///
/// Measured on macrozheng/mall: four `@SpringBootApplication` classes, four ports, ZERO pom
/// dependencies between the application modules, and 7 of 7 `duplicate-route` findings crossing those
/// module boundaries — every one of them false, and every one of them a `warning` that gated CI on a
/// question this rule holds no evidence for.
#[test]
fn a_cross_boundary_pair_is_info_while_a_same_boundary_pair_stays_warning() {
    let manifests = dirs(&["mall-admin", "mall-portal"]);
    let cross = duplicate_route_findings(
        &[
            provide_sym(
                "GET /order/list",
                "mall-admin/src/main/java/com/macro/mall/controller/OmsOrderController.java",
                27,
                "list",
            ),
            provide_sym(
                "GET /order/list",
                "mall-portal/src/main/java/com/macro/mall/portal/controller/OmsPortalOrderController.java",
                72,
                "list",
            ),
        ],
        &manifests,
    );
    assert_eq!(cross.len(), 1, "{cross:?}");
    assert_eq!(cross[0].severity, zzop_core::Severity::Info);
    assert_eq!(
        cross[0].data.as_ref().unwrap()["manifestBoundaries"]["first"],
        "mall-admin"
    );

    let same = duplicate_route_findings(
        &[
            provide_sym("GET /order/list", "mall-admin/src/a/A.java", 27, "list"),
            provide_sym("GET /order/list", "mall-admin/src/b/B.java", 72, "list"),
        ],
        &manifests,
    );
    assert_eq!(same.len(), 1, "{same:?}");
    assert_eq!(
        same[0].severity,
        zzop_core::Severity::Warning,
        "two sites inside ONE deployment unit are the case this rule CAN judge: {same:?}"
    );
}

/// The demotion rides on a MEASURED difference and on nothing else. An unmeasured side (no manifest
/// above it) and a tree where no manifest was scanned at all both keep the warning — treating absence
/// as evidence would turn "this ecosystem declares its unit in a file we do not read" into a silent
/// downgrade of every finding in the tree.
#[test]
fn an_unmeasured_boundary_never_buys_the_demotion() {
    let one_sided = duplicate_route_findings(
        &[
            provide_sym("GET /x", "packages/api/a.ts", 3, "list"),
            provide_sym("GET /x", "services/legacy/b.go", 9, "List"),
        ],
        &dirs(&["packages/api"]),
    );
    assert_eq!(one_sided.len(), 1);
    assert_eq!(one_sided[0].severity, zzop_core::Severity::Warning);

    let none_scanned = duplicate_route_findings(
        &[
            provide_sym("GET /x", "a/a.ts", 3, "list"),
            provide_sym("GET /x", "b/b.ts", 9, "other"),
        ],
        &Default::default(),
    );
    assert_eq!(none_scanned.len(), 1);
    assert_eq!(none_scanned[0].severity, zzop_core::Severity::Warning);
}

/// A demoted finding must SAY it was demoted and why, right where the reader meets it. A severity that
/// drops with no sentence beside it reads as the rule having changed its mind about the code.
#[test]
fn the_demoted_finding_states_the_demotion_and_names_both_units() {
    let found = duplicate_route_findings(
        &[
            provide_sym("GET /x", "mall-admin/a/A.java", 3, "list"),
            provide_sym("GET /x", "mall-portal/b/B.java", 9, "list"),
        ],
        &dirs(&["mall-admin", "mall-portal"]),
    );
    let m = &found[0].message;
    assert!(m.contains("mall-admin") && m.contains("mall-portal"), "{m}");
    assert!(
        m.contains("`info` rather than `warning`"),
        "the demotion must be stated, not inferred from the severity field: {m}"
    );
    assert!(
        m.contains("NOT cleared"),
        "a demotion is not a dismissal — the finding still stands: {m}"
    );
}

#[test]
fn non_http_provides_are_ignored() {
    let provides = vec![
        zzop_core::IoProvide {
            response: None,
            body: None,
            kind: "queue".to_string(),
            key: "topic".to_string(),
            file: "a.ts".to_string(),
            line: 1,
            symbol: None,
            ..Default::default()
        },
        zzop_core::IoProvide {
            response: None,
            body: None,
            kind: "queue".to_string(),
            key: "topic".to_string(),
            file: "b.ts".to_string(),
            line: 2,
            symbol: None,
            ..Default::default()
        },
    ];
    assert!(duplicate_route_findings(&provides, &Default::default()).is_empty());
}

// ---------------------------------------------------------------------------------------------
// The VERSION axis (`route-version-v1`) — a second disclosure riding beside the manifest one.
// ---------------------------------------------------------------------------------------------

/// The cal.com shape: three controller pairs at one path, split by `@Controller({path, version})` over
/// the `cal-api-version` HEADER, so the version never reaches the URL and every pair reads as a
/// collision. Measured 2026-08-21: 15 of 15 cal.com `duplicate-route` findings are this, all 15 under
/// ONE `apps/api/v2/package.json` so the manifest axis cannot reach them.
///
/// The prescription is what makes this dangerous rather than merely noisy: deleting
/// `BookingsController_2024_04_15` breaks every customer pinned to that version, and merging is
/// inexpressible because the input DTOs differ by design. So the imperative must not be the first
/// thing the reader meets.
#[test]
fn two_sites_declaring_different_version_scopes_are_info_and_drop_the_merge_imperative() {
    let found = duplicate_route_findings(
        &[
            provide_ver(
                "POST /v2/bookings",
                "apps/api/v2/src/modules/bookings/2024-04-15/bookings.controller.ts",
                104,
                "[VERSION_2024_04_15,VERSION_2024_06_11,VERSION_2024_06_14]",
            ),
            provide_ver(
                "POST /v2/bookings",
                "apps/api/v2/src/modules/bookings/2024-08-13/bookings.controller.ts",
                80,
                "VERSION_2024_08_13_VALUE",
            ),
        ],
        &Default::default(),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    let d = found[0].data.as_ref().unwrap();
    assert_eq!(
        d["routeVersions"]["first"],
        "[VERSION_2024_04_15,VERSION_2024_06_11,VERSION_2024_06_14]"
    );
    assert_eq!(d["routeVersions"]["duplicate"], "VERSION_2024_08_13_VALUE");
    let m = &found[0].message;
    assert!(
        m.contains("[VERSION_2024_04_15,VERSION_2024_06_11,VERSION_2024_06_14]")
            && m.contains("VERSION_2024_08_13_VALUE"),
        "both version texts must be named: {m}"
    );
    assert!(
        !m.contains("merge the handlers or remove the duplicate"),
        "merging is inexpressible when the DTOs differ by design: {m}"
    );
}

/// Two controllers claiming the SAME version scope at one path is a real ambiguity — the version
/// discriminator says nothing at all about it, so nothing is demoted.
#[test]
fn the_same_version_on_both_sides_stays_a_warning() {
    let found = duplicate_route_findings(
        &[
            provide_ver("GET /v2/bookings", "a.ts", 3, "VERSION_2024_08_13_VALUE"),
            provide_ver("GET /v2/bookings", "b.ts", 9, "VERSION_2024_08_13_VALUE"),
        ],
        &Default::default(),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
}

/// A version on ONE side is "not measured", never "differs" — exactly how `boundary::disclosure`
/// treats an unmeasured manifest. Manufacturing a difference out of a missing field would demote every
/// finding in a tree whose other producer simply does not emit the field.
#[test]
fn a_version_on_only_one_side_never_buys_the_demotion() {
    let found = duplicate_route_findings(
        &[
            provide_ver("GET /v2/bookings", "a.ts", 3, "VERSION_2024_08_13_VALUE"),
            provide("GET /v2/bookings", "b.ts", 9),
        ],
        &Default::default(),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
}

/// The two axes COMPOSE. A pair that both straddles a manifest boundary AND splits by version must
/// carry both facts in `data` and both sentences in the message — a second axis that overwrote the
/// first would silently delete a disclosure the reader was already relying on.
#[test]
fn the_version_axis_and_the_manifest_axis_compose_rather_than_overwrite() {
    let found = duplicate_route_findings(
        &[
            zzop_core::IoProvide {
                route_version: Some("VERSION_A".to_string()),
                ..provide("GET /x", "packages/api/a.ts", 3)
            },
            zzop_core::IoProvide {
                route_version: Some("VERSION_B".to_string()),
                ..provide("GET /x", "packages/web/b.ts", 9)
            },
        ],
        &dirs(&["packages/api", "packages/web"]),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    let d = found[0].data.as_ref().unwrap();
    assert_eq!(d["manifestBoundaries"]["first"], "packages/api");
    assert_eq!(d["routeVersions"]["duplicate"], "VERSION_B");
    let m = &found[0].message;
    assert!(
        m.contains("Manifest boundaries DIFFER") && m.contains("packages/web"),
        "the manifest sentence must survive the version axis: {m}"
    );
    assert!(
        m.contains("VERSION_A") && m.contains("VERSION_B"),
        "the version sentence must be present too: {m}"
    );
}

/// The COUNT never moves on either axis. Disclosure changes what a finding says and which side of the
/// gate it sits on; it never erases one. Every version fixture above yields exactly the one finding the
/// pre-change rule produced for the same two sites.
#[test]
fn the_version_axis_never_changes_the_finding_count() {
    let cases: [(&str, Option<&str>, Option<&str>); 4] = [
        ("differing", Some("V_A"), Some("V_B")),
        ("identical", Some("V_A"), Some("V_A")),
        ("one-sided", Some("V_A"), None),
        ("absent", None, None),
    ];
    for (label, a, b) in cases {
        for manifests in [Default::default(), dirs(&["packages/api", "packages/web"])] {
            let found = duplicate_route_findings(
                &[
                    zzop_core::IoProvide {
                        route_version: a.map(str::to_string),
                        ..provide("GET /x", "packages/api/a.ts", 3)
                    },
                    zzop_core::IoProvide {
                        route_version: b.map(str::to_string),
                        ..provide("GET /x", "packages/web/b.ts", 9)
                    },
                ],
                &manifests,
            );
            assert_eq!(found.len(), 1, "{label}: {found:?}");
        }
    }
}

/// A corpus-real TRUE POSITIVE that must keep firing at `warning`. nocodb
/// `packages/nocodb/src/controllers/extensions.controller.ts` registers a `:baseId` list route at :26
/// and a `:extensionId` read route at :52 — same class, same file, different handlers, `@Controller()`
/// with no version at all. Both normalize to one key, Nest matches the first for every request, and the
/// second handler is genuinely dead. Losing this is the failure mode the version axis must not cause.
#[test]
fn a_real_shadow_in_an_unversioned_controller_still_fires_at_warning() {
    let found = duplicate_route_findings(
        &[
            provide_sym(
                "GET /api/v2/extensions/{}",
                "packages/nocodb/src/controllers/extensions.controller.ts",
                26,
                "extensionList",
            ),
            provide_sym(
                "GET /api/v2/extensions/{}",
                "packages/nocodb/src/controllers/extensions.controller.ts",
                52,
                "extensionRead",
            ),
        ],
        &Default::default(),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 52);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
}

/// The same shadow INSIDE one versioned controller: both sites carry the SAME version scope, so the
/// discriminator separates nothing and the genuine "which handler wins?" ambiguity is intact.
#[test]
fn a_real_shadow_inside_one_versioned_controller_stays_a_warning() {
    let found = duplicate_route_findings(
        &[
            zzop_core::IoProvide {
                route_version: Some("VERSION_2024_08_13_VALUE".to_string()),
                ..provide_sym(
                    "GET /v2/bookings/{}",
                    "bookings.controller.ts",
                    26,
                    "getOne",
                )
            },
            zzop_core::IoProvide {
                route_version: Some("VERSION_2024_08_13_VALUE".to_string()),
                ..provide_sym(
                    "GET /v2/bookings/{}",
                    "bookings.controller.ts",
                    52,
                    "getUid",
                )
            },
        ],
        &Default::default(),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 52);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
}
