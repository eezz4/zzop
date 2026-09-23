//! The Go ROUTER-WRAPPER lane: routes registered on a project's OWN router type, recognized by the
//! shape of the call rather than by an import this adapter has been taught.
//!
//! ## The gap this closes, measured
//!
//! `adapters::net_http` and `adapters::gin` are both import-gated, and between them they read the two
//! routers a Go project does not usually expose directly. The dominant real-world Go layout wraps the
//! router in a project type, and both adapters are structurally blind to it:
//!
//! - **grafana** (`corpus/frameworks/grafana`) registers on its own `pkg/api/routing.RouteRegister`.
//!   381 registrations in non-test `pkg/`; `zzop coverage` reported **61** http provides for the whole
//!   tree, every one of them from a datasource plugin's `net/http` resource handler or a test, and
//!   **none** from the route table.
//! - **prometheus** registers on its own `util/route.Router`: 83 registrations in non-test `web/`,
//!   **7** http provides tree-wide.
//!
//! Nothing disclosed it either. The zero-extraction census
//! (`zzop_engine::recognizers::zero_extraction`) skips any extension that extracted more than zero, so
//! 61 and 7 kept `.go` off the list — while the SAME reply raised the census for grafana's `.ts` and
//! `.tsx`, which have no routes to miss. The list a reader treats as "where zzop found nothing" named
//! the languages where nothing was there and omitted the one where nearly everything was.
//!
//! ## Why the shape is the gate, and what says that is safe
//!
//! There is no import to key on: the router type is the project's own. So recognition keys on
//! `<identifier>.<Title-case verb>("<rooted literal>", …)`, and the question is whether that shape
//! means anything else in Go. Measured over every `.go` file in `corpus/` — 664 matches:
//!
//! ```text
//! grep -rhoE '\b[A-Za-z_][A-Za-z0-9_]*\.(Get|Post|Put|Delete|Patch|Head|Options|Any)\("/[^"]*"' \
//!   --include='*.go' corpus/
//! ```
//!
//! grafana 468 · prometheus 76 · gin 7 (gin's own `.Any`, all real routes) · **terraform 0 · cobra 0**.
//! Every receiver in the distribution is a router (`r`, `mux`, `router`, `apiRoute`, `routeRegister`,
//! …); the ambiguous-looking ones were read by hand (`m`, `entities`, `server`) and all three are
//! routes. Zero false positives. The two controls matter most: cobra is a CLI library and terraform has
//! no route table, and both return nothing.
//!
//! The rooted-literal requirement is what carries that. `cache.Get("key")`, `db.Get(id)` and
//! `headers.Get("Accept")` all fail it, and it is the same test `adapters::net_http` already applies to
//! tell a Go 1.22 pattern's host prefix from a path.
//!
//! ## Ownership boundary with the two import-gated adapters
//!
//! - `Handle`/`HandleFunc` are NOT in this vocabulary — that is `net/http`'s lane.
//! - gin's verbs are UPPERCASE (`.GET`), this lane's are Title-case (`.Get`), so they cannot collide —
//!   except on `.Any`, which both spell the same way. So this adapter declines a file that imports gin
//!   outright. gin's adapter is strictly better there (it traces receivers to `gin.New()`), and one
//!   route emitted twice is a worse failure than one file served by the better of two readers. A file
//!   that imports gin and ALSO registers on a non-gin wrapper is a documented v1 narrowing.
//!
//! ## Prefixes: composed lexically, or the route is not emitted
//!
//! The callback group (`X.Group("/api", func(Y routing.RouteRegister) { … })`) is the form grafana
//! uses 57 times. The prefix is resolved by DESCENT — `Y` is bound to `prefix(X) + "/api"` for the
//! callback body only, and restored after — rather than by emitting a `Mount` entry naming `Y`.
//!
//! 🔴 A `Mount` keyed on the parameter NAME would have been wrong, and the corpus says so: grafana's
//! `pkg/api/api.go` binds `dashboardRoute` at BOTH `/dashboards` (line 464) and `/dashboard/snapshots`
//! (line 490), and `orgRoute` three times. Name-keyed fragments would have served every route of each
//! group under every one of its namesake's prefixes. Lexical descent scopes them correctly because the
//! binding lives exactly as long as the callback body does.
//!
//! **A receiver whose mount point this adapter cannot see emits NOTHING.** That is any identifier that
//! is a parameter of an enclosing function and was not bound by a group callback here — the
//! `func (api *TeamAPI) registerRoutes(r routing.RouteRegister)` idiom, whose prefix is decided by a
//! caller in another file. Emitting those at their file-local paths would key them at a path the server
//! does not answer, and `zzop_core::fragments::RouterMountEntry::MountRef`'s own doc owns the rule this
//! follows: *a route emitted at the wrong key is worse than a route not emitted, because only the
//! second one looks like the absence it is.*
//!
//! Also out of v1 scope, for the same reason and stated rather than approximated: a value-returning
//! prefix builder (prometheus's `router = router.WithPrefix(o.RoutePrefix)`), whose prefix is a runtime
//! configuration value that no static read can supply.
//!
//! ## `Any` is a sentinel here, and the repo is SPLIT on that — read this before changing it
//!
//! `.Any("/x", h)` registers one handler for every method. This lane emits ONE
//! [`zzop_core::UNKNOWN_VERB`] entry for it. Its two siblings disagree with each other on exactly that
//! concept, and the disagreement is older than this file:
//!
//! - `adapters::net_http` gives a verbless Go 1.22 pattern ONE `UNKNOWN_VERB` entry, its doc calling
//!   the case "serves every method, statically unknown";
//! - `adapters::gin` expands `.Any` to one entry per `HTTP_KEY_VERBS`.
//!
//! Both ship, in this crate, today. The expansion side is a RECORDED decision and must be cited rather
//! than quietly overridden: v0.20.0 (`79eb0595`) item 5, *"Multi-verb route expansion. axum `any()`,
//! express/hono `.all`, gin `.Any`, and fastapi `@api_route(methods=[…])` each expand to one route per
//! verb"* — landed in the same commit as item 1, which abolished verb FABRICATION for a verb that is
//! merely unknown. The author's line between them is "known to be every verb" (expand) versus "not
//! statically known" (sentinel).
//!
//! **Why this lane sits on the sentinel side of that line.** Item 5 names FRAMEWORKS whose catch-all
//! semantics are documented by their own vendor. This lane has no framework: it reads a router type it
//! has never been shown, so `Any` here is a method name that LOOKS like a catch-all, not one whose
//! meaning has been verified. Under item 1's own test that is "not statically known", and the
//! conservative answer is the sentinel — which is also not a silent drop, because the engine partitions
//! a `"? <path>"` key into the path-level served-set and discloses it through
//! `cross-layer/unknown-verb-route`.
//!
//! 🔵 That grafana's `RouteRegister.Any` does document itself as *"any HTTP verb"* is a fact about ONE
//! project, learned by reading its source — not something this lane can establish for the next one.
//!
//! ✅ **The split is now decided, and this lane is on the named side of it** (2026-09-13, ledger
//! V229): `cross-layer-resolution.md`'s catch-all row draws the line at WHO GUARANTEES the semantics —
//! a vendor-documented catch-all API expands (gin `.Any`, express/hono `.all`, axum `any()`, fastapi
//! `@api_route(methods=[…])`), and everything without that guarantee emits the sentinel. A project's
//! own router type carries no such guarantee, so this lane emits the sentinel; `adapters::net_http`'s
//! verbless pattern is on the same side for the same reason. All three docs now cite each other, which
//! is what stops the two answers drifting apart again unnoticed.

