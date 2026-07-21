//! Spring Security **global authorization posture** extraction — the `http.authorizeRequests()...
//! .anyRequest().authenticated()` builder chain in a `WebSecurityConfigurerAdapter.configure(HttpSecurity)`
//! (or a `SecurityFilterChain` bean) — for the `mutating-route-no-auth` rule's route-auth exemption. This
//! is the application-GLOBAL auth mechanism (the residual the rule's own doc names): every route is
//! authenticated-by-default, with an enumerable list of `.permitAll()` exceptions, so — unlike an opaque
//! global guard — it IS route-mappable (a route is authenticated iff it matches no `.permitAll()` matcher).
//!
//! ## Safety: parse-all-or-nothing (a security rule must not false-clear)
//! Exempting a route wrongly (clearing a genuinely-open mutating route) would HIDE a real finding — the
//! dangerous direction. So this extractor is deliberately all-or-nothing: it returns a posture ONLY when
//! BOTH (a) the chain terminates in `.anyRequest().authenticated()`/`.fullyAuthenticated()`
//! (secure-by-default — any other `anyRequest` terminal, or none, means the default is not "authenticated"
//! and we cannot safely infer any route is guarded), AND (b) EVERY clause after `authorizeRequests`/
//! `authorizeHttpRequests` is recognized — a matcher (`antMatchers`/`requestMatchers`/`mvcMatchers`)
//! followed by a known terminal (`permitAll`/`authenticated`/`fullyAuthenticated`/`denyAll`), with LITERAL
//! path arguments only.
//! It also bails on anything that makes the posture NON-global or opens routes it can't see: a chain-level
//! request scoper before the entrypoint (`http.antMatcher(...)`/`securityMatcher`/`regexMatcher`/
//! `requestMatchers()` — detected as ANY object-side method whose name contains `Matcher`, so the guard is
//! robust to the full deprecated-and-current family, not a fragile name list), and a `WebSecurity.ignoring(`
//! call anywhere in the file (it bypasses the filter chain entirely, opening paths the `authorizeRequests`
//! chain never lists). Any unrecognized clause (a `.hasRole(...)`/`.access(...)` restriction, a non-literal
//! path, the lambda-DSL form), or more than one authorization chain in the file, likewise returns `None` —
//! no posture, no exemption, every finding kept. A missed exemption is a false-positive we already ship; a
//! wrong exemption is a hidden vulnerability. Only the classic fluent `WebSecurityConfigurerAdapter` form is
//! parsed; the Spring-6 lambda DSL is future work (bails safely).
//!
//! The `.permitAll()` matcher list is intentionally the ONLY thing acted on: an explicit
//! `.antMatchers(...).authenticated()` is redundant with the authenticated default (its routes are exempt
//! anyway), and `.denyAll()` blocks the route entirely (also not an open mutating route) — both are
//! recognized so they don't trigger a bail, but neither adds to the open-route exception list.

use tree_sitter::Node;

use crate::util::{node_text, valid_named_children};

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
        self.patterns.is_empty() || self.patterns.iter().any(|p| ant_matches(p, path))
    }
}

/// Spring `AntPathMatcher`-style match of an ANT `pattern` against a concrete `path`, segment-based:
/// `**` matches zero or more whole segments (so `/articles/**` matches `/articles` AND `/articles/{}`),
/// `*` matches exactly one segment, a literal segment matches itself. A route path's `{}` param
/// placeholder is an ordinary segment (a literal pattern segment won't equal it; `*`/`**` will).
fn ant_matches(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.trim_start_matches('/').split('/').collect();
    let s: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    seg_match(&p, &s)
}

fn seg_match(pat: &[&str], seg: &[&str]) -> bool {
    match pat.split_first() {
        None => seg.is_empty(),
        Some((&"**", rest)) => {
            // `**` consumes zero or more segments — a trailing `**` matches any remainder (incl. none).
            rest.is_empty() || (0..=seg.len()).any(|k| seg_match(rest, &seg[k..]))
        }
        Some((&ph, prest)) => match seg.split_first() {
            Some((&sh, srest)) if seg_glob(ph, sh) => seg_match(prest, srest),
            _ => false,
        },
    }
}

