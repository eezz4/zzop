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
//! `rules: { "mutating-route-no-auth": "off" }` (embedders: `disabledRules`), this rule having no marker.
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
//! stand-in rather than real package resolution — which turned out to cost a FALSE finding rather than a
//! missing one (a guard two hops out went unseen and the route fired), and the engine now bridges the
//! whole-corpus type index onto that graph afterwards (`callgraph::java_bridge`, 2026-09-07); Python
//! followed with a REAL module resolver
//! (`python_import_candidates` against the tree's own path set) — and still needed a bridge, because a
//! real resolver places the FILE while `resolve_method` reads `from pkg import mod` as a class binding
//! (`callgraph::python_bridge` and its doc, 2026-09-08, review ledger V100); Rust is the fourth and the ONE case
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
//! - **NestJS `@UseGuards(...)`** (class/method), and since 2026-09-05 any decorator whose own NAME says it
//!   authenticates or gates (`@Authenticated`, `@AuthGuard`) — the HOUSE decorator a codebase writes once it
//!   has wrapped its guard a single time. `zzop_parser_typescript::extract_controller_guarded_lines` owns that
//!   vocabulary and its two deliberate exclusions: a decorator that merely DOCUMENTS a scheme
//!   (`@ApiBearerAuth`) and one that opts OUT of auth (`@SkipAuth`) both carry auth words and are read as
//!   neither, which is why the accept side requires the full `authentic`/`authoriz` stem rather than `auth`.
//! - **Spring method security** `@PreAuthorize`/`@PostAuthorize`/`@Secured`/`@RolesAllowed` (class/method, SpEL
//!   never interpreted) — `zzop_parser_java_21::extract_spring_guarded_lines` (the route's own mapping-
//!   annotation line, the same anchor its `IoProvide` carries).
//! - **FastAPI `Depends(...)`** (route-decorator `dependencies=[...]`, a parameter default, an
//!   `Annotated[..., Depends(...)]` parameter, or a tree-resolved `Annotated` alias) —
//!   `zzop_parser_python_3::extract_fastapi_guarded_lines` (the route decorator's own anchor line).
//! - **Django REST Framework `permission_classes`** — `zzop_parser_python_3::
//!   extract_django_view_guard_classes` returns per-VIEW-CLASS verdicts (the evidence lives in
//!   `views.py`, the route anchor in `urls.py`), which the engine joins to a provide by its `symbol`.
//! - **NestJS route-scoped middleware** — an auth-named `consumer.apply(AuthX).forRoutes({path, method})`
//!   (`extract_nest_forroutes_guarded`); engine matches each (method,path) pattern (exact, prefix-anchored).
//! - **Spring global `SecurityFilterChain`** — a secure-by-default `anyRequest().authenticated()` chain in
//!   EITHER the classic fluent or the Spring-6 lambda-DSL spelling, folding two
//!   `authorizeHttpRequests(..)` customizers on one chain (`extract_spring_security_posture`); a route is
//!   authenticated iff it escapes every `.permitAll()` matcher. Strict parse-all-or-nothing: bails on any
//!   scoped/unrecognized form, with a NAMED reason (`zzop_parser_java_21::SpringPostureBail`).
//!
//! **Residual:** NestJS global guards (`useGlobalGuards`/`APP_GUARD`) and Spring configs that are
//! path-scoped (`securityMatcher`), carry `WebSecurity.ignoring()`, hold more than one authorization
//! chain, or whose matchers/`anyRequest` terminal aren't literally readable (a property-bound whitelist,
//! an `.access(mgr == null ? ... : mgr)` terminal) aren't modeled — a route relying ENTIRELY on those fires.
//!
//! **Second residual, and the only one with NO self-report: a tree carrying TWO OR MORE readable Spring
//! chains gets none of them.** The bails above are per-config and each one publishes its reason
//! (the `Spring Security config read but NOT applied` disclosure). This one is not a bail at all — every
//! config parsed, and the engine then discards the whole set, because `assemble_decorator_guarded`
//! (`crates/engine/.../callgraph/decorator_gate.rs`) consumes the postures through an exactly-one slice
//! pattern. Measured 2026-09-11 on a planted two-module tree (`svc-a` and `svc-b`, each with its own
//! secure-by-default `anyRequest().authenticated()` chain and one mutating route): both configs present
//! -> 2 findings (`/a/thing` AND `/b/thing`, so the posture that really does govern `/a/thing` was
//! thrown away); delete `svc-b`'s config -> 1 finding (`/b/thing` only, `/a/thing` correctly exempt);
//! restore it -> 2 again. In all three runs the posture disclosure count was ZERO. So on a multi-module
//! Spring monorepo — the layout where a second chain is ordinary rather than exotic — this rule reports
//! every mutating route and NOTHING in the reply says the evidence was read and dropped. The direction
//! is the safe one (over-reporting, never a false exemption), which is why it is a residual rather than
//! a defect; what it is not is honest yet. Do not read a per-config bail reason's ABSENCE as "no Spring
//! config interfered" — that is exactly the inference this case defeats. Fixing it is engine work
//! (a per-route-module posture map, plus a disclosure for the discard), not this crate's.

