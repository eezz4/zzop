//! Spring Security **global authorization posture** extraction — the `authorizeRequests()`/
//! `authorizeHttpRequests(..)` builder chain in a `WebSecurityConfigurerAdapter.configure(HttpSecurity)`
//! or a `SecurityFilterChain` bean — for the `mutating-route-no-auth` rule's route-auth exemption. This
//! is the application-GLOBAL auth mechanism (the residual the rule's own doc names): every route is
//! authenticated-by-default, with an enumerable list of `.permitAll()` exceptions, so — unlike an opaque
//! global guard — it IS route-mappable (a route is authenticated iff it matches no `.permitAll()` matcher).
//!
//! ## Safety: parse-all-or-nothing (a security rule must not false-clear)
//! Exempting a route wrongly (clearing a genuinely-open mutating route) would HIDE a real finding — the
//! dangerous direction. So this extractor is deliberately all-or-nothing: it returns a posture ONLY when
//! BOTH (a) the chain terminates in a PROVABLY authenticated `anyRequest` default, AND (b) EVERY clause
//! configuring the authorization registry is recognized — a matcher (`antMatchers`/`requestMatchers`/
//! `mvcMatchers`) followed by a known terminal (`permitAll`/`authenticated`/`fullyAuthenticated`/
//! `denyAll`), with LITERAL path arguments only. Anything else returns a NAMED bail
//! ([`SpringPostureBail`]) — no posture, no exemption, every finding kept. A missed exemption is a
//! false-positive we already ship; a wrong exemption is a hidden vulnerability.
//!
//! Both registry spellings are read — the classic fluent chain and the Spring-6 lambda DSL — and two
//! `authorizeHttpRequests(..)` customizers on ONE chain are folded, because Spring applies both to the
//! same registry. See [`clauses`] for that half. What still bails, and why:
//! - **`WebSecurity.ignoring(`** anywhere in the file: it bypasses the filter chain ENTIRELY, opening
//!   paths the `authorizeHttpRequests` chain never lists, so a config carrying one could hide an open
//!   mutating route from `permit_all`.
//! - **a chain-level request scoper** (`http.securityMatcher(..)`/`antMatcher(..)`/`requestMatchers()`,
//!   detected as ANY chain-spine method whose name contains `Matcher`, so the guard is robust to the full
//!   deprecated-and-current family rather than a fragile name list): its posture is path-LOCAL, and
//!   applying it tree-wide would false-clear open routes outside the scope.
//! - **a configurer that opens paths of its own** (`formLogin(f -> f.loginPage(..).permitAll())`,
//!   `logout().permitAll()`): those paths never reach `permit_all`. Both this hazard and the scoper above
//!   are checked on the entrypoint's chain AND on every SIBLING statement configuring the same builder —
//!   Spring reads `http.a(); http.b();` exactly as `http.a().b()`, so one spine is not the config. See
//!   [`siblings`].
//! - **more than one independent authorization chain** in the file: config-vs-config scoping is ambiguous.
//! - **a non-literal matcher argument**, an unrecognized clause, or an `anyRequest` default that is not
//!   provably "authenticated" (notably `.access(mgr == null ? … : mgr)`, whose live arm could GRANT).
//!
//! The `.permitAll()` matcher list is intentionally the ONLY thing acted on: an explicit
//! `.antMatchers(...).authenticated()` is redundant with the authenticated default (its routes are exempt
//! anyway), and `.denyAll()` blocks the route entirely (also not an open mutating route) — both are
//! recognized so they don't trigger a bail, but neither adds to the open-route exception list.

use tree_sitter::Node;

use crate::util::node_text;

mod clauses;
mod fold;
mod siblings;
#[cfg(test)]
mod tests;

/// The parsed global authorization posture: a secure-by-default (`anyRequest().authenticated()`) chain
/// plus its enumerated `.permitAll()` exceptions. A route is authenticated (and thus exempt from
/// `mutating-route-no-auth`) iff it matches NONE of `permit_all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpringSecurityPosture {
    pub permit_all: Vec<SpringAntMatcher>,
}

/// One `.antMatchers([HttpMethod.X, ] "pattern"...)` matcher: an optional HTTP method and the ANT path
/// patterns it opens. An empty `patterns` with a `method` set (`antMatchers(HttpMethod.OPTIONS)`) matches
/// every path for that method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpringAntMatcher {
    pub method: Option<String>,
    pub patterns: Vec<String>,
}

