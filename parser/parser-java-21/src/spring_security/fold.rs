//! The decision layer of [`super::extract_spring_security_posture`]: folding the collected
//! authorization-registry clause groups into one posture, or into the NAMED bail that says which shape
//! stopped it. Split from [`super::clauses`] (which only COLLECTS) to stay under the per-file line cap;
//! every item is `pub(super)` with one caller.
//!
//! Two directions are not symmetric here, and the asymmetry is the whole safety argument: over-collecting
//! `permitAll` only SHRINKS the exempt set (safe), while assuming an authenticated default, or skipping a
//! clause that might have opened a path, would clear a genuinely-open route (the defect this rule exists
//! to catch). So every unreadable shape bails rather than being ignored.

use tree_sitter::Node;

use crate::util::{node_text, valid_named_children};

use super::clauses::ClauseGroup;
use super::{SpringAntMatcher, SpringPostureBail, SpringSecurityPosture};

const MATCHER_METHODS: &[&str] = &["antMatchers", "requestMatchers", "mvcMatchers"];
/// Chain terminals that mean "not an open route" (recognized so they don't force a bail; only `permitAll`
/// is acted on). `denyAll` blocks entirely; `authenticated`/`fullyAuthenticated` require auth.
const CLOSED_TERMINALS: &[&str] = &["authenticated", "fullyAuthenticated", "denyAll"];
/// The ONLY `AuthorizationManager` whose identity proves "authenticated by default" for an
/// `.anyRequest().access(..)` terminal. Matched on the last dot-segment so a fully-qualified spelling
/// works, and on the CLASS rather than on the expression, so a ternary/variable/bean reference — which
/// could resolve to a manager that GRANTS — can never be read as secure-by-default.
const AUTHENTICATED_MANAGER: &str = "AuthenticatedAuthorizationManager";

/// Fold every clause group into one posture, or bail on ANY unrecognized shape. Matcher/terminal pairs
/// are read in order; only `permitAll` matchers reach `permit_all`, and the posture is returned ONLY if
/// some group proved `anyRequest` is authenticated by default.
pub(super) fn into_posture(
    groups: &[ClauseGroup],
    src: &str,
) -> Result<SpringSecurityPosture, SpringPostureBail> {
    let mut permit_all = Vec::new();
    let mut default_authenticated = false;
    for group in groups {
        let mut i = 0;
        while i < group.clauses.len() {
            let (name, args) = &group.clauses[i];
            // every matcher/anyRequest needs a following terminal
            let Some((term, term_args)) = group.clauses.get(i + 1) else {
                return Err(SpringPostureBail::UnrecognizedClause(name.clone()));
            };
            if MATCHER_METHODS.contains(&name.as_str()) {
                let matcher = parse_matcher(*args, group.bound.as_ref(), src)?;
                if term == "permitAll" {
                    permit_all.push(matcher);
                } else if !CLOSED_TERMINALS.contains(&term.as_str()) {
                    return Err(SpringPostureBail::UnrecognizedClause(term.clone()));
                }
            } else if name == "anyRequest" {
                default_authenticated |= any_request_is_authenticated(term, *term_args, src)?;
            } else {
                return Err(SpringPostureBail::UnrecognizedClause(name.clone()));
            }
            i += 2;
        }
    }
    if !default_authenticated {
        return Err(SpringPostureBail::NotSecureByDefault);
    }
    Ok(SpringSecurityPosture { permit_all })
}

/// Whether an `.anyRequest().<term>(..)` terminal proves the default is "authenticated".
/// `.access(..)` counts ONLY when its single argument is literally
/// `AuthenticatedAuthorizationManager.authenticated()`/`.fullyAuthenticated()`. Anything else — a
/// ternary (`mgr == null ? AuthenticatedAuthorizationManager.authenticated() : mgr`), a bean reference, a
/// SpEL string — could resolve to a manager that GRANTS, and reading it as secure-by-default would clear
/// routes that manager might open: the dangerous direction, so it bails.
fn any_request_is_authenticated(
    term: &str,
    args: Option<Node>,
    src: &str,
) -> Result<bool, SpringPostureBail> {
    if term == "authenticated" || term == "fullyAuthenticated" {
        return Ok(true);
    }
    if term != "access" {
        return Err(SpringPostureBail::NotSecureByDefault);
    }
    let provable = args.is_some_and(|args| match valid_named_children(args).as_slice() {
        [arg] if arg.kind() == "method_invocation" => {
            arg.child_by_field_name("name").is_some_and(|n| {
                matches!(node_text(n, src), "authenticated" | "fullyAuthenticated")
            }) && arg.child_by_field_name("object").is_some_and(|o| {
                node_text(o, src).rsplit('.').next() == Some(AUTHENTICATED_MANAGER)
            })
        }
        _ => false,
    });
    if provable {
        return Ok(true);
    }
    Err(SpringPostureBail::AnyRequestAccessNotProvable(
        args.map_or(String::new(), |a| node_text(a, src).to_string()),
    ))
}

/// Parse one matcher's argument list into `SpringAntMatcher`, or bail if any argument is neither a
/// `HttpMethod.X` (first position only) nor a string literal — a non-literal path cannot be reasoned
/// about, and skipping it would silently drop an open path from `permit_all`.
fn parse_matcher(
    args: Option<Node>,
    bound: Option<&(String, String)>,
    src: &str,
) -> Result<SpringAntMatcher, SpringPostureBail> {
    let nonliteral = |text: &str| SpringPostureBail::NonLiteralMatcher {
        arg: text.to_string(),
        bound_by: bound
            .filter(|(var, _)| var == text)
            .map(|(_, iterable)| iterable.clone()),
    };
    let Some(args) = args else {
        return Err(nonliteral(""));
    };
    // A malformed argument subtree (an ERROR/MISSING node) is dropped by `valid_named_children`, which
    // would let a non-literal/unparsed arg pass unseen — bail on any parse error in the args (safe).
    if args.has_error() {
        return Err(nonliteral(node_text(args, src)));
    }
    let mut method = None;
    let mut patterns = Vec::new();
    for (idx, arg) in valid_named_children(args).into_iter().enumerate() {
        match arg.kind() {
            "string_literal" => patterns.push(node_text(arg, src).trim_matches('"').to_string()),
            // `HttpMethod.GET` — only valid as the FIRST argument.
            "field_access" if idx == 0 => {
                let field = arg
                    .child_by_field_name("field")
                    .ok_or_else(|| nonliteral(node_text(arg, src)))?;
                method = Some(node_text(field, src).to_string());
            }
            _ => return Err(nonliteral(node_text(arg, src))),
        }
    }
    Ok(SpringAntMatcher { method, patterns })
}
