//! The three public extraction entry points and their AST-visitor collectors — see the parent
//! module's doc for scope, gating rules, and known limits.

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{ClassDecl, ClassMember, ClassMethod, Decorator};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, ControllerPrefixRouteFragment, IoProvide};

use super::context::{controller_context, ControllerCtx};
use super::method_facts::{decorator_name, method_route, method_route_facts};

/// Extracts NestJS `@Controller`/`@Get`/`@Post`/... HTTP route `IoProvide`s from one TS file's raw
/// source — see module doc for the decorator shapes and gating rules. Returns an empty `Vec` (never
/// panics) on an unparseable file, same convention as every other swc-AST adapter in this crate.
pub fn extract_controller_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut c = ControllerCollector {
        cm: cm_ref,
        file: rel,
        out: Vec::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct ControllerCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Vec<IoProvide>,
}

impl Visit for ControllerCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if let Some(ControllerCtx::Literal {
            prefix,
            route_version,
        }) = controller_context(&n.class.decorators)
        {
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    self.emit_method(&prefix, route_version.as_deref(), m);
                }
            }
        }
        // `ControllerCtx::DeferredRef` (a `RouteKey.Asset`-shaped prefix) emits no direct provides here
        // — see `extract_controller_prefix_route_fragments`.
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

impl ControllerCollector<'_> {
    /// `route_version` is the class-level `route-version-v1` discriminator (see `context`): it rides
    /// beside the key rather than inside it, because under header/media-type versioning the version
    /// never reaches the URL. Every route of one controller carries the same one — method-level
    /// `@Version()` is not read, so a method that overrides its class is stamped with the CLASS scope.
    /// That is a real hazard rather than a conservative default (a `VERSION_NEUTRAL` method answers at
    /// every version, so the class text UNDER-reports its reach and can be the difference that demotes
    /// a genuine shadow); the module doc's "Known limits" owns the measurement that leaves it in v1.
    fn emit_method(&mut self, prefix: &str, route_version: Option<&str>, m: &ClassMethod) {
        let Some((verb, name, line, paths, body, response)) = method_route_facts(self.cm, m) else {
            return;
        };
        for path in paths {
            let full_path = format!("{prefix}/{path}");
            self.out.push(IoProvide {
                route_version: route_version.map(str::to_string),
                body: body.clone(),
                response: response.clone(),
                kind: "http".to_string(),
                key: http_interface_key(&verb, &full_path),
                file: self.file.to_string(),
                line,
                symbol: Some(name.clone()),
            });
        }
    }
}

/// Extracts controller-prefix route FRAGMENTS — the deferred-to-assemble counterpart of
/// `extract_controller_provides` for the `controller-prefix-ref-v1` exception (module doc): a
/// `@Controller(RouteKey.Asset)`-shaped (dotted member-expression) prefix cannot be resolved from this
/// one file alone, so each qualifying controller's methods are projected as
/// `zzop_core::ControllerPrefixRouteFragment`s instead of `IoProvide`s.
/// `zzop_engine::analyze::compose`'s controller-prefix composer resolves `prefix_ref` against the
/// project-wide merged const map (`egress::const_map_fragment`, which also folds string-valued `enum`
/// members) and emits the real `IoProvide`s at assemble time. A `@Controller('literal')` class
/// contributes nothing here (already fully resolved by `extract_controller_provides`); any other
/// non-literal prefix shape (call, template, computed member, deeper chain, `{path: ref}` object)
/// contributes nothing here either — same skip-whole-controller convention as
/// `extract_controller_provides`. Returns an empty `Vec` (never panics) on an unparseable file.
pub fn extract_controller_prefix_route_fragments(
    rel: &str,
    text: &str,
) -> Vec<ControllerPrefixRouteFragment> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut c = ControllerPrefixFragmentCollector {
        cm: cm_ref,
        out: Vec::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct ControllerPrefixFragmentCollector<'a> {
    cm: &'a SourceMap,
    out: Vec<ControllerPrefixRouteFragment>,
}

impl Visit for ControllerPrefixFragmentCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if let Some(ControllerCtx::DeferredRef { prefix_ref }) =
            controller_context(&n.class.decorators)
        {
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    self.emit_fragment(&prefix_ref, m);
                }
            }
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

impl ControllerPrefixFragmentCollector<'_> {
    fn emit_fragment(&mut self, prefix_ref: &str, m: &ClassMethod) {
        let Some((verb, name, line, paths, body, response)) = method_route_facts(self.cm, m) else {
            return;
        };
        for path in paths {
            self.out.push(ControllerPrefixRouteFragment {
                body: body.clone(),
                response: response.clone(),
                prefix_ref: prefix_ref.to_string(),
                verb: verb.clone(),
                path,
                line,
                symbol: Some(name.clone()),
            });
        }
    }
}

