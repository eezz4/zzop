//! `mutating-route-no-auth` — flags a POST/PUT/PATCH/DELETE `IoProvide` (an HTTP route) whose handler
//! symbol, walked via call-graph BFS (`zzop_core::callgraph::bfs_reachable` over the whole-repo
//! `SymbolGraph`), never reaches a callee whose NAME looks like an auth guard. Unlike the DSL
//! `http/auth-gates` rule (`rules/dsl/http/http.json`), which only inspects the registration line's handler
//! identifier text, this rule follows the handler's actual downstream calls.
//!
//! ## Guard vocabulary
//! [`DEFAULT_AUTH_GUARD_PATTERN`] is matched against the TAIL name (after the last `#`/`.`) of every symbol
//! id `bfs_reachable` visits — a name-vocabulary check, not a body inspector. `access` is guarded to
//! `(has|can|check|require)access` only, since bare `access` also clears on non-auth names like
//! `accessLog`/`dataAccess`. A blanket `require[A-Z]\w*` fragment was considered and rejected: it would
//! clear pure input-validation middleware (`requireBody`/`requireJson`/`requireQuery`) and silently
//! suppress genuine missing-auth findings; for a security rule the recall loss outweighs the
//! false-positive savings, and `requireXxx` helpers with a real auth stem (`requireAuth`, `requireOwner`)
//! still match via those stems. A real guard named outside this vocabulary still false-positives here —
//! the finding message points at config `rules: { "mutating-route-no-auth": "off" }` (embedders:
//! `disabled_rules`) as the escape hatch, since this rule has no inline suppression marker.
//!
//! ## Decidable subset
//! Only mutating-method provides whose `symbol` resolves to a UNIQUE known symbol are checked
//! (`http_scan::resolve_handler`'s "do not guess" contract) — an unresolved or ambiguous handler is
//! skipped. `bfs_reachable` can match the handler's own symbol id at depth 0, so a self-describing handler
//! name alone is enough to clear this check.
//!
//! ## Precision limit (and its injection completion)
//! This is a vocabulary-based reachability check over the CALL graph only. Route-level middleware —
//! `app.post("/x", requireAuth, handler)`, or a router-level `.use(authMiddleware)` — never appears as a
//! call edge FROM the handler symbol itself, so it is invisible to this rule: a route guarded exclusively
//! via middleware will false-positive. Severity starts at [`Severity::Info`] because of this.
//!
//! Middleware is a per-project environment fact the native call-graph can't see — so, per zzop's design
//! line (native sees the common case; everything else is injected), it is COMPLETED BY INJECTION rather
//! than by ever-growing native middleware modeling. A producer/adapter that understands a project's
//! middleware injects an [`AUTH_GUARDED_ATTR`] attribute on the guarded route (an `IoKey`) or router
//! prefix (a `PathScope`) through the generic entity-attribute channel (`zzop_core::AttributeStore`,
//! [`ScanMutatingRouteNoAuthInput::route_attr_store`]); the native vocab BFS and the injected evidence
//! COMPOSE (either clears the route). This is one consumer of a general channel, not a bespoke auth path.
//!
//! ## Auth-acquisition exemption
//! A provide whose PATH sits on the auth-acquisition surface is exempt entirely, never entering the BFS —
//! that surface IS how a caller gets credentials, so it cannot require pre-existing auth to reach itself.
//! Two tiers, since some acquisition-shaped words also legitimately name unrelated mutating routes (e.g.
//! `POST /devices/register`):
//! - **Standalone tier** ([`AUTH_ACQUISITION_STANDALONE_PATTERN`]): exempt unconditionally — these segments
//!   ARE the auth surface regardless of what else is in the path.
//! - **Conditional tier** ([`AUTH_ACQUISITION_CONDITIONAL_PATTERN`]): exempt only when an auth-family
//!   segment ([`AUTH_FAMILY_PATH_PATTERN`]) also appears in the same path — e.g. `/auth/register` is
//!   exempt, but `/devices/register` is not.
//!
//! Every segment list is matched `/`-delimited on whole path segments only, never as a bare substring —
//! `/author/profile` does not match `auth`.
//!
//! ## Test-fixture exemption
//! A provide registered in a test/fixture file (`is_test_file` — the same predicate `unreachable`'s
//! dead-island check uses) is skipped outright: a route only ever defined and invoked from a test is not
//! exposed application surface.
//!
//! ## NestJS `@UseGuards` decorator exemption
//! A provide extracted by the NestJS adapter whose registration line carries `@UseGuards(...)` coverage
//! (class- or method-level) is exempt from the BFS entirely. NestJS's guard chain runs BEFORE the handler
//! regardless of what the handler's own body calls, so the BFS's core assumption — the guard must be
//! REACHABLE FROM the handler — doesn't apply to decorator-based auth: a decorator application is metadata,
//! not a call edge (the same structural blind spot as route-level middleware, just a different guise of
//! it). The exemption set is a side-channel `HashSet<(file, line)>` — see
//! [`ScanMutatingRouteNoAuthInput::nest_guarded`] — computed independently by
//! `zzop_parser_typescript::extract_controller_guarded_lines` and matched against each provide's own
//! `(file, line)`.
//!
//! **Residual (not fixed here):** NestJS global guards (`app.useGlobalGuards(...)` in `main.ts`, or an
//! `APP_GUARD` provider in a `*.module.ts`) apply to every route in the application, but are a file-level
//! signal that cannot be mapped back to specific routes from the extractor's per-file, per-controller view
//! — see `controller_decorators.rs`'s own module doc for the same residual. A controller relying ENTIRELY
//! on a global guard (no local `@UseGuards`) still false-positives on this rule.

