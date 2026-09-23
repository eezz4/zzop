//! ROUTE-GROUP PREFIX resolution for minimal-API registrations — the answer to "what path prefix does
//! this `MapGet` receiver carry?", split out of [`super::minimal_api`] because it is the only part of
//! that adapter that needs state beyond the one call site.
//!
//! It resolves TWO shapes the direct-receiver v1 could not, both measured on dotnet/eShop @ae71a061,
//! where `grep -rn --include=*.cs MapGroup` returns exactly 5 lines and **every one of them** has both:
//!
//! 1. **A BUILDER CALL AFTER `MapGroup`** — `vApi.MapGroup("api/catalog").HasApiVersion(1, 0)`. v1's
//!    prefix reader required the receiver to be EXACTLY `X.MapGroup(literal)`, so a single
//!    `.HasApiVersion(...)` on the end put the prefix out of reach. [`unwind_chain`] walks the whole
//!    receiver chain instead of matching one link.
//! 2. **A CROSS-STATEMENT GROUP VARIABLE** — `var api = …; api.MapGet("/items", …)`. A bare receiver
//!    that is not `app` was skipped outright (never-guess), which is why these produced nothing rather
//!    than a wrong key. [`collect_group_variables`] reads the file's `variable_declarator` initializers
//!    and gives the bare identifier a KNOWN prefix, so the skip is no longer needed for those names.
//!
//! Neither alone moves eShop — every site needs both — which is why they land together.
//!
//! WHAT IS STILL REFUSED RATHER THAN GUESSED:
//! - A `MapGroup` whose argument is not a plain string literal poisons its whole chain: the receiver
//!   resolves to `None` and the registration is skipped, never keyed on the remaining literals.
//! - A name declared TWICE IN ONE FILE with two different prefixes is AMBIGUOUS and resolves to
//!   nothing. The map is file-scoped, so two methods in one file that each spell `var api = …` with
//!   different groups silence each other rather than letting one speak for the other.
//! - A bare identifier that no declarator in this file initializes from a `MapGroup` chain stays
//!   unknown, and [`super::minimal_api`] skips it exactly as before — UNLESS the run DECLARED it as a
//!   root route builder ([`CSharpRouteVocab::root_route_builder_variable_names`], config key
//!   `vocabulary.csharpRootRouteBuilderVariableNames`). What a project calls that variable is a name the
//!   project picks, so it is declared rather than guessed; [`DEFAULT_ROOT_ROUTE_BUILDER_VARIABLE_NAMES`]
//!   carries the census of what zzop's own suggested value reaches and misses, and the two cheaper
//!   alternatives that were measured and rejected.
//!
//! THE ONE PLACE AN UNKNOWN IS TREATED AS EMPTY is inherited from the shipped v1 contract rather than
//! introduced here: when a chain DOES contain a literal `MapGroup`, its head is not required to be
//! recognizable — `vApi.MapGroup("api/catalog")` keys on `api/catalog` without knowing what `vApi` is,
//! exactly as `X.MapGroup("/p").MapGet(...)` has since v0.20.0. In eShop that head is
//! `app.NewVersionedApi("Catalog")`, an Asp.Versioning builder that contributes no path segment.

use std::collections::BTreeMap;

use tree_sitter::Node;

#[cfg(test)]
mod vocab_tests;

use crate::util::{node_text, string_literal_text, valid_named_children};

/// zzop's own suggested value for [`CSharpRouteVocab::root_route_builder_variable_names`] — the single
/// name `app`, which is what this adapter hardcoded from v0.20.0 until 2026-09-11.
///
/// 🔴 IT IS NOT NEARLY UNIVERSAL, and it carried the word "near-universal" for that whole time without
/// anyone counting. Bare identifiers receiving a `Map<Verb>(` call across `corpus/frameworks/aspnetcore`
/// (1990 sites):
///
/// ```text
/// app 983 | builder 527 | endpoints 106 | webApp 62 | routes 16 | endpointRouteBuilder 12
/// endpoint 11 | _builder 10 | rb 4 | _collectorApp 3 | router 1 | ... (17 further names)
/// ```
///
/// So this default reaches **983 of 1990 (49%)** and leaves 854 sites, under 17 names the project chose
/// for itself, to be declared. That is the exact failure `crates/config/config-surface.json` says the
/// whole `vocabulary` block exists to prevent, and until 2026-09-11 this value was a hidden judgment
/// rather than a knob: a project that spells its root `builder` got zero minimal-API routes and NO
/// `configWarnings` entry saying so. PRE-EXISTING DEBT, SURFACED — not debt this key introduced. The
/// literal `"app"` shipped inside `super::minimal_api`'s receiver test (`node_text(receiver, src) ==
/// "app"`) since v0.20.0, and `scripts/policy-census.txt` had no row for it, so
/// `scripts/check-convention-vocab-declarable.sh` was blind to it. Promoting it to a named constant is
/// what made that guard fire; the under-extraction it names is four releases old.
///
/// Two cheaper escapes were measured on 2026-09-11 and BOTH are wrong, recorded so neither is retried:
/// - **Treat any non-group bare identifier as the root.** Adds 854 aspnetcore sites, and the keys are
///   not safe: `CookieTests.cs:2080` declares `void AddChallengeAndForbidEndpoints(IEndpointRouteBuilder
///   routeGroup)` and calls it twice, with `endpoints.MapGroup("/api")` and with that group's own
///   `.MapGroup("/jk")`. Its two `routeGroup.MapGet` sites are FOUR real registrations under `/api/...`
///   and `/api/jk/...`; keying them bare is wrong for every one. Cross-FUNCTION, not merely cross-file.
/// - **Recognise the root structurally** (a variable initialised from `.Build()`/`WebApplication.*`,
///   the shape `super::super::ef_core`'s DbContext gate uses). It covers 427 of aspnetcore's 983 `app.`
///   sites and **0 of eShop's 3** — all three are `this IEndpointRouteBuilder app` extension-method
///   parameters, so adopting it would delete routes this adapter already reports.
pub const DEFAULT_ROOT_ROUTE_BUILDER_VARIABLE_NAMES: &[&str] = &["app"];

