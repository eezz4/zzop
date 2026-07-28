//! `mutating-route-no-auth` — flags a POST/PUT/PATCH/DELETE `IoProvide` (an HTTP route) whose handler
//! symbol, walked via call-graph BFS (`zzop_core::callgraph::bfs_reachable` over the whole-repo
//! `SymbolGraph`), never reaches a callee whose NAME looks like an auth guard — unlike the DSL
//! `http/protected-path-no-auth-evidence` rule (registration-line handler-identifier text only), this
//! follows actual calls.
//!
//! ## Guard vocabulary
//! [`DEFAULT_AUTH_GUARD_PATTERN`] is matched against (name-segment shape below — see "Match granularity")
//! every symbol id `bfs_reachable` visits — a name-vocabulary check, not a body inspector. `access` is
//! guarded to `(has|can|check|require)access` only (bare `access` clears `accessLog`/`dataAccess`). Two
//! classes are EXCLUDED — clearing on a non-authorization name silently suppresses a real missing-auth
//! finding (recall loss outweighs FP savings for a security rule): a blanket `require[A-Z]\w*` (clears
//! `requireBody`-style validation; `requireAuth`/`requireOwner` still match via the stem), and env gates
//! (`isProduction`/`isLocal`/`isDev` — WHERE code runs, not WHO calls it: route-EXPOSURE, not auth). A
//! real guard named outside this vocabulary still false-positives — the message points at config
//! `rules: { "mutating-route-no-auth": "off" }` (embedders: `disabled_rules`), this rule having no marker.
//!
//! ## Decidable subset
//! Only mutating provides whose handler resolves to a known symbol are checked
//! (`http_scan::resolve_handler_scoped`, "do not guess"): a repo-wide-unique name resolves directly; a name
//! ambiguous repo-wide resolves ONLY to a UNIQUE candidate in the route's OWN file — sound for a decorator-
//! routed method (it lives in its controller file), with a narrow imported-member-handler residual noted at
//! that fn. Any other ambiguity, or an unknown handler, is skipped; a `bfs_reachable` depth-0 self-match
//! clears a self-describing handler name on its own.
//!
//! ## Call-graph language coverage (the OTHER half of the decidable subset)
//! `symbol_graph` is built from re-parsed TypeScript/JavaScript, Java, Python AND Rust source
//! (`run_callgraph_rules`, which loops `ts_paths`/`java_rels` plus the Python- and Rust-dispatched
//! members of `ts_paths` — see that function's own module doc). No OTHER language parser in this
//! workspace produces the `RawCall` sites `bfs_reachable` walks, so for a handler outside
//! [`CALL_GRAPH_COVERED_EXTENSIONS`], `symbol_graph` restricted to that ecosystem is provably EMPTY — the
//! BFS can never find a guard there. [`is_call_graph_covered`] makes this explicit and load-bearing: a
//! mutating provide outside the covered set is exempt from the BFS entirely, same "do not guess" spirit
//! as the unresolved/ambiguous-handler skip above — recall there is zero until real coverage exists.
//! Lifting the exemption for a language needs two additions outside this crate: (1) a `RawCall`-producing
//! extractor (`RawCall`'s own doc, `crates/core/src/callgraph.rs`); (2) engine wiring in
//! `run_callgraph_rules` to gather that language's calls; neither is this crate's to make (`rules/**`
//! cannot depend on parser/engine internals). Java did both first, with an opaque-specifier `resolve_file`
//! stand-in rather than real package resolution; Python followed with a REAL module resolver
//! (`python_import_candidates` against the tree's own path set); Rust is the fourth and the ONE case
//! where the guard half needed no side-channel — its extractor evidence
//! (`zzop_parser_rust::parse_extractor_guards`) is an edge out of the handler, so the vocabulary below
//! matches it unchanged, and its resolver is real and crate-aware. Rust residual, mainstream rather
//! than marginal: a route guarded ONLY by `.route_layer(..)`/`.wrap(..)` fires — `framework_silence::
//! rust_router_layer` discloses that range per run.
//!
//! **A lift is only honest WITH the guard-vocabulary half** (see "Decorator/annotation auth exemption").
//! A language's guards are usually applied as framework metadata, not as calls the handler body makes
//! (FastAPI `Depends`, Spring `@PreAuthorize`, Nest `@UseGuards`): covering a language's call graph
//! without its guard evidence turns every guarded mutating route into a false positive, while shipping
//! only the guard evidence leaves every route in that language exempt and the rule silent.
//!
//! ## Precision limit (and its injection completion)
//! This is a vocabulary-based reachability check over the CALL graph only. Route-level middleware —
//! `app.post("/x", requireAuth, handler)`, or a router-level `.use(authMiddleware)` — never appears as a
//! call edge FROM the handler symbol itself, so it is invisible to this rule: a route guarded exclusively
//! via middleware will false-positive. Severity starts at [`Severity::Info`] because of this.
//!
//! Middleware is a per-project environment fact the native call-graph can't see — so, per zzop's design
//! line (native sees the common case; everything else is injected), it is COMPLETED BY INJECTION rather
//! than ever-growing native middleware modeling. The common Express shapes (`app`/`router.use(guard)`, a
//! route-level guard argument) are prepaid: the native `router_mounts` producer
//! (`zzop_parser_typescript::adapters::router_mounts`) emits the attribute directly. Everything else (a
//! non-Express framework, custom guard naming) needs an adapter to inject an [`AUTH_GUARDED_ATTR`] on the
//! guarded route (`IoKey`) or router prefix (`PathScope`) via the generic entity-attribute channel
//! (`zzop_core::AttributeStore`, [`ScanMutatingRouteNoAuthInput::route_attr_store`]); native vocab BFS and
//! injected evidence COMPOSE (either clears the route), one consumer of a general channel.
//!
//! ## Match granularity: tail name PLUS the immediate qualifier
//! [`is_guard_id`] checks TWO trailing segments of a visited id (`<file>#<Receiver>.<method>`) with
//! deliberately DIFFERENT matchers: the tail (method name) keeps the substring
//! [`DEFAULT_AUTH_GUARD_PATTERN`] (verb-shaped names); the qualifier (class name) needs an exact
//! camel-token hit AND a symbol this tree actually DECLARES under that name, since the resolver mints
//! receiver ids it never verified. `qualifier.rs` owns both gates and why each is needed (Java
//! static-utility guards stay visible; domain-noun substrings and phantom receivers do not).
//!
//! ## Auth-acquisition exemption
//! A provide whose PATH sits on the auth-acquisition surface is exempt entirely, never entering the BFS —
//! that surface IS how a caller gets credentials, so it cannot require pre-existing auth to reach itself.
//! Two tiers (some acquisition-shaped words also name unrelated mutating routes, `POST /devices/register`)
//! — the tiers, their vocabularies and the whole-segment matching rule live in [`vocab`].
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
//! `HashSet<(file, line)>` ([`ScanMutatingRouteNoAuthInput::decorator_guarded`]); its producers:
//! - **NestJS `@UseGuards(...)`** (class/method) — `zzop_parser_typescript::extract_controller_guarded_lines`.
//! - **Spring method security** `@PreAuthorize`/`@PostAuthorize`/`@Secured`/`@RolesAllowed` (class/method, SpEL
//!   never interpreted) — `zzop_parser_java_21::extract_spring_guarded_lines` (the route method's anchor line).
//! - **FastAPI `Depends(...)`** (route-decorator `dependencies=[...]`, a parameter default, an
//!   `Annotated[..., Depends(...)]` parameter, or a tree-resolved `Annotated` alias) —
//!   `zzop_parser_python_3::extract_fastapi_guarded_lines` (the route decorator's own anchor line).
//! - **Django REST Framework `permission_classes`** — `zzop_parser_python_3::
//!   extract_django_view_guard_classes` returns per-VIEW-CLASS verdicts (the evidence lives in
//!   `views.py`, the route anchor in `urls.py`), which the engine joins to a provide by its `symbol`.
//! - **NestJS route-scoped middleware** — an auth-named `consumer.apply(AuthX).forRoutes({path, method})`
//!   (`extract_nest_forroutes_guarded`); engine matches each (method,path) pattern (exact, prefix-anchored).
//! - **Spring global `SecurityFilterChain`** — a secure-by-default `authorizeRequests()...anyRequest()
//!   .authenticated()` chain (`extract_spring_security_posture`); a route is authenticated iff it escapes
//!   every `.permitAll()` matcher. Strict parse-all-or-nothing: bails on any scoped/unrecognized form.
//!
//! **Residual:** NestJS global guards (`useGlobalGuards`/`APP_GUARD`) and Spring's lambda-DSL / path-scoped
//! or `WebSecurity.ignoring()`-bearing configs aren't modeled — a route relying ENTIRELY on those fires.