/// Single-segment match: `*` matches any one segment; a Spring path-variable segment `{id}` matches any
/// one segment too (crucially, it matches the route path's own `{}` param placeholder — the two mirror
/// halves normalize path variables differently, `http_interface_key` to `{}` and the matcher to `{id}`,
/// and this reconciles them so a `permitAll("/users/{id}")` is NOT under-matched against `/users/{}`);
/// a within-segment glob (`feed*`/`user?`, rare) is treated permissively as a match; else a literal must be
/// equal. Every non-exact case errs toward MATCH — the generous/safe direction (matching a permitAll means
/// NOT exempting the route, so an open route is never wrongly cleared).
fn seg_glob(pat: &str, seg: &str) -> bool {
    pat == "*"
        || pat == seg
        || (pat.starts_with('{') && pat.ends_with('}'))
        || pat.contains('*')
        || pat.contains('?')
}

const MATCHER_METHODS: &[&str] = &["antMatchers", "requestMatchers", "mvcMatchers"];
const AUTHZ_ENTRYPOINTS: &[&str] = &["authorizeRequests", "authorizeHttpRequests"];
/// Chain terminals that mean "not an open route" (recognized so they don't force a bail; only `permitAll`
/// is acted on). `denyAll` blocks entirely; `authenticated`/`fullyAuthenticated` require auth.
const CLOSED_TERMINALS: &[&str] = &["authenticated", "fullyAuthenticated", "denyAll"];

/// Extract the Spring Security global posture from one Java file, or `None` if the file has no single
/// fully-recognized secure-by-default authorization chain (see the module doc's safety contract).
pub fn extract_spring_security_posture(_rel: &str, text: &str) -> Option<SpringSecurityPosture> {
    // `WebSecurity.ignoring().antMatchers(...)` (in a `configure(WebSecurity)` method, often the same
    // class) opens paths by bypassing the filter chain ENTIRELY — stronger than `permitAll`, and invisible
    // to the `authorizeRequests` chain this parses. A mutating route on an ignored path is genuinely open,
    // so any config that uses `ignoring` could hide such a route from `permit_all`: bail conservatively
    // rather than risk exempting it. (`ignoring` paths are almost always static GET resources, but the
    // safe posture is to not reason about a config we can't fully see.)
    if text.contains(".ignoring(") {
        return None;
    }
    let tree = crate::parse_tree(text)?;
    let root = tree.root_node();

    // Find every `authorizeRequests`/`authorizeHttpRequests` call. Exactly one → parse it; else bail
    // (zero = not a config; more than one = multiple/ambiguous chains, unsafe to reason about).
    let mut entrypoints = Vec::new();
    collect_authz_entrypoints(root, text, &mut entrypoints);
    let [entry] = entrypoints.as_slice() else {
        return None;
    };

    // A chain-level request scoper BEFORE the entrypoint — `http.antMatcher("/api/**").authorizeRequests()`
    // / `securityMatcher`/`mvcMatcher`/`requestMatcher` (SINGULAR) — narrows the whole chain to a path
    // subset, so its posture is NOT global and must never be applied tree/module-wide (it would false-clear
    // open routes OUTSIDE the scope). Such a scoper sits on the entrypoint's `object` side, invisible to the
    // upward `ascend_chain` walk, so descend the object chain and bail if one is present.
    if chain_has_scoper(*entry, text) {
        return None;
    }

    // Ascend the method chain from the entrypoint, collecting each following `.clause(args)` in source
    // order (the parents whose `object` is the node below).
    let clauses = ascend_chain(*entry, text);
    parse_clauses(&clauses, text)
}

/// Collect every `method_invocation` node whose method name is an authorization entrypoint.
fn collect_authz_entrypoints<'a>(node: Node<'a>, src: &str, out: &mut Vec<Node<'a>>) {
    if node.kind() == "method_invocation"
        && node
            .child_by_field_name("name")
            .is_some_and(|n| AUTHZ_ENTRYPOINTS.contains(&node_text(n, src)))
    {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_authz_entrypoints(child, src, out);
    }
}