/// The declared C# route vocabulary one parse reads — the C# member of the family
/// `zzop_parser_python_3::PythonGuardVocab` and `zzop_parser_rust::RustGuardVocab` belong to: borrowed
/// slices owned by `zzop_engine::vocabulary::ResolvedVocabulary` and held for the length of a parse.
///
/// EMPTY IS A REAL DECLARATION, not a fallback trigger: this adapter never substitutes
/// [`DEFAULT_ROOT_ROUTE_BUILDER_VARIABLE_NAMES`] for an empty slice. With nothing declared, no bare
/// receiver is the root and every root-level registration is skipped (never keyed at a guessed path) —
/// the `vocabulary` roof's "absent or empty means the judgment is NOT MADE" rule, which reaches this
/// adapter through `ResolvedVocabulary` exactly as it reaches every other declared name.
pub struct CSharpRouteVocab<'a> {
    /// The bare receiver identifiers this project gives its PREFIX-FREE root route builder — the
    /// `WebApplication`/`IEndpointRouteBuilder` value a `MapGet` is registered on directly, carrying no
    /// path of its own. A name here contributes the EMPTY prefix, so `app.MapGet("/x")` keys `/x`.
    ///
    /// It is deliberately NOT the place for a group variable: `var api = app.MapGroup("/api")` is
    /// resolved structurally by [`collect_group_variables`], and naming `api` here would key its routes
    /// at `/x` instead of `/api/x` — a confidently WRONG key, which this adapter refuses everywhere else.
    pub root_route_builder_variable_names: &'a [&'a str],
}

impl CSharpRouteVocab<'static> {
    /// zzop's own suggested value, the single accessor `zzop_engine::VocabularyConfig::built_in` reads —
    /// so the default above has no second copy anywhere in the workspace.
    pub fn built_in() -> Self {
        CSharpRouteVocab {
            root_route_builder_variable_names: DEFAULT_ROOT_ROUTE_BUILDER_VARIABLE_NAMES,
        }
    }
}

const MAP_GROUP: &str = "MapGroup";

/// How many receiver links [`unwind_chain`] will walk before giving up. A fluent registration chains a
/// handful of builder calls; a bound this far above that costs nothing and makes the walk total.
const MAX_CHAIN_LINKS: usize = 64;

/// How many times [`collect_group_variables`] re-resolves the declarator set. Each round can resolve a
/// group declared in terms of an earlier one (`var v1 = api.MapGroup("/v1")`), and declaration order in
/// the file is not guaranteed to be dependency order, so the map is iterated to a fixed point under
/// this bound rather than filled in one pass.
const MAX_RESOLVE_ROUNDS: usize = 4;

/// A file's route-group variables. `Some(prefix)` is resolved; `None` marks a name declared more than
/// once with CONFLICTING prefixes — recorded rather than dropped, so a later round cannot resolve it.
pub(crate) type GroupMap = BTreeMap<String, Option<String>>;

/// Every `MapGroup`-derived group variable this file declares — see module doc for what is refused.
pub(crate) fn collect_group_variables(
    root: Node,
    src: &str,
    vocab: &CSharpRouteVocab<'_>,
) -> GroupMap {
    let mut declarators = Vec::new();
    collect_declarators(root, src, &mut declarators);
    let mut map = GroupMap::new();
    for _ in 0..MAX_RESOLVE_ROUNDS {
        let mut changed = false;
        for (name, value) in &declarators {
            // Only a chain that actually NAMES a group makes a group variable. `var x = app;` aliases
            // the root without carrying a prefix and is deliberately not followed.
            let Some(prefix) = group_bearing_prefix(*value, src, &map, vocab) else {
                continue;
            };
            match map.get(name) {
                None => {
                    map.insert(name.clone(), Some(prefix));
                    changed = true;
                }
                Some(Some(existing)) if *existing != prefix => {
                    map.insert(name.clone(), None); // conflicting declarations — never guess.
                    changed = true;
                }
                _ => {}
            }
        }
        if !changed {
            break;
        }
    }
    map
}

