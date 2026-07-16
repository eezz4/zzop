//! `gin` (`github.com/gin-gonic/gin`) router PROVIDES, projected as framework-neutral router-mount
//! fragments — combined with `adapters::net_http`'s own fragments by `extract_go_router_fragments`.
//!
//! ## Scope (v1)
//! Import-gated on `github.com/gin-gonic/gin` (exact specifier). Recognition is a FULL CST walk (module
//! doc parity with `adapters::net_http` — every call site reachable, not top-level statements only).
//!
//! - **Engine receivers**: `r := gin.Default()` / `r := gin.New()` (or a plain re-assignment to an
//!   already-declared name) binds `r` as a tracked router receiver.
//! - **Groups**: `api := r.Group("/api")` where `r` is ALREADY a tracked receiver -> records BOTH a
//!   `Mount{prefix: "/api", ident: "api", ...}` entry on `r`'s own fragment AND registers `api` itself
//!   as a NEW tracked receiver — its own later `.GET`/`.POST`/... calls contribute to a fragment named
//!   `"api"`, which the engine's compose pass joins back through the `Mount` entry (crate root doc /
//!   `zzop_core::fragments`' own module doc explain the cross-file/cross-fragment composition). A
//!   NON-literal prefix skips the WHOLE `Group` call — no receiver registered, no `Mount` entry, never
//!   guessed. Chained/nested groups (`v1 := api.Group("/v1")`) work for free: `run`'s single top-down
//!   pass registers `api` before it can reach the statement that reads `api` again, the same
//!   "declared before used" argument `adapters::net_http::Collector::run`'s own doc makes.
//! - **Verbs**: `<receiver>.GET("/users", h)` / `.POST` / `.PUT` / `.DELETE` / `.PATCH` on any tracked
//!   receiver (an engine OR a group) -> `Verb{method: <UPPERCASE, verbatim — gin's own method names are
//!   already uppercase>, path, handler, line, attr_keys: vec![]}`. `handler` is `Some(name)` only for a
//!   bare-identifier second argument. A non-literal path skips the WHOLE call.
//! - Only a direct single-name `:=`/`=` binding is tracked for BOTH engine and group receivers (module
//!   doc parity with `adapters::net_http`'s identical narrowing) — a router received as a FUNCTION
//!   PARAMETER (`func setup(r *gin.Engine) { r.GET(...) }`, a common real idiom) is out of v1 scope:
//!   there is no local `:=`/`=` binding site to anchor the receiver's name to, and guessing from a
//!   parameter's declared TYPE would require resolving `*gin.Engine`/`*gin.RouterGroup` across this
//!   crate's `qualified_type`/`pointer_type` vocabulary with no `zzop_parser_rust`-style precedent to
//!   mirror — documented gap, not attempted.
//! - One `RouterMountFragment` per tracked receiver with at least one surviving entry, in
//!   first-appearance order.

use std::collections::HashSet;

use tree_sitter::Node;
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment, HTTP_KEY_VERBS};

use crate::util::{node_text, string_literal_text, valid_named_children};

use super::{append_entries, bare_identifier, nth_arg, single_rhs_call, single_target_name};

/// gin's own verb-method names — pinned equal to `zzop_core::HTTP_KEY_VERBS` (test below), the same
/// "happens to agree one-for-one" precedent `zzop_parser_python_3::adapters::fastapi::VERB_DECORATORS`
/// and `zzop_parser_rust::adapters::axum::VERB_METHODS` both document, except gin's own method names
/// are ALREADY uppercase (`.GET`, not `.get`) so no case conversion is needed at the call site.
pub const GIN_VERB_METHODS: &[&str] = HTTP_KEY_VERBS;

/// Extract this file's `gin` router-mount fragments — see module doc. Empty when the file does not
/// import `github.com/gin-gonic/gin` (never panics).
pub(crate) fn extract(
    tree: &tree_sitter::Tree,
    imports: &ImportMap,
    src: &str,
) -> Vec<RouterMountFragment> {
    let gin_names = local_names(imports);
    if gin_names.is_empty() {
        return Vec::new();
    }
    let mut collector = Collector {
        gin_names: &gin_names,
        known: HashSet::new(),
        order: Vec::new(),
        entries: std::collections::HashMap::new(),
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

fn local_names(imports: &ImportMap) -> HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| b.specifier == "github.com/gin-gonic/gin")
        .map(|(local, _)| local.clone())
        .collect()
}