/// WHY an extraction produced no posture. Every bail is named rather than a silent `None`, so a later
/// pass can hook the one it knows how to resolve (a property-backed whitelist reaches
/// [`Self::NonLiteralMatcher`], carrying both the unreadable argument and the expression that binds it)
/// and so a "this config exists but we could not read it" self-report can say which shape stopped it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpringPostureBail {
    /// No authorization entrypoint in the file (or it did not parse) — not a security config at all.
    /// This is the overwhelmingly common case: every non-config Java file lands here.
    NotAConfig,
    /// A `WebSecurity.ignoring(` call anywhere in the file.
    WebSecurityIgnoring,
    /// A chain-level request scoper before/after the entrypoint — the posture is path-local, not global.
    ChainScoper,
    /// More than one independent authorization builder chain in the file.
    MultipleChains,
    /// One chain mixes the classic-fluent and lambda-DSL spellings.
    MixedDsl,
    /// A customizer-lambda shape that cannot be enumerated exhaustively; carries the node kind or the
    /// structural reason (`"if_statement"`, `"chain-not-on-parameter"`, …).
    LambdaBody(String),
    /// A matcher argument that is not a literal path. `arg` is its source text; `bound_by` is the
    /// enhanced-for iterable that binds it when the argument is such a loop variable
    /// (`requestMatchers(url)` under `for (String url : ignoreUrlsConfig.getUrls())`).
    NonLiteralMatcher {
        arg: String,
        bound_by: Option<String>,
    },
    /// An `HttpSecurity` configurer on the chain spine that opens paths of its own
    /// (`logout().permitAll()`, `formLogin(f -> f.loginPage(..).permitAll())`); carries its name.
    ConfigurerPermitAll(String),
    /// The builder's use across the enclosing method cannot be enumerated, so a sibling statement could
    /// carry a hazard unseen: the entrypoint chain has no nameable receiver (`"chain-base"`), or a second
    /// name is bound to the same object (`"alias"`). See [`siblings`].
    SiblingScope(String),
    /// A registry clause that is neither a known matcher nor a known terminal; carries its name.
    UnrecognizedClause(String),
    /// The chain never proves an authenticated `anyRequest` default.
    NotSecureByDefault,
    /// `.anyRequest().access(..)` whose argument is not provably `AuthenticatedAuthorizationManager
    /// .authenticated()`; carries the argument source text.
    AnyRequestAccessNotProvable(String),
}

impl SpringPostureBail {
    /// A stable kebab-case id for this bail — the name a consumer matches on and a report prints.
    pub fn name(&self) -> &'static str {
        match self {
            Self::NotAConfig => "not-a-config",
            Self::WebSecurityIgnoring => "web-security-ignoring",
            Self::ChainScoper => "chain-scoper",
            Self::MultipleChains => "multiple-chains",
            Self::MixedDsl => "mixed-dsl",
            Self::LambdaBody(_) => "lambda-body",
            Self::NonLiteralMatcher { .. } => "non-literal-matcher",
            Self::ConfigurerPermitAll(_) => "configurer-permit-all",
            Self::SiblingScope(_) => "sibling-scope",
            Self::UnrecognizedClause(_) => "unrecognized-clause",
            Self::NotSecureByDefault => "not-secure-by-default",
            Self::AnyRequestAccessNotProvable(_) => "any-request-access-not-provable",
        }
    }

    /// The WHICH behind the WHAT — the payload a carrying variant holds, empty for the variants that
    /// carry none. [`Self::name`] answers "what family of shape stopped us" and several families have
    /// many members: `lambda-body` alone covers `"if_statement"`, `"chain-not-on-parameter"`,
    /// `"arguments"`, `"body"`, `"parameters"` and `"expression_statement"`, which take entirely
    /// different work to support. A report that prints only the name sends its reader to open the config
    /// and guess which one they hit — the guessing this enum was made to end (see the type's own doc:
    /// "so a 'this config exists but we could not read it' self-report can say which shape stopped it").
    ///
    /// Added 2026-09-05, one commit after the self-report itself shipped printing the name alone. The
    /// gap was found by reading this enum rather than by a failing test, which is the reason the pin on
    /// the consuming warning now asserts a DETAIL-carrying bail reaches the reply: the name-only version
    /// was green.
    ///
    /// `NonLiteralMatcher` renders both halves because its `bound_by` is the actionable one — it names
    /// the enhanced-for iterable that supplies the matcher, which is how a reader learns the exception
    /// list lives outside the source at all.
    pub fn detail(&self) -> String {
        match self {
            Self::NotAConfig
            | Self::WebSecurityIgnoring
            | Self::ChainScoper
            | Self::MultipleChains
            | Self::MixedDsl
            | Self::NotSecureByDefault => String::new(),
            Self::LambdaBody(d)
            | Self::ConfigurerPermitAll(d)
            | Self::SiblingScope(d)
            | Self::UnrecognizedClause(d)
            | Self::AnyRequestAccessNotProvable(d) => d.clone(),
            Self::NonLiteralMatcher { arg, bound_by } => match bound_by {
                Some(b) => format!("{arg} (bound by {b})"),
                None => arg.clone(),
            },
        }
    }
}