use std::collections::HashMap;

use zzop_core::callgraph::{bfs_reachable, SymbolGraph};
use zzop_core::{Finding, Severity, SourceSymbol};

use crate::http_scan::{build_name_index, resolve_handler_scoped};
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

use vocab::vocab_re;
pub use vocab::{
    AUTH_ACQUISITION_CONDITIONAL_PATTERN, AUTH_ACQUISITION_STANDALONE_PATTERN,
    AUTH_FAMILY_PATH_PATTERN,
};

use crate::http_scan::WRITE_HTTP_METHODS;

/// Extensions the whole-repo call-graph BFS actually has `RawCall` edges for — module doc "Call-graph
/// language coverage". Duplicated from `zzop_engine`'s `dead_exports::is_ts_source_ext` list plus
/// `"java"`, `"py"`/`"pyi"` and `"rs"` rather than shared (this crate depends on `zzop_core` only). Each
/// addition is the wiring-completion step this constant's own doc predicts — the `lang::calls::
/// parse_calls` of `zzop_parser_java_21`/`zzop_parser_python_3`/`zzop_parser_rust` feed `symbol_graph`
/// real call-site edges. `py`/`pyi` are BOTH listed because they are exactly what
/// `zzop_engine`'s dispatch routes to the Python parser (`is_python_source_ext`) and therefore exactly
/// what the engine's Python re-parse loop walks — a route provide never comes from a `.pyi` stub, so
/// listing it claims nothing extra. `"rs"` is the newest lift and the one whose guard half looks least
/// like the others: a Rust guard is a TYPE in the handler's signature, projected as an ordinary edge by
/// `zzop_parser_rust::parse_extractor_guards`, so it needs no `decorator_guarded` entry at all.
/// `pub`: pinned against `is_ts_source_ext`.
pub const CALL_GRAPH_COVERED_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "java", "py", "pyi", "rs",
];