/// Detects NestJS `@UseGuards(...)` decorator coverage — see module doc "NestJS `@UseGuards` decorator
/// exemption". Returns the set of route-registration lines (matching `IoProvide::line` for whatever
/// `extract_controller_provides` would emit from the same file) covered by an explicit `@UseGuards(...)`
/// chain, either class-level or method-level. Empty set (never panics) on an unparseable file.
pub fn extract_controller_guarded_lines(rel: &str, text: &str) -> HashSet<u32> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return HashSet::new();
    };
    let mut c = GuardedLineCollector {
        cm: &cm,
        out: HashSet::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct GuardedLineCollector<'a> {
    cm: &'a SourceMap,
    out: HashSet<u32>,
}

impl Visit for GuardedLineCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        // Mirrors `emit_method`'s own class/method gating exactly, so a guarded line only appears
        // here if `extract_controller_provides` would also emit a real `IoProvide` for it.
        if let Some(_ctx) = controller_context(&n.class.decorators) {
            let class_guarded = has_use_guards(&n.class.decorators);
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    if class_guarded || has_use_guards(&m.function.decorators) {
                        if let Some((_, decorator, _)) = method_route(&m.function.decorators) {
                            self.out.insert(crate::line_of(self.cm, decorator.span.lo));
                        }
                    }
                }
            }
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

/// True when a decorator's own NAME says it authenticates or gates. Deliberately its own vocabulary
/// rather than a reuse of `router_mounts::guard`'s middleware one, and MUCH narrower — two shapes, and
/// it holds no list, which is the point rather than an economy.
///
/// # Why not the middleware vocabulary
/// That one accepts `permission`, `acl`, `token` and `loggedin`, which are the right words for an
/// argument sitting in front of a handler and the wrong ones here. NestJS puts authorization METADATA
/// decorators BESIDE the guard rather than instead of it — `@Roles('admin')` carries the policy and a
/// `@UseGuards(RolesGuard)` enforces it — so a controller carrying `@Roles` and NO guard is precisely
/// the shape the consuming rules exist to report, and every one of those four words would clear it.
///
/// # The two accepted shapes, and why they need no veto list
/// A name containing the full stem `authentic`/`authoriz`, or a name ending in `guard`. Requiring the
/// STEM rather than the bare word `auth` is what keeps two whole families out without a list to
/// maintain:
///
/// * DOCUMENTATION. Every `@nestjs/swagger` security decorator ends in an auth word and enforces
///   nothing at all — `@ApiBearerAuth`, `@ApiOAuth2`, `@ApiSecurity`, `@ApiCookieAuth`,
///   `@ApiBasicAuth`. None carries the stem, so none is read as evidence, and the day the package adds
///   a sixth it is already covered.
/// * NEGATION. `@SkipAuth`, `@NoAuth`, `@OptionalAuth`, `@BypassAuth` are opt-OUT markers that mean the
///   opposite of what they spell, and they sit on exactly the route a reader most needs told about.
///   None carries the stem either.
///
/// The ONE spelling that gets past the stem and still means its opposite is `@Unauthenticated` /
/// `@Unauthorized`, so `un` is vetoed outright — one condition, not a vocabulary.
///
/// # The accepted cost, stated rather than discovered
/// `@RequireAuth`, `@JwtAuth` and a bare `@Auth` go UNRECOGNIZED. That is under-recognition: the
/// finding still fires and a human still looks, whereas a false guard silently suppresses a real
/// missing gate — the same direction `router_mounts::guard`'s own doc argues for, and the reason this
/// predicate would rather miss a real guard than invent one.
///
/// Added 2026-09-05 off a measurement that opened every first-screen row of three real projects against
/// the source: `http/protected-path-no-auth-evidence` was wrong on 4 of 4, and a HOUSE decorator whose
/// own name says it authenticates was one of the three shapes it could not see. The module doc's "Known
/// residual" section had already named and rejected `@Licensed`/`@GlobalScope`/`@ProjectScope` as
/// `@UseGuards` equivalents — correctly, since none of them authenticates — but a decorator that says
/// it does had never been considered.
fn is_auth_decorator_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // `@Unauthenticated`/`@Unauthorized` carry the accepted stem and mean its opposite.
    if lower.starts_with("un") {
        return false;
    }
    lower.contains("authentic") || lower.contains("authoriz") || lower.ends_with("guard")
}

/// `@UseGuards` by its exact framework spelling, plus any decorator [`is_auth_decorator_name`] judges.
/// The literal stays spelled out rather than folded into the predicate: `UseGuards` ends in `guards`,
/// not `guard`, and letting the framework's own name survive a vocabulary edit is cheaper than the day
/// it does not.
fn has_use_guards(decorators: &[Decorator]) -> bool {
    decorators.iter().any(|d| {
        decorator_name(&d.expr).is_some_and(|n| n == "UseGuards" || is_auth_decorator_name(&n))
    })
}