/// True when the entrypoint's `object` chain (everything BEFORE `authorizeRequests`) contains a
/// chain-level request SCOPER. Rather than enumerate the scoping method names (fragile: `securityMatcher`/
/// `antMatcher`/`mvcMatcher`/`regexMatcher`/`requestMatcher` singular AND the `requestMatchers()`/
/// `securityMatchers()` plural chain-level entrypoints, plus whatever future Spring adds), this matches ANY
/// object-side method whose name contains `Matcher` — every `HttpSecurity` scoping method is spelled that
/// way and NO non-scoping builder method on the pre-`authorizeRequests` spine (`csrf`/`cors`/`and`/
/// `sessionManagement`/`exceptionHandling`/…) contains it, so the broad match is both robust and free of
/// false bails. A scoped chain's posture is path-local, not global, and applying it would false-clear open
/// routes outside the scope — so its presence forces the whole extraction to bail.
fn chain_has_scoper(entry: Node, src: &str) -> bool {
    let mut cur = entry.child_by_field_name("object");
    while let Some(node) = cur {
        if node.kind() != "method_invocation" {
            break;
        }
        if node
            .child_by_field_name("name")
            .is_some_and(|n| node_text(n, src).contains("Matcher"))
        {
            return true;
        }
        cur = node.child_by_field_name("object");
    }
    false
}

/// The clauses chained after `entry`, in source order: ascend parent `method_invocation`s where the
/// current node is the `object`. Each yields `(method_name_node, arguments_node_opt)`.
fn ascend_chain<'a>(entry: Node<'a>, src: &str) -> Vec<(String, Option<Node<'a>>)> {
    let mut out = Vec::new();
    let mut cur = entry;
    while let Some(parent) = cur.parent() {
        if parent.kind() != "method_invocation" || parent.child_by_field_name("object") != Some(cur)
        {
            break;
        }
        if let Some(name) = parent.child_by_field_name("name") {
            out.push((
                node_text(name, src).to_string(),
                parent.child_by_field_name("arguments"),
            ));
        }
        cur = parent;
    }
    out
}

/// Fold the clause sequence into a posture, or `None` on ANY unrecognized shape (safety bail).
fn parse_clauses(clauses: &[(String, Option<Node>)], src: &str) -> Option<SpringSecurityPosture> {
    let mut permit_all = Vec::new();
    let mut default_authenticated = false;
    let mut i = 0;
    while i < clauses.len() {
        let (name, args) = &clauses[i];
        let (term, _) = clauses.get(i + 1)?; // every matcher/anyRequest needs a following terminal
        if MATCHER_METHODS.contains(&name.as_str()) {
            let matcher = parse_matcher(*args, src)?; // non-literal args -> bail
            if term == "permitAll" {
                permit_all.push(matcher);
            } else if !CLOSED_TERMINALS.contains(&term.as_str()) {
                return None; // unrecognized terminal after a matcher
            }
        } else if name == "anyRequest" {
            if term == "authenticated" || term == "fullyAuthenticated" {
                default_authenticated = true;
            } else {
                return None; // anyRequest not authenticated -> not secure-by-default -> bail
            }
        } else {
            return None; // unrecognized clause
        }
        i += 2;
    }
    default_authenticated.then_some(SpringSecurityPosture { permit_all })
}

/// Parse one matcher's argument list into `SpringAntMatcher`, or `None` if any argument is neither a
/// `HttpMethod.X` (first position only) nor a string literal (a non-literal path can't be reasoned about).
fn parse_matcher(args: Option<Node>, src: &str) -> Option<SpringAntMatcher> {
    let args = args?;
    // A malformed argument subtree (an ERROR/MISSING node) is dropped by `valid_named_children`, which
    // would let a non-literal/unparsed arg pass unseen — bail on any parse error in the args (safe).
    if args.has_error() {
        return None;
    }
    let mut method = None;
    let mut patterns = Vec::new();
    for (idx, arg) in valid_named_children(args).into_iter().enumerate() {
        match arg.kind() {
            "string_literal" => patterns.push(strip_string(node_text(arg, src))),
            // `HttpMethod.GET` — only valid as the FIRST argument.
            "field_access" if idx == 0 => {
                let field = arg.child_by_field_name("field")?;
                method = Some(node_text(field, src).to_string());
            }
            _ => return None, // a variable, concatenation, or other non-literal -> bail
        }
    }
    Some(SpringAntMatcher { method, patterns })
}

/// Strip the surrounding quotes from a Java string-literal node's text (`"/x"` -> `/x`).
fn strip_string(raw: &str) -> String {
    raw.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests;