use std::collections::HashMap;

use regex::Regex;
use zzop_core::callgraph::{bfs_reachable, SymbolGraph};
use zzop_core::{disable_hint, Finding, Severity, SourceSymbol};

use crate::http_scan::{build_name_index, resolve_handler};
use zzop_core::is_test_file;

/// Default guard-name vocabulary — see module doc "Guard vocabulary".
pub const DEFAULT_AUTH_GUARD_PATTERN: &str = r"(?i)(auth|guard|verify|session|token|permission|acl|owner|admin|role|(?:has|can|check|require)access|is(?:local|dev|production))";

/// The attribute key this rule reads off the generic entity-attribute channel (`zzop_core::AttributeStore`)
/// to clear a route it cannot see a guard for. A producer/adapter that understands a project's middleware
/// (route-level middleware, a router-wide `.use(authMiddleware)`, a framework guard the call-graph BFS
/// can't reach) injects `{ target: <route IoKey | PathScope>, key: "auth-guarded", value: true }`. This is
/// the injection completion of the "Precision limit" below — native sees the vocab guards it can, the
/// adapter completes the middleware layer, and the two compose (either clears the route). This literal is
/// RULE vocabulary, never the kernel's — the store is queried by key, agnostic to what it means.
pub const AUTH_GUARDED_ATTR: &str = "auth-guarded";

/// Auth-acquisition exemption, standalone tier — see module doc "Auth-acquisition exemption".
const AUTH_ACQUISITION_STANDALONE_PATTERN: &str = r"(?i)/(auth|login|logout|signin|signup)(/|$)";

/// Auth-acquisition exemption, conditional tier — exempt only alongside [`AUTH_FAMILY_PATH_PATTERN`]. See
/// module doc.
const AUTH_ACQUISITION_CONDITIONAL_PATTERN: &str =
    r"(?i)/(register|token|refresh|password|otp)(/|$)";

/// Auth-family gate for the conditional exemption tier — see module doc.
const AUTH_FAMILY_PATH_PATTERN: &str = r"(?i)/(auth|login|signin|signup|session|oauth)(/|$)";

use crate::http_scan::WRITE_HTTP_METHODS;