/// True when `file`'s extension is one the call-graph BFS has evidence for — module doc "Call-graph
/// language coverage". A file outside this set (Go, C#, ...) is exempt: `symbol_graph`
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
    /// How the project spells its guard functions. `None` — undeclared — means no call NAME can prove a
    /// guard, so this half of the evidence is simply absent and every mutating route depends on the
    /// structural exemptions (decorator/annotation, injected attribute) alone.
    pub auth_guard_pattern: Option<&'a str>,
    /// Camel tokens proving a receiver-CLASS name is a guard — the qualifier half of the two-segment
    /// match (module doc "Match granularity"). Empty means that half is not judged.
    pub qualifier_guard_tokens: &'a [&'a str],
    /// The three auth-acquisition path vocabularies — see [`vocab`] for the tiers. `None` means that tier
    /// exempts nothing; an undeclared vocabulary is never silently replaced with ours.
    pub auth_acquisition_standalone_pattern: Option<&'a str>,
    pub auth_acquisition_conditional_pattern: Option<&'a str>,
    pub auth_family_path_pattern: Option<&'a str>,
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
    let acquisition = vocab::AcquisitionSurface::compile(input);
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
            WRITE_HTTP_METHODS.contains(&method) && !acquisition.exempts(path)
        })
        .collect();
    if mutating.is_empty() {
        return Vec::new();
    }

    let name_index = build_name_index(input.symbols);
    let guard_re = vocab_re(input.auth_guard_pattern);
    let qual_guard = |q: &str| qualifier::is_guard(q, &name_index, input.qualifier_guard_tokens);
    let is_guard_id = |id: &str| -> bool {
        let mut seg = id.rsplit(['#', '.']);
        let tail = seg.next().unwrap_or(id);
        guard_re.as_ref().is_some_and(|re| re.is_match(tail)) || seg.next().is_some_and(&qual_guard)
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
        // Scope ambiguity tie-break to the route's own file: a NestJS `@Delete() delete()` handler is a
        // controller-class method in `p.file`, so a bare method name colliding across controllers
        // (`delete` in four controllers) still resolves — otherwise the whole rule is inert on idiomatic
        // decorator-routed controllers.
        let Some(handler_symbol) = resolve_handler_scoped(handler_ref, &name_index, Some(&p.file))
        else {
            continue; // unresolved/ambiguous handler — do not guess
        };
        if reaches_guard(&handler_symbol) {
            continue;
        }
        let hint = message::missing_auth_hint(method, path, handler_ref, input.auth_guard_pattern);
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

mod message;
/// `pub` only so `QUALIFIER_GUARD_TOKENS` can be re-exported at the crate root as a declarable default —
/// everything else in it stays `pub(super)`.
pub mod qualifier;
mod vocab;

#[cfg(test)]
mod tests;
