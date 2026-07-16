//! Orchestration coverage: verb-export wiring, pages/api lexical-scan wiring, the Remix
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
fn pages_api_without_method_literals_falls_back_to_get_post() {
    let rel = "apps/web/pages/api/auth/verify-email.ts";
    let out =
        compose_file_convention_provides([rel], &[], &|_| Some("export default handler;\n".into()));
    let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(
        keys,
        vec!["GET /api/auth/verify-email", "POST /api/auth/verify-email"]
    );
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

/// Policy-value equality pin (T2): `PAGES_API_FALLBACK_VERBS` here and
/// `zzop_parser_typescript::PATHNAME_DISPATCH_FALLBACK_VERBS` encode the same policy decision
/// (the verb set assumed for a handler that names no method) but live on opposite sides of a
/// crate boundary where symbol sharing is impossible. This pin forces a deliberate change on
/// either side to be re-justified against the other rather than silently drifting apart.
#[test]
fn pathname_dispatch_fallback_verbs_pin() {
    assert_eq!(
        PAGES_API_FALLBACK_VERBS,
        zzop_parser_typescript::PATHNAME_DISPATCH_FALLBACK_VERBS,
        "PAGES_API_FALLBACK_VERBS and PATHNAME_DISPATCH_FALLBACK_VERBS must stay in lockstep \
         (policy-value equality pin, T2)"
    );
}

/// Same T2 equality pin for the Go side: parser-go cannot depend on this crate, so its
/// `GO_HANDLEFUNC_FALLBACK_VERBS` (the verbs assumed for a `HandleFunc` pattern naming no
/// method) carried documented parity only — this engine-side test (the dependency edge DOES
/// exist in this direction) upgrades that prose to an executable pin (opus review F2).
#[test]
fn go_handlefunc_fallback_verbs_pin() {
    assert_eq!(
        PAGES_API_FALLBACK_VERBS,
        zzop_parser_go::adapters::net_http::GO_HANDLEFUNC_FALLBACK_VERBS,
        "PAGES_API_FALLBACK_VERBS and GO_HANDLEFUNC_FALLBACK_VERBS must stay in lockstep \
         (policy-value equality pin, T2)"
    );
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

/// Policy-value subset pin (T2): the no-method fallback verbs must always be drawn from the
/// core verb vocabulary — a fallback verb core cannot key would mint unjoinable provides.
#[test]
fn pages_api_fallback_verbs_are_a_subset_of_core_key_verbs() {
    for v in PAGES_API_FALLBACK_VERBS {
        assert!(
            zzop_core::HTTP_KEY_VERBS.contains(&v),
            "fallback verb {v} is not in zzop_core::HTTP_KEY_VERBS (policy subset pin, T2)"
        );
    }
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