use std::collections::HashMap;

use tree_sitter::Node;
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment};

use super::{append_entries, bare_identifier, nth_arg};
use crate::util::{string_literal_text, valid_named_children};

mod scope;

use scope::{join, param_names, single_param_closure, FUNCTION_KINDS};

/// The Title-case verb methods this lane reads. Deliberately NOT `zzop_core::HTTP_KEY_VERBS` (those are
/// uppercase wire spellings) and deliberately without `Handle`/`HandleFunc` (`net/http`'s lane).
/// `Any` is absent here and handled on its own — it is not a verb, it is the absence of one.
const WRAPPER_VERB_METHODS: &[&str] = &["Get", "Post", "Put", "Delete", "Patch", "Head", "Options"];

/// The catch-all registration this lane maps to [`zzop_core::UNKNOWN_VERB`] — see module doc.
const ANY_METHOD: &str = "Any";

/// The sub-router registration whose prefix this lane composes by descent — see module doc.
const GROUP_METHOD: &str = "Group";

/// gin's import specifier, whose files this adapter declines outright — see module doc's ownership
/// boundary. Read from `adapters::gin` rather than spelled again so the two cannot drift apart.
fn file_imports_gin(imports: &ImportMap) -> bool {
    imports
        .iter()
        .any(|(_, b)| b.specifier == super::gin::GIN_SPECIFIER)
}

/// Extract this file's router-wrapper fragments — see module doc. Empty for a file that imports gin,
/// and for a file with no recognized registration (never panics).
pub(crate) fn extract(
    tree: &tree_sitter::Tree,
    imports: &ImportMap,
    src: &str,
) -> Vec<RouterMountFragment> {
    if file_imports_gin(imports) {
        return Vec::new();
    }
    let mut collector = Collector {
        bound: HashMap::new(),
        param_scopes: Vec::new(),
        order: Vec::new(),
        entries: HashMap::new(),
    };
    collector.run(tree.root_node(), src);
    collector
        .order
        .into_iter()
        .filter_map(|name| {
            let es = collector.entries.remove(&name)?;
            (!es.is_empty()).then_some(RouterMountFragment { name, entries: es })
        })
        .collect()
}

struct Collector {
    /// Group-callback parameter -> `(absolute prefix, fragment name)`. Populated only for the callback
    /// body being walked and restored on the way out, which is what keeps two same-named callbacks in
    /// one file from sharing a prefix.
    bound: HashMap<String, (String, String)>,
    /// Parameter names of each enclosing function, innermost last. An identifier found here and not in
    /// `bound` has a mount point this file cannot see, and its routes are refused.
    param_scopes: Vec<std::collections::HashSet<String>>,
    order: Vec<String>,
    entries: HashMap<String, Vec<RouterMountEntry>>,
}