/// The composed prefix a `MapX` RECEIVER carries, or `None` when this build cannot know it (module doc)
/// — in which case the caller must skip the registration rather than key it prefix-less.
///
/// The returned prefix always starts with `/` and never ends with one; it is concatenated with the
/// route's own literal path and handed to `http_interface_key`, whose slash collapsing makes the exact
/// spelling of the seam irrelevant.
pub(crate) fn receiver_prefix(
    receiver: Node,
    src: &str,
    groups: &GroupMap,
    vocab: &CSharpRouteVocab<'_>,
) -> Option<String> {
    let (literals, head) = unwind_chain(receiver, src)?;
    let head_prefix = head_prefix(head, src, groups, vocab);
    if literals.is_empty() {
        // No `MapGroup` anywhere in the chain: only a recognized head (the `app` root, or a group
        // variable used bare) is evidence of a known prefix.
        return head_prefix;
    }
    // A chain WITH a literal group does not require a recognized head — the shipped v1 contract.
    let mut prefix = head_prefix.unwrap_or_default();
    for literal in literals {
        prefix.push('/');
        prefix.push_str(literal.trim_matches('/'));
    }
    Some(prefix)
}

/// The prefix of a chain that names at least one `MapGroup`; `None` for any other expression. This is
/// what makes a DECLARATOR a group variable, and it is deliberately stricter than [`receiver_prefix`]:
/// `var app2 = app;` is not a group.
fn group_bearing_prefix(
    value: Node,
    src: &str,
    groups: &GroupMap,
    vocab: &CSharpRouteVocab<'_>,
) -> Option<String> {
    let (literals, _) = unwind_chain(value, src)?;
    if literals.is_empty() {
        return None;
    }
    receiver_prefix(value, src, groups, vocab)
}

/// A bare head expression's own prefix: a DECLARED root name contributes nothing, a resolved group
/// variable contributes its prefix, anything else is unknown.
fn head_prefix(
    head: Node,
    src: &str,
    groups: &GroupMap,
    vocab: &CSharpRouteVocab<'_>,
) -> Option<String> {
    if head.kind() != "identifier" {
        return None;
    }
    let name = node_text(head, src);
    if vocab.root_route_builder_variable_names.contains(&name) {
        return Some(String::new());
    }
    groups.get(name).cloned().flatten()
}

/// Walk a receiver chain from the OUTERMOST call inwards, collecting every `MapGroup` literal and
/// returning them in SOURCE order together with the chain's head expression.
///
/// `None` when a `MapGroup` in the chain has a non-literal argument — that poisons the whole chain
/// rather than being skipped over, because the segment it would have contributed is a real part of the
/// path and continuing without it would produce a confidently WRONG key.
///
/// Non-`MapGroup` links (`.HasApiVersion(1, 0)`, `.WithTags("Items")`, `.RequireAuthorization()`) are
/// walked THROUGH: `MapGroup` is ASP.NET Core's only route-builder method that contributes a path
/// segment, so a builder call between the group and the registration changes nothing about the prefix.
fn unwind_chain<'a>(expr: Node<'a>, src: &str) -> Option<(Vec<String>, Node<'a>)> {
    let mut literals = Vec::new();
    let mut node = expr;
    for _ in 0..MAX_CHAIN_LINKS {
        if node.kind() != "invocation_expression" {
            break;
        }
        let Some(func) = node.child_by_field_name("function") else {
            break;
        };
        if func.kind() != "member_access_expression" {
            break;
        }
        let Some(name_node) = func.child_by_field_name("name") else {
            break;
        };
        if node_text(name_node, src) == MAP_GROUP {
            literals.push(first_string_argument(node, src)?); // non-literal group — poison the chain.
        }
        let Some(inner) = func.child_by_field_name("expression") else {
            break;
        };
        node = inner;
    }
    literals.reverse(); // collected outermost-first; a path composes innermost-first.
    Some((literals, node))
}

/// The first `argument`'s plain string literal, when that is what it is; `None` otherwise.
fn first_string_argument(call: Node, src: &str) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    let first = valid_named_children(args)
        .into_iter()
        .find(|a| a.kind() == "argument")?;
    let expr = valid_named_children(first).into_iter().next()?;
    string_literal_text(expr, src)
}

/// Every `variable_declarator` in the file, as (name, initializer). A declarator with no initializer —
/// or whose only extra child is a `bracketed_argument_list` (an array-rank declarator) — is skipped.
fn collect_declarators<'a>(node: Node<'a>, src: &str, out: &mut Vec<(String, Node<'a>)>) {
    if node.kind() == "variable_declarator" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let value = valid_named_children(node)
                .into_iter()
                .find(|c| c.id() != name_node.id() && c.kind() != "bracketed_argument_list");
            if let Some(value) = value {
                out.push((node_text(name_node, src).to_string(), value));
            }
        }
    }
    for child in valid_named_children(node) {
        collect_declarators(child, src, out);
    }
}
