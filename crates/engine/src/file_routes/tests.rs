//! Orchestration coverage: verb-export wiring, pages/api AST-scan wiring, the Remix
//! resource-route gate, and the test/fixture skip. Path→URL transforms are tested per-submodule.
use super::*;
use zzop_core::SourceSymbolKind;

fn sym(file: &str, name: &str, line: u32, is_default: bool) -> SourceSymbol {
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name: name.into(),
        kind: SourceSymbolKind::Const,
        line,
        exported: true,
        is_default,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

fn no_text(_: &str) -> Option<String> {
    None
}

/// T2 pin (rule-quality.md §6) over the ONE divergence this module deliberately keeps from
/// `dead_exports::is_ts_source_ext`. A T1 shared symbol is not available here — the sets answer
/// different questions ("does the framework route it" vs "can we parse it"), so sharing one symbol
/// would be wrong, not merely inconvenient. What must never happen is the divergence becoming
/// SILENT, which is precisely the state this pin was added to end.
///
/// Both directions are checked, and both have teeth:
/// - **Subset**: a routed extension the TypeScript frontend cannot parse would mint a PROVIDE for a
///   file no extractor ever reads — the route's verbs would come from an empty symbol set.
/// - **Exact delta**: the difference is `{mts, cts}` and nothing else. Growing the route list to
///   cover them (the drift an outside reviewer flagged as possibly unintentional) goes red here and
///   must be argued against [`super::ROUTE_EXTENSIONS`]'s recorded framework-default reasoning;
///   shrinking it, or growing the dispatch set, goes red for the same reason.
#[test]
fn route_extensions_are_a_declared_subset_of_the_typescript_dispatch_set() {
    use crate::dead_exports::is_ts_source_ext;

    for ext in super::ROUTE_EXTENSIONS {
        assert!(
            is_ts_source_ext(&format!("x.{ext}")),
            "ROUTE_EXTENSIONS routes {ext:?}, but the TypeScript frontend does not claim it — the \
             convention would provide a route for a file no extractor reads"
        );
    }

    // Enumerated from `is_ts_source_ext`'s own match arm (it is a predicate, not an iterable set);
    // if that arm changes, this list must change with it and the loop below re-justified.
    const TYPESCRIPT_DISPATCH_EXTENSIONS: [&str; 8] =
        ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];
    /// The deliberate, documented delta — see [`super::ROUTE_EXTENSIONS`]: no supported file-routing
    /// convention's DEFAULT extension list contains these two.
    const NOT_ROUTED_BY_ANY_CONVENTION: [&str; 2] = ["mts", "cts"];

    for ext in TYPESCRIPT_DISPATCH_EXTENSIONS {
        assert!(
            is_ts_source_ext(&format!("x.{ext}")),
            "this pin's hand-copy of is_ts_source_ext's match arm has gone stale: it lists {ext:?}, \
             which is_ts_source_ext no longer accepts"
        );
        assert_eq!(
            super::is_route_extension(ext),
            !NOT_ROUTED_BY_ANY_CONVENTION.contains(&ext),
            "the routed/parseable delta must stay exactly {NOT_ROUTED_BY_ANY_CONVENTION:?}; {ext:?} \
             broke it. Re-justify BOTH sides together — see ROUTE_EXTENSIONS's doc for the \
             framework-default evidence that set the delta."
        );
    }
}

/// The `route.<ext>` filename gate and the `pages/api` stem gate must accept the same extensions —
/// they now read one constant, and this holds them to it from the outside (a future convention that
/// re-spells its own list gets caught here).
#[test]
fn both_route_filename_gates_agree_on_the_extension_set() {
    for ext in [
        "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "json", "md",
    ] {
        let routed = super::is_route_extension(ext);
        assert_eq!(
            super::is_route_module_filename(&format!("route.{ext}")),
            routed,
            "route.{ext} filename gate disagrees with is_route_extension"
        );
        assert_eq!(
            super::next::pages_api_route(&format!("pages/api/x.{ext}")).is_some(),
            routed,
            "pages/api/x.{ext} stem gate disagrees with is_route_extension"
        );
    }
    // ...and the gate is a suffix test on a literal `route.` stem, not a substring one.
    assert!(!super::is_route_module_filename("myroute.ts"));
    assert!(!super::is_route_module_filename("route.ts.bak"));
    assert!(!super::is_route_module_filename("route."));
}

#[test]
fn medusa_verb_exports_become_http_provides() {
    let rel = "packages/medusa/src/api/admin/campaigns/[id]/route.ts";
    let symbols = vec![sym(rel, "GET", 10, false), sym(rel, "POST", 40, false)];
    let out = compose_file_convention_provides([rel], &symbols, &no_text);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].key, "GET /admin/campaigns/{}");
    assert_eq!(out[0].kind, "http");
    assert_eq!(out[0].line, 10);
    assert_eq!(out[0].symbol.as_deref(), Some("GET"));
    assert_eq!(out[1].key, "POST /admin/campaigns/{}");
}

