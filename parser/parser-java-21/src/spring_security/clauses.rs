//! The clause layer of [`super::extract_spring_security_posture`]: turning ONE Spring Security builder
//! chain into the authorization-registry clause sequences it configures, and folding those into a
//! posture. Split out of the parent module purely to stay under the repo's per-file line cap — every
//! item is `pub(super)` and has exactly one caller.
//!
//! Two spellings reach the same registry, and both are read here:
//! - **classic fluent** — `http.authorizeRequests().antMatchers(..).permitAll().anyRequest().authenticated()`.
//!   The registry clauses ARE the outer chain's own tail, so everything after the entrypoint is a clause.
//! - **lambda DSL (Spring 6)** — `http.authorizeHttpRequests(reg -> reg.requestMatchers(..).permitAll())`.
//!   The registry clauses live INSIDE the customizer lambda, chained off its parameter; the outer chain's
//!   tail is plain `HttpSecurity` configuration (`csrf`/`sessionManagement`/`addFilterBefore`/…) and is
//!   skipped — except that any tail method whose name contains `Matcher` is a chain-level scoper and
//!   still bails, exactly as one before the entrypoint does.
//!
//! This module reads ONE spine. The same builder object can also be configured by SIBLING STATEMENTS,
//! which Spring treats identically and which no spine walk can see — [`super::siblings`] re-runs
//! [`spine_hazard`] over those.
//!
//! Spring applies EVERY `authorizeHttpRequests(..)` customizer on a chain to the SAME registry, so two
//! such calls on one chain are FOLDED (matchers from the first, the `anyRequest` terminal from the
//! second) rather than treated as two ambiguous chains. Two calls on two DIFFERENT chains are still a
//! bail — the caller groups by chain root before getting here.
//!
//! The all-or-nothing safety contract of the parent module is unchanged: over-collecting `permitAll` is
//! the SAFE direction (it shrinks the exempt set), so an unreadable shape must bail rather than be
//! skipped, and `default_authenticated` must be provable rather than assumed.

use tree_sitter::Node;

use crate::util::{node_text, valid_named_children};

use super::SpringPostureBail;

pub(super) const AUTHZ_ENTRYPOINTS: &[&str] = &["authorizeRequests", "authorizeHttpRequests"];

/// One `.name(args)` link of an authorization-registry chain.
pub(super) type Clause<'a> = (String, Option<Node<'a>>);

/// The clauses of ONE registry statement, plus the enhanced-for binding (loop variable, iterable source
/// text) it sits under when it does — carried only so a non-literal matcher argument can name what binds
/// it (`registry.requestMatchers(url)` inside `for (String url : ignoreUrlsConfig.getUrls())` bails with
/// `bound_by = "ignoreUrlsConfig.getUrls()"`). Clauses are grouped per statement because the fold reads
/// them in matcher/terminal PAIRS: flattening two statements into one list would desync the pairing
/// whenever a statement has an odd clause count.
pub(super) struct ClauseGroup<'a> {
    pub(super) clauses: Vec<Clause<'a>>,
    pub(super) bound: Option<(String, String)>,
}

/// Walk one builder chain top-to-bottom and collect every authorization-registry clause group it
/// configures. `chain` is the chain's ROOT invocation (the outermost link); the caller has already
/// proven every entrypoint in the file belongs to this one chain.
pub(super) fn walk_chain<'a>(
    chain: Node<'a>,
    src: &str,
) -> Result<Vec<ClauseGroup<'a>>, SpringPostureBail> {
    let mut groups = Vec::new();
    // `Some` once the classic-fluent entrypoint is seen — from there the chain's tail IS the registry.
    let mut fluent: Option<Vec<Clause>> = None;
    let mut lambda_seen = false;
    for node in spine(chain) {
        let name = node
            .child_by_field_name("name")
            .map_or("", |n| node_text(n, src));
        let args = node.child_by_field_name("arguments");
        if AUTHZ_ENTRYPOINTS.contains(&name) {
            if fluent.is_some() {
                return Err(SpringPostureBail::MixedDsl);
            }
            match entry_customizer(args)? {
                Some(lambda) => {
                    lambda_seen = true;
                    collect_groups(
                        lambda_body(lambda, src)?,
                        &lambda_param(lambda, src)?,
                        None,
                        src,
                        &mut groups,
                    )?;
                }
                None if lambda_seen => return Err(SpringPostureBail::MixedDsl),
                None => fluent = Some(Vec::new()),
            }
        } else if let Some(tail) = fluent.as_mut() {
            tail.push((name.to_string(), args));
        } else if let Some(bail) = spine_hazard(name, args, src) {
            return Err(bail);
        }
    }
    if let Some(tail) = fluent {
        groups.push(ClauseGroup {
            clauses: tail,
            bound: None,
        });
    }
    Ok(groups)
}

