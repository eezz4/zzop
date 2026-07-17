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
//! - **Function-parameter receivers** (`func <Name>(<param> *gin.RouterGroup)` / `*gin.Engine` — the
//!   dominant real-world cross-file registration idiom: a top-level route-registration function that
//!   receives its router by PARAMETER rather than by a local `:=`/`=` binding): `<param>`'s declared type
//!   must be a `pointer_type` over a `qualified_type` whose package identifier resolves (this file's
//!   `ImportMap`) to the gin specifier and whose own type name is `RouterGroup` or `Engine`
//!   ([`GIN_RECEIVER_TYPES`]). When it is, `<param>` is registered as a tracked receiver whose FRAGMENT
//!   is named after the ENCLOSING FUNCTION (`<Name>`, deliberately NOT the parameter's own local name) —
//!   exactly the name a cross-file `Mount.ident` (next bullet) references, and the same name-first
//!   lookup `zzop_engine::analyze::compose::router_mounts`' `find_child` already performs at compose
//!   time. `.GET`/.../`.Group` calls on that parameter inside the function body accumulate into THAT
//!   fragment per the rules above. The registration is SCOPED to this one function: a same-named
//!   parameter in a different function (`func A(r *gin.RouterGroup)` / `func B(r *gin.RouterGroup)`)
//!   registers and restores independently, never bleeding into a sibling function or into unrelated code
//!   after either returns. Receiver METHODS (`func (s *Server) Register(r *gin.RouterGroup)`) are out of
//!   v1 scope: `method_declaration` is a distinct grammar node this recognizer never matches against —
//!   documented gap, not attempted.
//! - **Cross-file mount calls** (the call side of the parameter idiom above): a call `pkg.Fn(...)` or
//!   bare `Fn(...)` of ANY arity where EXACTLY ONE argument is a mountable receiver — a bare
//!   tracked-receiver identifier (an engine or a group) or `<tracked>.Group("<literal>")` -> appends
//!   `Mount{prefix: <the Group literal, or "" for a bare-receiver argument>, ident: "Fn", specifier:
//!   <the `ImportMap` specifier of `pkg` for a package-qualified callee, `None` for a bare same-file
//!   callee>, attr_keys: vec![]}` to THAT argument's OWN fragment (the group/engine being passed in —
//!   NOT a fragment named after `Fn`), mirroring how `zzop_parser_rust::adapters::axum`'s `nest`/`merge`
//!   mount entries live on the MOUNTING receiver while naming the mounted side. Every OTHER argument (a
//!   `*sql.DB` handle, a config struct, a literal, ...) is ignored outright — it carries no mount
//!   information and never blocks recognition of the one receiver argument that does
//!   (`pkg.Register(db, api.Group("/admin"))` mounts on `api` exactly as `pkg.Register(api.Group("/admin"))`
//!   would — the dominant real-world multi-parameter registration idiom, e.g. `func Register(db *DB, r
//!   *gin.RouterGroup)`). TWO OR MORE mountable-receiver arguments in the same call (`pkg.Wire(a.Group("/a"),
//!   b.Group("/b"))`) is genuinely ambiguous — which one does `Wire` actually mount onto? — so the WHOLE
//!   call is rejected, never guessed; this crate has no parser-layer disclosure/self-report channel to
//!   flag the ambiguity (disclosure for router-mount gaps lives downstream in
//!   `crates/engine`/`rules-cross-layer`, e.g. route-near-miss/prefix-drift), so
//!   [`Collector::try_call_site`]'s own code comment is the only record. This is the OPPOSITE case from
//!   a fresh local `Group` binding's specifier (always `None` — see the F1 comment in `try_binding`'s
//!   `Group` arm below): a package qualifier genuinely IS an imported symbol, so the `ImportMap` lookup
//!   is the correct resolution here, not the F1 hazard. The callee's operand (for a selector callee) is
//!   checked against the `ImportMap` to tell a package qualifier (`users.UsersRegister(v1)`, a candidate)
//!   apart from a method call on a local variable (`mux.HandleFunc(...)`, already handled by
//!   [`Collector::try_verb_call`], never a candidate here) — an operand that does not resolve as an
//!   import is skipped outright. A non-literal `Group` prefix in a candidate argument skips the whole
//!   call, the same never-guess rule as every other prefix in this file. This is a deliberately loose
//!   over-approximation on the CALLEE side: any call taking exactly one tracked receiver (bare or via
//!   `.Group(...)`), of any arity, is a candidate, not just a fixed allowlist of registration-function
//!   names — unlike gin's own `GET`/`POST`/... vocabulary, a user's own registration functions have no
//!   fixed name to allowlist. Documented rather than narrowed further.
//! - Only a direct single-name `:=`/`=` binding is tracked for a LOCAL engine/group receiver (module doc
//!   parity with `adapters::net_http`'s identical narrowing); a function-PARAMETER receiver is tracked
//!   too (two bullets up), closing the dominant real-world gap this file's v1 used to document outright.
//!   Still out of v1 scope, documented rather than attempted: a `*gin.Engine` handed to a `net/http`-
//!   shaped helper (`http.ListenAndServe(addr, engine)` — `engine` implements `http.Handler`) is not
//!   recognized as a mount site here (that is `net/http` vocabulary, not a gin verb/mount call, and this
//!   file only ever looks at ITS OWN gin-shaped call vocabulary); and a registration call whose argument
//!   list resolves to zero or two-or-more mountable receivers is never a candidate either (zero: nothing
//!   to mount; two-or-more: ambiguous, previous bullet) — [`Collector::try_call_site`]'s own arity-agnostic
//!   gate skips both, never guessed at.
//! - One `RouterMountFragment` per tracked receiver with at least one surviving entry, in
//!   first-appearance order.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment, HTTP_KEY_VERBS};

