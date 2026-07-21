//! `mutating-route-no-auth` — flags a POST/PUT/PATCH/DELETE `IoProvide` (an HTTP route) whose handler
//! symbol, walked via call-graph BFS (`zzop_core::callgraph::bfs_reachable` over the whole-repo
//! `SymbolGraph`), never reaches a callee whose NAME looks like an auth guard — unlike the DSL
//! `http/auth-gates` rule (registration-line handler-identifier text only), this follows actual calls.
//!
//! ## Guard vocabulary
//! [`DEFAULT_AUTH_GUARD_PATTERN`] is matched against (name-segment shape below — see "Match granularity")
//! every symbol id `bfs_reachable` visits — a name-vocabulary check, not a body inspector. `access` is
//! guarded to `(has|can|check|require)access` only (bare `access` clears `accessLog`/`dataAccess`). Two
//! classes are EXCLUDED — clearing on a non-authorization name silently suppresses a real missing-auth
//! finding (recall loss outweighs FP savings for a security rule): a blanket `require[A-Z]\w*` (clears
//! `requireBody`-style validation; `requireAuth`/`requireOwner` still match via the stem), and env gates
//! (`isProduction`/`isLocal`/`isDev` — WHERE code runs, not WHO calls it: route-EXPOSURE, not auth). A
//! real guard named outside this vocabulary still false-positives here —
//! the finding message points at config `rules: { "mutating-route-no-auth": "off" }` (embedders:
//! `disabled_rules`) as the escape hatch, since this rule has no inline suppression marker.
//!
//! ## Decidable subset
//! Only mutating-method provides whose `symbol` resolves to a UNIQUE known symbol are checked
//! (`http_scan::resolve_handler`'s "do not guess" contract) — an unresolved or ambiguous handler is
//! skipped. `bfs_reachable` can match the handler's own symbol id at depth 0, so a self-describing handler
//! name alone is enough to clear this check.
//!
//! ## Call-graph language coverage (the OTHER half of the decidable subset)
//! `symbol_graph` is built from re-parsed TypeScript/JavaScript source AND (as of
//! `java21/tree-sitter-java-0.23.5/v2`) re-parsed Java source (`run_callgraph_rules`, which loops
//! `ts_paths`/`java_rels` — see that function's own module doc). No OTHER language parser in this
//! workspace produces the `RawCall` sites `bfs_reachable` walks, so for a handler outside
//! [`CALL_GRAPH_COVERED_EXTENSIONS`], `symbol_graph` restricted to that ecosystem is provably EMPTY — the
//! BFS can never find a guard there. [`is_call_graph_covered`] makes this explicit and load-bearing: a
//! mutating provide outside the covered set is exempt from the BFS entirely, same "do not guess" spirit
//! as the unresolved/ambiguous-handler skip above — recall for an uncovered ecosystem is zero until real
//! coverage exists, honest given the mechanism has no evidence for it, vs. a naming-accident-gated mix of
//! false positives/negatives (skipped only when `resolve_handler` happens to fail on an unrelated name
//! collision) that looks like a bug and is one.
//! Lifting the exemption for a language needs two additions outside this crate: (1) a `RawCall`-producing
//! extractor (`RawCall`'s own doc, `crates/core/src/callgraph.rs`); (2) engine wiring in
//! `run_callgraph_rules` to gather that language's calls. Java is the first to have done both: its
//! extractor is `zzop_parser_java_21::lang::calls::parse_calls`; its `resolve_file` is an
//! opaque-specifier stand-in, not real package resolution (see `run_callgraph_rules`'s doc for why
//! that's sound here). Neither addition is this crate's call to make (`rules/**` cannot depend on
//! `zzop_parser_typescript`/`zzop_parser_java_21`/engine internals) — see its dependency boundary.
//!
//! ## Precision limit (and its injection completion)
//! This is a vocabulary-based reachability check over the CALL graph only. Route-level middleware —
//! `app.post("/x", requireAuth, handler)`, or a router-level `.use(authMiddleware)` — never appears as a
//! call edge FROM the handler symbol itself, so it is invisible to this rule: a route guarded exclusively
//! via middleware will false-positive. Severity starts at [`Severity::Info`] because of this.
//!
//! Middleware is a per-project environment fact the native call-graph can't see — so, per zzop's design
//! line (native sees the common case; everything else is injected), it is COMPLETED BY INJECTION rather
//! than by ever-growing native middleware modeling. The common Express shapes (`app`/`router.use(guard)`,
//! a route-level guard argument) are the exception that proves the rule: the native TypeScript parser's
//! router-mounts producer (`zzop_parser_typescript::adapters::router_mounts`) now judges and emits the
//! same attribute directly, PREPAID injection for that one common environment, not a second code path —
//! everything outside that vocabulary (a non-Express framework, a project's own custom guard naming)
//! still needs an adapter to inject it. A producer/adapter that understands a project's
//! middleware injects an [`AUTH_GUARDED_ATTR`] attribute on the guarded route (an `IoKey`) or router
//! prefix (a `PathScope`) through the generic entity-attribute channel (`zzop_core::AttributeStore`,
//! [`ScanMutatingRouteNoAuthInput::route_attr_store`]); the native vocab BFS and the injected evidence
//! COMPOSE (either clears the route). This is one consumer of a general channel, not a bespoke auth path.
//!
//! ## Match granularity: tail name PLUS the immediate qualifier
//! [`is_guard_id`] checks TWO trailing segments of a visited id (`<file>#<Receiver>.<method>`), with
//! deliberately DIFFERENT matchers: the tail (method name) keeps the substring
//! [`DEFAULT_AUTH_GUARD_PATTERN`] (verb-shaped names); the qualifier (class name) uses exact
//! camel-token matching against [`qualifier::QUALIFIER_GUARD_TOKENS`] — this is what makes a Java
//! static-utility guard visible (`AuthorizationService.canWriteComment`: `auth` lives only in the
//! qualifier) WITHOUT substring's domain-noun false-clears (`AuthorRepository` ⊃ `auth` — see
//! `qualifier.rs`). A BARE call id's qualifier is its file-extension token — harmless either way.
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
//!   exempt, but `/devices/register` is not. Every segment list is matched `/`-delimited on whole path
//!   segments only, never as a bare substring — `/author/profile` does not match `auth`.
//!
//! ## Test-fixture exemption
//! A provide registered in a test/fixture file (`is_test_file` — the same predicate `unreachable`'s
//! dead-island check uses) is skipped outright: a route only defined/invoked from a test isn't exposed
//! application surface.
//!
//! ## Decorator/annotation auth exemption
//! A provide whose registration line carries decorator/annotation-based auth is exempt from the BFS
//! entirely: such auth runs BEFORE the handler regardless of what its body calls, so the BFS assumption
//! (the guard must be REACHABLE FROM the handler) doesn't apply — its application is metadata, not a call
//! edge (the same blind spot as route-level middleware). The exemption is a framework-neutral side-channel
//! `HashSet<(file, line)>` ([`ScanMutatingRouteNoAuthInput::decorator_guarded`]); four producers feed it:
//! - **NestJS `@UseGuards(...)`** (class/method) — `zzop_parser_typescript::extract_controller_guarded_lines`.
//! - **Spring method security** `@PreAuthorize`/`@PostAuthorize`/`@Secured`/`@RolesAllowed` (class/method, SpEL
//!   never interpreted) — `zzop_parser_java_21::extract_spring_guarded_lines` (the route method's anchor line).
//! - **NestJS route-scoped middleware** — an auth-named `consumer.apply(AuthX).forRoutes({path, method})`
//!   (`extract_nest_forroutes_guarded`); engine matches each (method,path) pattern (exact, prefix-anchored).
//! - **Spring global `SecurityFilterChain`** — a secure-by-default `authorizeRequests()...anyRequest()
//!   .authenticated()` chain (`extract_spring_security_posture`); a route is authenticated iff it escapes
//!   every `.permitAll()` matcher. Strict parse-all-or-nothing: bails on any scoped/unrecognized form.
//!
//! **Residual:** NestJS global guards (`useGlobalGuards`/`APP_GUARD`) and Spring's lambda-DSL / path-scoped
//! or `WebSecurity.ignoring()`-bearing configs aren't modeled — a route relying ENTIRELY on those fires.