/// Input for [`scan_mutating_route_no_auth`]. Takes `io_provides` directly (not the `ApiEndpoint` shape
/// `http_scan`'s two rules take) so the emitted `Finding` can anchor on the route's own registration
/// `file`/`line` — `ApiEndpoint` carries no line number (see `zzop_engine::io`'s module doc, "`ApiEndpoint`
/// has no line number"), and this rule's problem IS the route registration, not a downstream write site.
pub struct ScanMutatingRouteNoAuthInput<'a> {
    pub io_provides: &'a [zzop_core::IoProvide],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    pub auth_guard_pattern: &'a str,
    /// NestJS decorator-based auth coverage (`@UseGuards(...)` at class or method level) — see module doc
    /// "NestJS `@UseGuards` decorator exemption". `(file, line)` pairs matching an `IoProvide`'s own
    /// `file`/`line` are exempt from the BFS entirely, the same way the test-fixture and
    /// auth-acquisition-path exemptions already are: this IS how the route is guarded, just via a
    /// decorator the call-graph BFS structurally cannot see (decorator application is metadata, not a
    /// call edge — see "Precision limit" above). Pass an empty set for a non-Nest tree, or when the
    /// caller doesn't compute this exemption — old behavior (no exemption) is preserved.
    pub nest_guarded: &'a std::collections::HashSet<(String, u32)>,
    /// Injected auth-guard evidence from the generic entity-attribute channel — a route whose
    /// [`AUTH_GUARDED_ATTR`] attribute resolves truthy (an exact `IoKey`, or a `PathScope` prefix a
    /// middleware guards) is exempt, the injection completion of the middleware "Precision limit". Pass an
    /// empty store (`&AttributeStore::default()`) when nothing is injected — old behavior is preserved.
    pub route_attr_store: &'a zzop_core::AttributeStore,
}