use crate::util::{node_text, string_literal_text, valid_named_children};

use super::{append_entries, bare_identifier, nth_arg, single_rhs_call, single_target_name};

/// gin's own verb-method names — pinned equal to `zzop_core::HTTP_KEY_VERBS` (test below), the same
/// "happens to agree one-for-one" precedent `zzop_parser_python_3::adapters::fastapi::VERB_DECORATORS`
/// and `zzop_parser_rust::adapters::axum::VERB_METHODS` both document, except gin's own method names
/// are ALREADY uppercase (`.GET`, not `.get`) so no case conversion is needed at the call site.
pub const GIN_VERB_METHODS: &[&str] = HTTP_KEY_VERBS;

// The cross-file half (call-site mounts, function-parameter receivers, `GIN_RECEIVER_TYPES`) lives
// in `cross_file` — split for the 300-line cap, same module-doc contract.
mod cross_file;

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
        imports,
        known: HashMap::new(),
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
    imports: &'a ImportMap,
    /// Tracked-receiver name -> the FRAGMENT name it contributes to. Identity (`name -> name`) for a
    /// local engine/group binding; `name -> <enclosing function name>` for a function-parameter receiver
    /// (module doc) — the indirection this map exists for.
    known: HashMap<String, String>,
    order: Vec<String>,
    entries: std::collections::HashMap<String, Vec<RouterMountEntry>>,
}

impl<'a> Collector<'a> {
    fn run(&mut self, node: Node, src: &str) {
        if node.is_error() || node.is_missing() {
            return;
        }
        match node.kind() {
            "short_var_declaration" | "assignment_statement" => {
                self.try_binding(
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                    src,
                );
            }
            "call_expression" => {
                self.try_verb_call(node, src);
                self.try_call_site(node, src);
            }
            "function_declaration" => {
                // Scoped registration: register this function's gin-receiver parameter(s), walk its
                // body (and everything else under it) with those registrations active, then restore —
                // module doc's "never bleeds into a sibling function" guarantee.
                let restores = self.register_receiver_params(node, src);
                for child in valid_named_children(node) {
                    self.run(child, src);
                }
                for (name, prior) in restores {
                    match prior {
                        Some(fragment) => {
                            self.known.insert(name, fragment);
                        }
                        None => {
                            self.known.remove(&name);
                        }
                    }
                }
                return;
            }
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
        } else if let Some(recv_fragment) = self.known.get(recv).cloned() {
            if method != "Group" {
                return;
            }
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
                recv_fragment,
                vec![mount],
            );
        }
    }

    /// Marks `name` as a tracked receiver whose fragment is named after ITSELF (identity mapping) —
    /// the local engine/group binding case. See `known`'s own doc for the function-parameter case,
    /// registered separately by `register_receiver_params`. Deliberately does NOT touch
    /// `order`/`entries` — those two are populated lazily by `append_entries` only once a receiver
    /// actually gets a surviving entry, so a receiver that ends up with zero verb/mount calls
    /// contributes no empty fragment (module doc's final rule).
    fn register(&mut self, name: &str) {
        self.known.insert(name.to_string(), name.to_string());
    }

    fn try_verb_call(&mut self, call: Node, src: &str) {
        let Some((recv, method)) = selector_call(call, src) else {
            return;
        };
        let Some(fragment) = self.known.get(recv).cloned() else {
            return;
        };
        if !GIN_VERB_METHODS.contains(&method) {
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
        append_entries(&mut self.order, &mut self.entries, fragment, vec![entry]);
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