#[test]
fn non_verb_exports_on_route_modules_emit_nothing() {
    let rel = "packages/medusa/src/api/admin/campaigns/route.ts";
    let symbols = vec![
        sym(rel, "AUTHENTICATE", 3, false),
        sym(rel, "config", 5, false),
    ];
    let out = compose_file_convention_provides([rel], &symbols, &no_text);
    assert!(out.is_empty());
}

#[test]
fn fixture_and_test_paths_are_skipped() {
    let fixture = "integration-tests/http/__fixtures__/x/src/api/admin/route.ts";
    let test = "apps/web/pages/api/book/recurring-event.test.ts";
    let symbols = vec![sym(fixture, "GET", 1, false)];
    let out = compose_file_convention_provides([fixture, test], &symbols, &|_| {
        Some("export default handler;".into())
    });
    assert!(out.is_empty());
}

#[test]
fn app_router_verb_exports_become_http_provides() {
    let rel = "apps/web/app/api/cancel/route.ts";
    let symbols = vec![sym(rel, "POST", 7, false), sym(rel, "DELETE", 9, false)];
    let out = compose_file_convention_provides([rel], &symbols, &no_text);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].key, "DELETE /api/cancel");
    assert_eq!(out[1].key, "POST /api/cancel");
}

#[test]
fn pages_api_uses_scanned_verb_hints() {
    let rel = "apps/web/pages/api/book/event.ts";
    let text = concat!(
        "async function handler(req, res) {\n",
        "  if (req.method !== \"POST\") return res.status(405).end();\n",
        "}\n",
        "export default handler;\n",
    );
    let out =
        compose_file_convention_provides([rel], &[], &|r| (r == rel).then(|| text.to_string()));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "POST /api/book/event");
    assert_eq!(out[0].line, 4);
    assert_eq!(out[0].symbol.as_deref(), Some("default"));
}

#[test]
fn pages_api_without_method_literals_emits_unknown_verb_sentinel() {
    let rel = "apps/web/pages/api/auth/verify-email.ts";
    let out =
        compose_file_convention_provides([rel], &[], &|_| Some("export default handler;\n".into()));
    // A serve-all handler naming no method literal emits ONE UNKNOWN_VERB sentinel (`?`), not a
    // fabricated GET+POST pair — the engine partitions it into `cross-layer/unknown-verb-route`.
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["? /api/auth/verify-email"]);
}

#[test]
fn remix_resource_route_maps_loader_and_action() {
    let rel = "apps/remix/app/routes/api+/stripe.webhook.ts";
    let symbols = vec![sym(rel, "action", 12, false)];
    let out = compose_file_convention_provides([rel], &symbols, &no_text);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key, "POST /api/stripe/webhook");
    assert_eq!(out[0].symbol.as_deref(), Some("action"));
}

#[test]
fn remix_ui_page_with_default_export_is_not_a_provide() {
    let rel = "apps/remix/app/routes/_authenticated+/dashboard.tsx";
    let symbols = vec![
        sym(rel, "loader", 5, false),
        sym(rel, "Dashboard", 20, true),
    ];
    let out = compose_file_convention_provides([rel], &symbols, &no_text);
    assert!(out.is_empty());
}

/// Policy-value set-equality pin (T2): `HTTP_VERB_EXPORTS` (which export NAMES count as verb
/// handlers in file-convention routing — deliberately omits HEAD/OPTIONS, see its doc) and
/// core's `HTTP_KEY_VERBS` (the name-inferred verb keying vocabulary) are DIFFERENT policy
/// domains that today hold the same 5-verb set. If either grows or shrinks deliberately
/// (e.g. core learns HEAD), this pin forces the divergence to be decided rather than drift.
#[test]
fn http_verb_exports_matches_core_key_verbs_set() {
    let mut exports: Vec<&str> = HTTP_VERB_EXPORTS.to_vec();
    let mut core: Vec<&str> = zzop_core::HTTP_KEY_VERBS.to_vec();
    exports.sort_unstable();
    core.sort_unstable();
    assert_eq!(
        exports, core,
        "HTTP_VERB_EXPORTS and zzop_core::HTTP_KEY_VERBS hold the same verb set today; a \
         deliberate change to either must be re-justified here (policy set-equality pin, T2)"
    );
}

#[test]
fn remix_default_expr_page_is_caught_by_lexical_fallback() {
    // `export default memo(Page)` produces no `parse_symbols` default symbol — the re-read
    // lexical check is what keeps this UI page out of the provide surface.
    let rel = "apps/remix/app/routes/api+/pretend.ts";
    let symbols = vec![sym(rel, "loader", 5, false)];
    let out = compose_file_convention_provides([rel], &symbols, &|_| {
        Some("export default memo(Page);".into())
    });
    assert!(out.is_empty());
}