impl SpringSecurityPosture {
    /// Whether a route `(method, path)` is authenticated under this secure-by-default posture — i.e. it
    /// matches NONE of the `permitAll` exceptions. `path` is the route's own path (the `IoProvide` key's
    /// path half, `{}`-normalized). Matching errs toward "matches a permitAll" (the generous direction),
    /// so a route is only reported authenticated when it provably escapes every open matcher — the safe
    /// bias for a security rule (never exempt a route that might be open).
    pub fn route_is_authenticated(&self, method: &str, path: &str) -> bool {
        !self.permit_all.iter().any(|m| m.matches(method, path))
    }
}

impl SpringAntMatcher {
    fn matches(&self, method: &str, path: &str) -> bool {
        if let Some(m) = &self.method {
            if !m.eq_ignore_ascii_case(method) {
                return false;
            }
        }
        // A method-only matcher (`antMatchers(HttpMethod.OPTIONS)`) opens every path for that method.
        self.patterns.is_empty()
            || self
                .patterns
                .iter()
                .any(|p| zzop_core::ant_path_matches(p, path))
    }
}

/// Extract the Spring Security global posture from one Java file, or a NAMED bail if the file has no
/// single fully-recognized secure-by-default authorization chain (see the module doc's safety contract).
pub fn extract_spring_security_posture(
    _rel: &str,
    text: &str,
) -> Result<SpringSecurityPosture, SpringPostureBail> {
    // `WebSecurity.ignoring().antMatchers(...)` (in a `configure(WebSecurity)` method, often the same
    // class) opens paths by bypassing the filter chain ENTIRELY — stronger than `permitAll`, and invisible
    // to the authorization chain this parses. A mutating route on an ignored path is genuinely open, so
    // any config that uses `ignoring` could hide such a route from `permit_all`: bail conservatively
    // rather than risk exempting it. (`ignoring` paths are almost always static GET resources, but the
    // safe posture is to not reason about a config we can't fully see.)
    if text.contains(".ignoring(") {
        return Err(SpringPostureBail::WebSecurityIgnoring);
    }
    let tree = crate::parse_tree(text).ok_or(SpringPostureBail::NotAConfig)?;
    let mut entrypoints = Vec::new();
    collect_authz_entrypoints(tree.root_node(), text, &mut entrypoints);
    let Some(first) = entrypoints.first() else {
        return Err(SpringPostureBail::NotAConfig);
    };

    // Every entrypoint must sit on ONE builder chain. Two `authorizeHttpRequests(..)` calls on the SAME
    // chain configure the same registry and are folded; two on DIFFERENT chains are two postures whose
    // relative scoping we cannot resolve, so they bail.
    let chain = chain_root(*first);
    if entrypoints
        .iter()
        .any(|e| chain_root(*e).id() != chain.id())
    {
        return Err(SpringPostureBail::MultipleChains);
    }
    let groups = clauses::walk_chain(chain, text)?;
    // …and the chain's own spine is not the whole configuration: a SIBLING statement can reconfigure the
    // same `http` object, and Spring treats `http.a(); http.b();` exactly as `http.a().b()`. The two
    // chain-level hazards are re-checked over those statements before any posture is returned.
    siblings::scan(chain, text)?;
    fold::into_posture(&groups, text)
}

/// Collect every `method_invocation` node whose method name is an authorization entrypoint.
fn collect_authz_entrypoints<'a>(node: Node<'a>, src: &str, out: &mut Vec<Node<'a>>) {
    if node.kind() == "method_invocation"
        && node
            .child_by_field_name("name")
            .is_some_and(|n| clauses::AUTHZ_ENTRYPOINTS.contains(&node_text(n, src)))
    {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_authz_entrypoints(child, src, out);
    }
}

/// The outermost `method_invocation` of the builder chain `node` belongs to — ascend while the current
/// node is the parent's `object`. Two entrypoints share a chain iff they share this root.
fn chain_root(node: Node) -> Node {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() != "method_invocation" || parent.child_by_field_name("object") != Some(cur)
        {
            break;
        }
        cur = parent;
    }
    cur
}