use std::collections::HashMap;

use regex::Regex;
use zzop_core::callgraph::{bfs_reachable, SymbolGraph};
use zzop_core::{disable_hint, Finding, Severity, SourceSymbol};

use crate::http_scan::{build_name_index, resolve_handler};
use zzop_core::is_test_file;

/// Default guard-name vocabulary — see module doc "Guard vocabulary".
pub const DEFAULT_AUTH_GUARD_PATTERN: &str = r"(?i)(auth|guard|verify|session|token|permission|acl|owner|admin|role|(?:has|can|check|require)access)";

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

/// Extensions the whole-repo call-graph BFS actually has `RawCall` edges for — module doc "Call-graph
/// language coverage". Duplicated from `zzop_engine`'s `dead_exports::is_ts_source_ext` list plus
/// `"java"` rather than shared (this crate depends on `zzop_core` only). Adding `"java"` here is the
/// wiring-completion step this constant's own doc predicted: `zzop_parser_java_21::lang::calls::
/// parse_calls` now feeds `symbol_graph` real Java call-site edges. `pub`: pinned against `is_ts_source_ext`.
pub const CALL_GRAPH_COVERED_EXTENSIONS: &[&str] =
    &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "java"];

/// True when `file`'s extension is one the call-graph BFS has evidence for — module doc "Call-graph
/// language coverage". A file outside this set (Python, Go, Rust, ...) is exempt: `symbol_graph`
/// restricted to its ecosystem is provably empty, so "never reaches a guard" is guaranteed, not evidence.
fn is_call_graph_covered(file: &str) -> bool {
    std::path::Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| CALL_GRAPH_COVERED_EXTENSIONS.contains(&e.as_str()))
}