/// The links of ONE builder chain in source order (the innermost call first). `chain` is the chain's ROOT
/// invocation; the walk descends the `object` field and stops at the first non-invocation base.
pub(super) fn spine(chain: Node) -> Vec<Node> {
    let mut spine = Vec::new();
    let mut cur = Some(chain);
    while let Some(node) = cur {
        if node.kind() != "method_invocation" {
            break;
        }
        spine.push(node);
        cur = node.child_by_field_name("object");
    }
    spine.reverse();
    spine
}

/// The two CHAIN-LEVEL hazards one builder link can carry, or `None` when it is ordinary `HttpSecurity`
/// configuration. Separate from [`walk_chain`] because [`super::siblings`] runs the SAME two checks over
/// the statements that configure the same builder object without being on this chain — Spring applies
/// both spellings to one filter chain, so a hazard hidden in a sibling statement is exactly as dangerous.
///
/// Only ever called on the OUTER `HttpSecurity` chain: in classic-fluent mode the tail after the
/// entrypoint IS the authorization registry and is read as clauses instead, so a legitimate
/// `.antMatchers(..).permitAll()` never reaches here.
pub(super) fn spine_hazard(name: &str, args: Option<Node>, src: &str) -> Option<SpringPostureBail> {
    if name.contains("Matcher") {
        // A chain-level request SCOPER (`securityMatcher`/`antMatcher`/`requestMatchers()`/…) narrows the
        // whole chain to a path subset, so its posture is NOT global. Matched by "contains Matcher" rather
        // than an enumerated list: every `HttpSecurity` scoping method is spelled that way and no
        // non-scoping builder method on the chain spine is.
        return Some(SpringPostureBail::ChainScoper);
    }
    if name == "permitAll" || args.is_some_and(|a| node_text(a, src).contains("permitAll")) {
        // A CONFIGURER shortcut that opens paths the authorization registry never lists —
        // `http.logout().permitAll()`, `formLogin(f -> f.loginPage("/login").permitAll())`. Those paths
        // are invisible to `permit_all`, exactly like `WebSecurity.ignoring(`, so a mutating route on one
        // could be wrongly exempted.
        return Some(SpringPostureBail::ConfigurerPermitAll(name.to_string()));
    }
    None
}

/// The entrypoint's single `Customizer` lambda argument, or `None` for the zero-argument classic-fluent
/// spelling. Any OTHER argument shape (`Customizer.withDefaults()`, a method reference, a named field) is
/// a customizer whose clauses live somewhere this parse cannot follow — bail.
fn entry_customizer(args: Option<Node>) -> Result<Option<Node>, SpringPostureBail> {
    let Some(args) = args else {
        return Ok(None);
    };
    if args.has_error() {
        return Err(SpringPostureBail::LambdaBody("arguments".into()));
    }
    match valid_named_children(args).as_slice() {
        [] => Ok(None),
        [one] if one.kind() == "lambda_expression" => Ok(Some(*one)),
        _ => Err(SpringPostureBail::LambdaBody("customizer".into())),
    }
}

fn lambda_body<'a>(lambda: Node<'a>, _src: &str) -> Result<Node<'a>, SpringPostureBail> {
    lambda
        .child_by_field_name("body")
        .ok_or_else(|| SpringPostureBail::LambdaBody("body".into()))
}