struct Collector<'a> {
    gin_names: &'a HashSet<String>,
    known: HashSet<String>,
    order: Vec<String>,
    entries: std::collections::HashMap<String, Vec<RouterMountEntry>>,
}

impl<'a> Collector<'a> {
    fn run(&mut self, node: Node, src: &str) {
        if node.is_error() || node.is_missing() {
            return;
        }
        match node.kind() {
            "short_var_declaration" | "assignment_statement" => self.try_binding(
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
                src,
            ),
            "call_expression" => self.try_verb_call(node, src),
            _ => {}
        }
        for child in valid_named_children(node) {
            self.run(child, src);
        }
    }

    fn try_binding(&mut self, left: Option<Node>, right: Option<Node>, src: &str) {
        let Some(name) = single_target_name(left, src) else {
            return;
        };
        let Some(call) = single_rhs_call(right) else {
            return;
        };
        let Some((recv, method)) = selector_call(call, src) else {
            return;
        };
        if self.gin_names.contains(recv) && matches!(method, "Default" | "New") {
            self.register(&name);
        } else if self.known.contains(recv) && method == "Group" {
            let Some(prefix_node) = nth_arg(call, 0) else {
                return;
            };
            let Some(prefix) = string_literal_text(prefix_node, src) else {
                return; // non-literal prefix — skip the whole Group call, module doc.
            };
            self.register(&name);
            // `specifier` is unconditionally None: a gin Group's ident is the FRESHLY-BOUND local
            // group variable (`api := r.Group("/api")`), never an imported symbol — the opposite of
            // axum's mounted-router ident, where an import-map lookup is legitimate. Looking the
            // local name up in the import map here would, on a name collision (`db := r.Group("/db")`
            // in a file that also imports a `db` package), attach that package's import path as the
            // specifier and send compose down its resolve-by-specifier branch, which cannot resolve a
            // Go import path — silently dropping the group's routes from provides (opus review F1).
            let mount = RouterMountEntry::Mount {
                prefix,
                ident: name.clone(),
                specifier: None,
                attr_keys: Vec::new(),
            };
            append_entries(
                &mut self.order,
                &mut self.entries,
                recv.to_string(),
                vec![mount],
            );
        }
    }

    /// Marks `name` as a tracked receiver (eligible for a later `.GET`/`.../.Group` call to attach
    /// entries to it) WITHOUT touching `order`/`entries` — those two are populated lazily by
    /// `append_entries` only once a receiver actually gets a surviving entry, so a receiver that ends
    /// up with zero verb/mount calls contributes no empty fragment (module doc's final rule).
    fn register(&mut self, name: &str) {
        self.known.insert(name.to_string());
    }

    fn try_verb_call(&mut self, call: Node, src: &str) {
        let Some((recv, method)) = selector_call(call, src) else {
            return;
        };
        if !self.known.contains(recv) || !GIN_VERB_METHODS.contains(&method) {
            return;
        }
        let Some(path_node) = nth_arg(call, 0) else {
            return;
        };
        let Some(path) = string_literal_text(path_node, src) else {
            return;
        };
        let handler = nth_arg(call, 1).and_then(|n| bare_identifier(n, src));
        let entry = RouterMountEntry::Verb {
            method: method.to_string(),
            path,
            handler,
            line: crate::util::line_of(call),
            attr_keys: Vec::new(),
        };
        append_entries(
            &mut self.order,
            &mut self.entries,
            recv.to_string(),
            vec![entry],
        );
    }
}

/// `<receiver>.<Method>(...)` -> `(receiver name, method name)`, `None` for any other call shape.
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
    Some((node_text(operand, src), node_text(field, src)))
}

#[cfg(test)]
mod tests;