/// Input for [`scan_mutating_route_no_auth`]. Takes `io_provides` directly (not the `ApiEndpoint` shape
/// `http_scan`'s two rules take) so the emitted `Finding` can anchor on the route's own registration
/// `file`/`line` — `ApiEndpoint` carries no line number (see `zzop_engine::io`'s module doc, "`ApiEndpoint`
/// has no line number"), and this rule's problem IS the route registration, not a downstream write site.
pub struct ScanMutatingRouteNoAuthInput<'a> {
    pub io_provides: &'a [zzop_core::IoProvide],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    pub auth_guard_pattern: &'a str,
    /// Framework-neutral decorator/annotation-based auth coverage — see module doc "Decorator/annotation
    /// auth exemption". `(file, line)` pairs matching an `IoProvide`'s own `file`/`line` are exempt from the
    /// BFS entirely (like the test-fixture / auth-acquisition exemptions): this IS how the route is guarded,
    /// via a decorator/annotation the BFS structurally can't see (metadata, not a call edge). Fed by NestJS
    /// `@UseGuards` and Spring method security (`@PreAuthorize`/etc.) — see the module doc for the producers.
    /// Pass an empty set when the caller computes no such exemption — old behavior (no exemption) preserved.
    pub decorator_guarded: &'a std::collections::HashSet<(String, u32)>,
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
        // The call-graph BFS below has zero evidence for a non-TS/JS ecosystem — module doc "Call-graph
        // language coverage". Exempt before resolving/BFS-ing, the same "do not guess" spirit as the
        // unresolved/ambiguous-handler skip.
        .filter(|p| is_call_graph_covered(&p.file))
        .filter(|p| !input.decorator_guarded.contains(&(p.file.clone(), p.line)))
        // Injected auth-guard evidence (route-level middleware the call-graph BFS can't see) — see
        // `AUTH_GUARDED_ATTR`. Exempt BEFORE the BFS, like `decorator_guarded`: this IS how the route is guarded.
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
        let mut segments = id.rsplit(['#', '.']);
        let tail = segments.next().unwrap_or(id);
        guard_re.is_match(tail) || segments.next().is_some_and(qualifier::qualifier_is_guard)
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

mod qualifier;

#[cfg(test)]
mod tests;