impl Collector {
    fn run(&mut self, node: Node, src: &str) {
        if node.is_error() || node.is_missing() {
            return;
        }
        if node.kind() == "call_expression" && self.try_call(node, src) {
            return; // handled, including its own recursion
        }
        let pushed = FUNCTION_KINDS.contains(&node.kind());
        if pushed {
            self.param_scopes.push(param_names(node, src));
        }
        for child in valid_named_children(node) {
            self.run(child, src);
        }
        if pushed {
            self.param_scopes.pop();
        }
    }

    /// `true` when this call was a group whose body this method already walked — the caller must not
    /// walk it again. A verb call returns `false`: it has no router-bearing children, but its arguments
    /// may still contain closures that register routes of their own.
    fn try_call(&mut self, call: Node, src: &str) -> bool {
        let Some((recv, method)) = selector_call(call, src) else {
            return false;
        };
        if method == GROUP_METHOD {
            return self.try_group(call, recv, src);
        }
        self.try_verb(call, recv, method, src);
        false
    }

    /// The absolute prefix and fragment name a receiver contributes to, or `None` when this file cannot
    /// see where the receiver is mounted — see the module doc's never-guess paragraph.
    fn resolve(&self, recv: &str) -> Option<(String, String)> {
        if let Some(bound) = self.bound.get(recv) {
            return Some(bound.clone());
        }
        if self.param_scopes.iter().any(|s| s.contains(recv)) {
            return None;
        }
        Some((String::new(), recv.to_string()))
    }

    fn try_group(&mut self, call: Node, recv: &str, src: &str) -> bool {
        let Some((prefix, param, body)) = self.group_binding(call, recv, src) else {
            return false;
        };
        let previous = self.bound.insert(param.clone(), prefix);
        for child in valid_named_children(body) {
            self.run(child, src);
        }
        match previous {
            Some(p) => self.bound.insert(param, p),
            None => self.bound.remove(&param),
        };
        true
    }

    /// `(the binding for the callback parameter, its name, the closure node)` for a recognized callback
    /// group. `None` for every shape this lane does not read — a non-literal or unrooted prefix, a
    /// callback that is not a single-parameter closure, or a receiver whose own mount is unknown — and
    /// in each of those cases the caller walks the call normally, so the parameter lands in an ordinary
    /// parameter scope and the routes inside are refused rather than guessed.
    fn group_binding<'t>(
        &self,
        call: Node<'t>,
        recv: &str,
        src: &str,
    ) -> Option<((String, String), String, Node<'t>)> {
        let prefix_lit = string_literal_text(nth_arg(call, 0)?, src)?;
        if !prefix_lit.starts_with('/') {
            return None;
        }
        let (closure, param) = single_param_closure(nth_arg(call, 1)?, src)?;
        let (outer_prefix, fragment) = self.resolve(recv)?;
        let body = closure.child_by_field_name("body")?;
        Some(((join(&outer_prefix, &prefix_lit), fragment), param, body))
    }

    fn try_verb(&mut self, call: Node, recv: &str, method: &str, src: &str) {
        let wire_method = if method == ANY_METHOD {
            zzop_core::UNKNOWN_VERB
        } else if let Some(verb) = WRAPPER_VERB_METHODS.iter().find(|v| **v == method) {
            *verb
        } else {
            return;
        };
        let Some(path_node) = nth_arg(call, 0) else {
            return;
        };
        let Some(path) = string_literal_text(path_node, src) else {
            return;
        };
        // The rooted-literal gate, and the whole reason this lane can run without an import to key on.
        if !path.starts_with('/') {
            return;
        }
        let Some((prefix, fragment)) = self.resolve(recv) else {
            return;
        };
        let entry = RouterMountEntry::Verb {
            method: wire_method.to_uppercase(),
            path: join(&prefix, &path),
            handler: nth_arg(call, 1).and_then(|n| bare_identifier(n, src)),
            line: crate::util::line_of(call),
            attr_keys: Vec::new(),
        };
        append_entries(&mut self.order, &mut self.entries, fragment, vec![entry]);
    }
}

/// `<receiver>.<Method>(...)` -> `(receiver name, method name)`, `None` for any other call shape.
/// Byte-identical in intent to `adapters::gin::shapes::selector_call`; kept here rather than shared
/// because that one is `pub(super)` inside gin's own module tree and this lane must not depend on
/// gin's internals to decide a shape that is not gin's.
fn selector_call<'s>(call: Node, src: &'s str) -> Option<(&'s str, &'s str)> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "selector_expression" {
        return None;
    }
    let operand = func.child_by_field_name("operand")?;
    let field = func.child_by_field_name("field")?;
    if operand.kind() != "identifier" {
        return None;
    }
    Some((
        crate::util::node_text(operand, src),
        crate::util::node_text(field, src),
    ))
}

#[cfg(test)]
mod tests;