/// The customizer lambda's parameter NAME — the identifier every registry chain in its body must be
/// rooted at. `reg ->`, `(reg) ->` and `(RegistryType reg) ->` all resolve; anything else bails.
fn lambda_param(lambda: Node, src: &str) -> Result<String, SpringPostureBail> {
    let bail = || SpringPostureBail::LambdaBody("parameters".into());
    let params = lambda.child_by_field_name("parameters").ok_or_else(bail)?;
    if params.kind() == "identifier" {
        return Ok(node_text(params, src).to_string());
    }
    match valid_named_children(params).as_slice() {
        [one] if one.kind() == "identifier" => Ok(node_text(*one, src).to_string()),
        [one] if one.kind() == "formal_parameter" => one
            .child_by_field_name("name")
            .map(|n| node_text(n, src).to_string())
            .ok_or_else(bail),
        _ => Err(bail()),
    }
}

/// Collect the registry clause groups a customizer-lambda body configures. EVERY statement must be
/// recognized — a shape this cannot enumerate (an `if`, a local declaration, a nested lambda, a `return`)
/// could hide a `permitAll` and so bails, naming the node kind it choked on.
fn collect_groups<'a>(
    node: Node<'a>,
    param: &str,
    bound: Option<&(String, String)>,
    src: &str,
    out: &mut Vec<ClauseGroup<'a>>,
) -> Result<(), SpringPostureBail> {
    match node.kind() {
        "block" => {
            for stmt in valid_named_children(node) {
                collect_groups(stmt, param, bound, src, out)?;
            }
        }
        "expression_statement" => match valid_named_children(node).as_slice() {
            [inner] => collect_groups(*inner, param, bound, src, out)?,
            _ => return Err(SpringPostureBail::LambdaBody("expression_statement".into())),
        },
        // `for (String url : cfg.getUrls()) registry.requestMatchers(url).permitAll();` — mall's shape.
        // Recognized so the bail lands on the non-literal MATCHER (naming the iterable that binds it)
        // rather than on the loop, which is what a later property-resolution pass has to hook.
        "enhanced_for_statement" => {
            let (Some(var), Some(iter), Some(body)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("value"),
                node.child_by_field_name("body"),
            ) else {
                return Err(SpringPostureBail::LambdaBody(
                    "enhanced_for_statement".into(),
                ));
            };
            let binding = (
                node_text(var, src).to_string(),
                node_text(iter, src).to_string(),
            );
            collect_groups(body, param, Some(&binding), src, out)?;
        }
        "method_invocation" => out.push(ClauseGroup {
            clauses: registry_chain(node, param, src)?,
            bound: bound.cloned(),
        }),
        // A COMMENT IS NOT A CLAUSE. tree-sitter-java makes `line_comment`/`block_comment` NAMED
        // nodes, and `valid_named_children` filters only errors and missing nodes, so a comment
        // reached the catch-all below and bailed the whole config. Measured on macrozheng/mall
        // (2026-09-05): its customizer lambda opens with a non-English `//` comment line, and that
        // single line stopped a chain this pass otherwise handles — the `enhanced_for_statement` arm
        // above was written FOR mall's shape and had never once been reached on mall.
        //
        // Skipped HERE rather than inside `valid_named_children`: that helper serves every extractor
        // in this crate, and a comment is meaningful to some of them (a suppression marker is a
        // comment). Narrow the skip to the place where a comment provably carries no clause.
        "line_comment" | "block_comment" => {}
        kind => return Err(SpringPostureBail::LambdaBody(kind.to_string())),
    }
    Ok(())
}

/// One registry statement's clauses in source order. The chain's base object MUST be the customizer
/// lambda's own parameter — a chain rooted at anything else (`http.…`, a field, another builder) is not
/// a registry configuration this pass can account for.
fn registry_chain<'a>(
    node: Node<'a>,
    param: &str,
    src: &str,
) -> Result<Vec<Clause<'a>>, SpringPostureBail> {
    let mut clauses = Vec::new();
    let mut cur = node;
    let base = loop {
        clauses.push((
            cur.child_by_field_name("name")
                .map_or(String::new(), |n| node_text(n, src).to_string()),
            cur.child_by_field_name("arguments"),
        ));
        let Some(object) = cur.child_by_field_name("object") else {
            break None;
        };
        if object.kind() != "method_invocation" {
            break Some(object);
        }
        cur = object;
    };
    if !base.is_some_and(|b| b.kind() == "identifier" && node_text(b, src) == param) {
        return Err(SpringPostureBail::LambdaBody(
            "chain-not-on-parameter".into(),
        ));
    }
    clauses.reverse();
    Ok(clauses)
}