pub fn scan_mutating_route_no_auth(input: &ScanMutatingRouteNoAuthInput) -> Vec<Finding> {
    let standalone_re = Regex::new(AUTH_ACQUISITION_STANDALONE_PATTERN).unwrap();
    let conditional_re = Regex::new(AUTH_ACQUISITION_CONDITIONAL_PATTERN).unwrap();
    let auth_family_re = Regex::new(AUTH_FAMILY_PATH_PATTERN).unwrap();
    let is_auth_acquisition_exempt = |path: &str| -> bool {
        standalone_re.is_match(path)
            || (conditional_re.is_match(path) && auth_family_re.is_match(path))
    };
    let mutating: Vec<&zzop_core::IoProvide> = input
        .io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter(|p| !is_test_file(&p.file))
        .filter(|p| !input.nest_guarded.contains(&(p.file.clone(), p.line)))
        // Injected auth-guard evidence (route-level middleware the call-graph BFS can't see) — see
        // `AUTH_GUARDED_ATTR`. Exempt BEFORE the BFS, like `nest_guarded`: this IS how the route is guarded.
        .filter(|p| {
            !input
                .route_attr_store
                .route_attr(&p.kind, &p.key, AUTH_GUARDED_ATTR)
                .is_some_and(zzop_core::attr_is_truthy)
        })
        .filter(|p| {
            let Some((method, path)) = p.key.split_once(' ') else {
                return false;
            };
            // The auth-acquisition surface itself is exempt — see module doc.
            WRITE_HTTP_METHODS.contains(&method) && !is_auth_acquisition_exempt(path)
        })
        .collect();
    if mutating.is_empty() {
        return Vec::new();
    }

    let name_index = build_name_index(input.symbols);
    let guard_re = Regex::new(input.auth_guard_pattern)
        .unwrap_or_else(|_| Regex::new(DEFAULT_AUTH_GUARD_PATTERN).unwrap());
    let is_guard_id = |id: &str| -> bool {
        let tail = id.rsplit(['#', '.']).next().unwrap_or(id);
        guard_re.is_match(tail)
    };

    // Memoizes the per-handler BFS across every mutating endpoint sharing a handler symbol.
    let cache: std::cell::RefCell<HashMap<String, bool>> = std::cell::RefCell::new(HashMap::new());
    let reaches_guard = |handler_symbol: &str| -> bool {
        if let Some(hit) = cache.borrow().get(handler_symbol) {
            return *hit;
        }
        let found =
            bfs_reachable(input.symbol_graph, handler_symbol, |id| is_guard_id(id)).is_some();
        cache.borrow_mut().insert(handler_symbol.to_string(), found);
        found
    };

    let mut out = Vec::new();
    for p in mutating {
        let Some(handler_ref) = p.symbol.as_deref() else {
            continue; // no handler reference captured — cannot resolve, do not guess
        };
        let Some((method, path)) = p.key.split_once(' ') else {
            continue;
        };
        let Some(handler_symbol) = resolve_handler(handler_ref, &name_index) else {
            continue; // unresolved/ambiguous handler — do not guess
        };
        if reaches_guard(&handler_symbol) {
            continue;
        }
        let hint = format!(
            "{method} {path} (handler `{handler_ref}`) never reaches a call whose name looks like an auth \
             guard ({}) anywhere in its call graph — this mutating route may be missing an authorization \
             check. Add an explicit, named guard call reachable from the handler (e.g. requireAuth(), \
             verifySession()), or confirm auth is actually enforced. Exemption: routes whose path is itself \
             on the auth-acquisition surface are never checked by this rule, since that surface cannot \
             require pre-existing auth to reach itself — either a standalone segment (`/auth/...`, \
             `/login`, `/logout`, `/signin`, `/signup`), or a segment like `/register`, `/token`, \
             `/refresh`, `/password`, `/otp` PAIRED WITH an auth-family segment elsewhere in the same path \
             (e.g. `/auth/register` is exempt, but `/devices/register` is NOT — `register` alone isn't \
             enough). A route registered in a test/fixture file (`__tests__/`, `__test__/`, `tests?/`, \
             `spec/`, `*.test.*`, `*.spec.*`, and similar per-language conventions) is also never checked — \
             a route only ever defined/called from a test is not exposed application surface. \
             Precision limit: this is a call-graph-BFS, vocabulary-based check — route-level middleware (e.g. \
             `apiRoutes.post(\"{path}\", requireAuth, {handler_ref})`, or a router-wide `.use(authMiddleware)`) \
             never appears as a call FROM the handler itself, so it is invisible to this check and WILL \
             false-positive on a route guarded only that way — this finding starts at Info severity until \
             this check becomes middleware-aware. {} if your auth happens at the middleware layer (this \
             rule has no inline suppression marker).",
            input.auth_guard_pattern,
            disable_hint("mutating-route-no-auth")
        );
        out.push(Finding {
            rule_id: "mutating-route-no-auth".to_string(),
            severity: Severity::Info,
            file: p.file.clone(),
            line: p.line,
            message: hint.clone(),
            data: Some(serde_json::json!({
                "method": method,
                "path": path,
                "handler": handler_ref,
                "handlerSymbol": handler_symbol,
                "hint": hint,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    //! Unit tests for `scan_mutating_route_no_auth`'s BFS + guard-vocabulary logic in isolation (e2e coverage
    //! — real handler-file fixtures — lives in `crates/engine/tests/analyze_io_natives.rs`).
    use super::*;
    use zzop_core::callgraph::SymbolEdge;
    use zzop_core::SourceSymbolKind;

    fn sym(file: &str, name: &str, line: u32) -> SourceSymbol {
        SourceSymbol {
            id: format!("{file}#{name}"),
            file: file.to_string(),
            name: name.to_string(),
            kind: SourceSymbolKind::Function,
            line,
            exported: true,
            is_default: false,
            body_start: Some(line),
            body_end: Some(line),
            write_sites: Vec::new(),
        }
    }

    fn provide(key: &str, file: &str, line: u32, handler: &str) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: Some(handler.to_string()),
        }
    }

    fn edge(from: &str, to: &str) -> SymbolEdge {
        SymbolEdge {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    #[test]
    fn mutating_handler_never_reaching_a_guard_is_flagged() {
        let provides = vec![provide("POST /users", "routes/api.ts", 3, "createUser")];
        let symbols = vec![sym("routes/handlers.ts", "createUser", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file, "routes/api.ts");
        assert_eq!(out[0].line, 3);
        assert_eq!(out[0].rule_id, "mutating-route-no-auth");
        assert_eq!(out[0].severity, Severity::Info);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["method"], "POST");
        assert_eq!(data["path"], "/users");
    }

    #[test]
    fn auth_acquisition_route_is_exempt_even_when_never_guarded() {
        let provides = vec![provide(
            "POST /api/auth/register",
            "routes/api.ts",
            3,
            "register",
        )];
        let symbols = vec![sym("routes/handlers.ts", "register", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn standalone_exempt_segment_is_exempt_alone_with_no_auth_family_segment_present() {
        let provides = vec![provide("POST /signup", "routes/api.ts", 3, "signup")];
        let symbols = vec![sym("routes/handlers.ts", "signup", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn conditional_segment_paired_with_an_auth_family_segment_is_exempt() {
        // /auth/register — "register" (conditional tier) paired with "auth" (auth-family) elsewhere in the
        // same path is exempt.
        let provides = vec![provide(
            "POST /auth/register",
            "routes/api.ts",
            3,
            "register",
        )];
        let symbols = vec![sym("routes/handlers.ts", "register", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn conditional_segment_alone_with_no_auth_family_segment_is_not_exempt() {
        // /devices/register — "register" alone (no auth-family segment anywhere in the path) is over-broad
        // to exempt: a device-registration endpoint has nothing to do with authentication, so this route is
        // checked normally.
        let provides = vec![provide(
            "POST /devices/register",
            "routes/api.ts",
            3,
            "registerDevice",
        )];
        let symbols = vec![sym("routes/handlers.ts", "registerDevice", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1, "{:?}", out);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["path"], "/devices/register");
    }

    #[test]
    fn conditional_segment_token_refresh_with_no_auth_family_segment_is_not_exempt() {
        // /token/refresh — both segments are conditional-tier; with no auth-family segment present, this
        // route is checked normally rather than assumed to be the auth-acquisition surface. Handler name
        // deliberately avoids any guard-vocabulary substring (`auth`/`guard`/`verify`/`session`/`token`/
        // `permission`/`acl`) so this isolates the PATH exemption from the separate guard-NAME match.
        let provides = vec![provide(
            "POST /token/refresh",
            "routes/api.ts",
            3,
            "renewCredentials",
        )];
        let symbols = vec![sym("routes/handlers.ts", "renewCredentials", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1, "{:?}", out);
    }

    #[test]
    fn a_path_segment_that_only_contains_auth_as_a_substring_is_not_exempt() {
        // Handler name deliberately avoids an "auth" substring (unlike the path) so this isolates the
        // PATH-segment exemption from the separate, unrelated guard-vocabulary name match that would
        // independently clear a handler literally named e.g. `updateAuthorProfile` at BFS depth 0.
        let provides = vec![provide(
            "POST /author/profile",
            "routes/api.ts",
            3,
            "patchWriterBio",
        )];
        let symbols = vec![sym("routes/handlers.ts", "patchWriterBio", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1, "{:?}", out);
    }

    #[test]
    fn handler_reaching_a_guard_call_across_an_edge_is_not_flagged() {
        let provides = vec![provide("POST /users", "routes/api.ts", 3, "createUser")];
        let symbols = vec![
            sym("routes/handlers.ts", "createUser", 1),
            sym("routes/handlers.ts", "requireAuth", 2),
        ];
        let graph = vec![edge(
            "routes/handlers.ts#createUser",
            "routes/handlers.ts#requireAuth",
        )];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &graph,
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn handler_named_like_a_guard_itself_clears_at_depth_zero() {
        let provides = vec![provide(
            "DELETE /users/{}",
            "routes/api.ts",
            4,
            "deleteUserWithAuthCheck",
        )];
        let symbols = vec![sym("routes/handlers.ts", "deleteUserWithAuthCheck", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn safe_methods_are_never_checked() {
        let provides = vec![provide("GET /users", "routes/api.ts", 3, "listUsers")];
        let symbols = vec![sym("routes/handlers.ts", "listUsers", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn ambiguous_handler_name_defined_in_two_files_is_skipped() {
        let provides = vec![provide("POST /dup", "routes/api.ts", 3, "dup")];
        let symbols = vec![sym("a.ts", "dup", 1), sym("b.ts", "dup", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn provide_with_no_symbol_captured_is_skipped() {
        let provides = vec![zzop_core::IoProvide {
            body: None,
            kind: "http".to_string(),
            key: "POST /anon".to_string(),
            file: "routes/api.ts".to_string(),
            line: 3,
            symbol: None,
        }];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &[],
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn route_registered_in_a_test_fixture_file_is_skipped() {
        let provides = vec![provide(
            "POST /users",
            "routes/__tests__/api.test.ts",
            3,
            "createUser",
        )];
        let symbols = vec![sym("routes/__tests__/api.test.ts", "createUser", 1)];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty(), "{:?}", out);
    }

    #[test]
    fn handler_reaching_a_require_prefixed_ownership_guard_is_not_flagged() {
        let provides = vec![provide(
            "DELETE /guilds/{}",
            "routes/api.ts",
            3,
            "deleteGuild",
        )];
        let symbols = vec![
            sym("routes/handlers.ts", "deleteGuild", 1),
            sym("routes/handlers.ts", "requireGuildOwner", 2),
        ];
        let graph = vec![edge(
            "routes/handlers.ts#deleteGuild",
            "routes/handlers.ts#requireGuildOwner",
        )];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &graph,
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty(), "{:?}", out);
    }

    #[test]
    fn handler_reaching_only_input_validation_require_helpers_is_still_flagged() {
        // `requireBody`/`requireJson` are input-validation middleware, not auth — a blanket
        // `require[A-Z]\w*` recognizer would silently clear this genuine missing-auth case.
        // Only auth-stemmed names may clear.
        let provides = vec![provide("POST /users", "routes/api.ts", 3, "createUser")];
        let symbols = vec![
            sym("routes/handlers.ts", "createUser", 1),
            sym("routes/handlers.ts", "requireBody", 2),
            sym("routes/handlers.ts", "requireJson", 3),
        ];
        let graph = vec![
            edge(
                "routes/handlers.ts#createUser",
                "routes/handlers.ts#requireBody",
            ),
            edge(
                "routes/handlers.ts#createUser",
                "routes/handlers.ts#requireJson",
            ),
        ];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &graph,
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1, "{:?}", out);
        assert_eq!(out[0].file, "routes/api.ts");
    }

    #[test]
    fn handler_reaching_an_is_local_env_gate_is_not_flagged() {
        let provides = vec![provide(
            "POST /debug/reset",
            "routes/api.ts",
            3,
            "resetDebugState",
        )];
        let symbols = vec![
            sym("routes/handlers.ts", "resetDebugState", 1),
            sym("config.ts", "isLocal", 2),
        ];
        let graph = vec![edge(
            "routes/handlers.ts#resetDebugState",
            "config.ts#isLocal",
        )];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &graph,
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty(), "{:?}", out);
    }

    #[test]
    fn require_lowercase_substring_in_an_unrelated_word_does_not_false_clear() {
        // "checkUnrequiredParam" contains "require" as a lowercase substring ("unREQUIREd"), but it is
        // never followed by a capital letter — the case-sensitive `(?-i:require[A-Z]\w*)` branch must NOT
        // match it, so this handler, which reaches only this call, is still flagged as unguarded.
        let provides = vec![provide("POST /setup", "routes/api.ts", 3, "runSetup")];
        let symbols = vec![
            sym("routes/handlers.ts", "runSetup", 1),
            sym("routes/handlers.ts", "checkUnrequiredParam", 2),
        ];
        let graph = vec![edge(
            "routes/handlers.ts#runSetup",
            "routes/handlers.ts#checkUnrequiredParam",
        )];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &graph,
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1, "{:?}", out);
    }

    #[test]
    fn nest_guarded_line_is_exempt_before_entering_the_bfs() {
        // Empty symbol_graph and a handler name with no guard-vocabulary substring — the BFS alone would
        // find nothing. The provide's own (file, line) is in `nest_guarded`, so it must never be flagged,
        // proving the exemption applies BEFORE/INSTEAD of the BFS.
        let provides = vec![provide(
            "POST /items",
            "items.controller.ts",
            5,
            "handleApiPost",
        )];
        let symbols = vec![sym("items.controller.ts", "handleApiPost", 5)];
        let mut nest_guarded = std::collections::HashSet::new();
        nest_guarded.insert(("items.controller.ts".to_string(), 5));
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &nest_guarded,
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty(), "{:?}", out);
    }

    #[test]
    fn a_provide_whose_line_is_not_in_nest_guarded_is_still_flagged_normally() {
        // Regression guard: `nest_guarded` containing some OTHER line does not blanket-suppress the rule —
        // only the exact (file, line) pairs it names are exempt.
        let provides = vec![provide(
            "POST /items",
            "items.controller.ts",
            5,
            "handleApiPost",
        )];
        let symbols = vec![sym("items.controller.ts", "handleApiPost", 5)];
        let mut nest_guarded = std::collections::HashSet::new();
        nest_guarded.insert(("items.controller.ts".to_string(), 99));
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &nest_guarded,
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1, "{:?}", out);
    }

    #[test]
    fn nest_guarded_exemption_is_precise_per_route_in_a_shared_controller() {
        // End-to-end-flavored: two routes in one controller file, only one line present in
        // `nest_guarded` (simulating method-level-only guarding) — the guarded one is exempt, the other
        // still fires. Neither handler name nor the empty symbol_graph offers the BFS anything to find.
        let provides = vec![
            provide("POST /items/a", "items.controller.ts", 4, "createA"),
            provide("POST /items/b", "items.controller.ts", 7, "createB"),
        ];
        let symbols = vec![
            sym("items.controller.ts", "createA", 4),
            sym("items.controller.ts", "createB", 7),
        ];
        let mut nest_guarded = std::collections::HashSet::new();
        nest_guarded.insert(("items.controller.ts".to_string(), 4));
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &nest_guarded,
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert_eq!(out.len(), 1, "{:?}", out);
        assert_eq!(out[0].line, 7);
    }

    #[test]
    fn non_http_provides_are_ignored() {
        let provides = vec![zzop_core::IoProvide {
            body: None,
            kind: "queue".to_string(),
            key: "POST /topic".to_string(),
            file: "routes/api.ts".to_string(),
            line: 3,
            symbol: Some("publish".to_string()),
        }];
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &[sym("routes/handlers.ts", "publish", 1)],
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &zzop_core::AttributeStore::default(),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn injected_auth_guarded_attribute_on_the_route_iokey_exempts_it() {
        // Empty graph + a handler name with no guard vocabulary — the BFS alone clears nothing. An
        // injected `auth-guarded` attribute on the route's exact IoKey (middleware the BFS can't see)
        // exempts it, the injection completion of the middleware precision limit.
        let provides = vec![provide("POST /items", "routes/api.ts", 3, "createItem")];
        let symbols = vec![sym("routes/handlers.ts", "createItem", 1)];
        let store = zzop_core::AttributeStore::from_attrs(vec![zzop_core::Attribute {
            target: zzop_core::EntityRef::IoKey {
                kind: "http".to_string(),
                key: "POST /items".to_string(),
            },
            key: AUTH_GUARDED_ATTR.to_string(),
            value: serde_json::json!(true),
        }]);
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &store,
        });
        assert!(out.is_empty(), "{:?}", out);
    }

    #[test]
    fn injected_pathscope_auth_guarded_exempts_every_route_under_the_prefix() {
        // A router-level middleware guards `/admin/*`; injected as a PathScope, it clears both routes
        // under it without naming each — while a route OUTSIDE the scope still fires.
        let provides = vec![
            provide("DELETE /admin/users/{}", "routes/api.ts", 3, "deleteUser"),
            provide("POST /public/signup-lite", "routes/api.ts", 5, "createLite"),
        ];
        let symbols = vec![
            sym("routes/handlers.ts", "deleteUser", 1),
            sym("routes/handlers.ts", "createLite", 2),
        ];
        let store = zzop_core::AttributeStore::from_attrs(vec![zzop_core::Attribute {
            target: zzop_core::EntityRef::PathScope {
                prefix: "/admin".to_string(),
            },
            key: AUTH_GUARDED_ATTR.to_string(),
            value: serde_json::json!(true),
        }]);
        let out = scan_mutating_route_no_auth(&ScanMutatingRouteNoAuthInput {
            io_provides: &provides,
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            auth_guard_pattern: DEFAULT_AUTH_GUARD_PATTERN,
            nest_guarded: &std::collections::HashSet::new(),
            route_attr_store: &store,
        });
        assert_eq!(out.len(), 1, "{:?}", out);
        assert_eq!(out[0].data.as_ref().unwrap()["path"], "/public/signup-lite");
    }
}