use zzop_core::callgraph::SymbolGraph;
use zzop_core::{Finding, Severity, SourceSymbol};

use crate::http_scan::{build_name_index, resolve_handler_scoped};

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
    /// Calls the resolver could not place, indexed by caller symbol
    /// (`zzop_core::callgraph::build_symbol_graph_with_unresolved`) — the NAMES of callees that drew no
    /// edge. Pass an empty map to get the pre-2026-08-17 behaviour.
    ///
    /// Load-bearing, and the reason is a measured false positive at 8 of 16. This rule's own message
    /// says it looks for a CALL WHOSE NAME LOOKS LIKE A GUARD, but the walk only ever saw resolved
    /// symbol ids — so a guard declared inside a factory (not a top-level symbol, therefore no edge)
    /// made the route read as reaching no guard at all. The rule already refuses to guess about an
    /// unresolved HANDLER (`resolve_handler_scoped`, "do not guess"); it had no such discipline about an
    /// unresolved CALLEE, and asserted the absence instead.
    ///
    /// A name here is weaker evidence than an edge and is used only in the direction that CLEARS a
    /// finding: it can prove a guard is called, never that one is missing. Names that do NOT match stay
    /// on the finding as `data.unresolvedCallees`, so a reader can dismiss the residue in seconds
    /// instead of re-deriving why the graph is short an edge.
    pub unresolved_callees: &'a std::collections::BTreeMap<String, Vec<String>>,
}

pub fn scan_mutating_route_no_auth(input: &ScanMutatingRouteNoAuthInput) -> Vec<Finding> {
    let acquisition = vocab::AcquisitionSurface::compile(input);
    // Which routes are even candidates — see `candidates`. Every exemption there is a "do not guess"
    // gate, and they are read together rather than interleaved with the call-graph walk below.
    let mutating = candidates::mutating_route_candidates(input, &acquisition);
    if mutating.is_empty() {
        return Vec::new();
    }

    let name_index = build_name_index(input.symbols);
    let guard_reach = guard_reach::GuardReach::new(input, &name_index);

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
        if guard_reach.reaches(&handler_symbol) {
            continue;
        }
        let unresolved = guard_reach.unresolved_residue(&handler_symbol);
        let hint = message::missing_auth_hint(
            method,
            path,
            handler_ref,
            input.auth_guard_pattern,
            &unresolved,
        );
        out.push(Finding {
            rule_id: "mutating-route-no-auth".to_string(),
            severity: Severity::Info,
            file: p.file.clone(),
            line: p.line,
            message: hint.clone(),
            evidence_paths: Vec::new(),
            data: Some(serde_json::json!({
                "method": method,
                "path": path,
                "handler": handler_ref,
                "handlerSymbol": handler_symbol,
                // 🔴 NO `hint` KEY HERE, and its absence is the repair. This rule used to emit
                // `"hint": hint` beside `message: hint.clone()` — the SAME string twice in one finding.
                // Harmless while both were inline; expensive once the prose fold landed, because the fold
                // shrinks `message` to a pointer and `data.hint` kept shipping the full text per finding,
                // cancelling the saving exactly. 📏 Measured 2026-09-13 (ledger V231) on
                // `analyze corpus/frameworks/fastapi --limit 1000`: `data.hint` was 624,774 of the reply's
                // 1,167,742 bytes (60%), and 117 of 117 hints were byte-identical to their own finding's
                // message. The text is not lost — it is in `message`, which is the field that carries it.
                // Present only when there is something to say, the additive-disclosure convention: an
                // always-present empty array reads as "the resolver placed every call", which is a
                // stronger claim than "this handler had none it could not place".
                "unresolvedCallees": (!unresolved.is_empty()).then_some(unresolved),
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

mod candidates;
mod guard_reach;
mod message;
/// `pub` only so `QUALIFIER_GUARD_TOKENS` can be re-exported at the crate root as a declarable default —
/// everything else in it stays `pub(super)`.
pub mod qualifier;
mod vocab;

#[cfg(test)]
mod tests;
