//! `net/http` router PROVIDES (the raw standard-library `DefaultServeMux` and an explicit
//! `http.NewServeMux()`), projected as framework-neutral router-mount fragments — combined with
//! `adapters::gin`'s own fragments by `extract_go_router_fragments`. See `zzop_core::fragments`'
//! module doc for the fragment shape rationale.
//!
//! ## Scope (v1)
//! Import-gated on `net/http` (specifier exactly `"net/http"` — NOT a `net/http/...` subpackage like
//! `net/http/httptest`, none of which expose `HandleFunc`/`Handle`/`NewServeMux`). Recognition is a
//! FULL CST walk (every `call_expression` anywhere reachable — inside an `if`, a helper function, a
//! goroutine — not top-level statements only; the F3 defect class the task brief calls out), unlike
//! `lang::symbols`'s file-top-level-only scope.
//!
//! - **`DefaultServeMux` family**: `<net/http-name>.HandleFunc("<pattern>", h)` /
//!   `<net/http-name>.Handle("<pattern>", h)` -> one fixed fragment named `"http"` (there is no
//!   receiver variable to name it after — both register on the package-level default mux).
//! - **Explicit `ServeMux` family**: `mux := <net/http-name>.NewServeMux()` (or `mux = ...` re-
//!   assignment to an already-declared name) binds `mux` as a tracked receiver — mirrors
//!   `zzop_parser_rust::adapters::http_clients::BindingCollector`'s "first pass collects bindings,
//!   second pass collects call sites" discipline, adapted to a single combined recursive pass here
//!   (document order gives "declared before used" for free — see `run`'s doc). `mux.HandleFunc(...)` /
//!   `mux.Handle(...)` then contribute to a fragment named `"mux"` (the receiver's own binding name),
//!   NOT `"http"`. Only a direct single-name `:=`/`=` binding is tracked (a `var mux = ...`
//!   declaration, or a destructuring/multi-target assignment, is out of v1 scope — documented
//!   narrowing, the overwhelmingly common idiom is `mux := http.NewServeMux()`).
//! - **Pattern parsing (Go 1.22+ method-in-pattern syntax)**: a pattern literal may lead with an
//!   UPPERCASE `zzop_core::HTTP_KEY_VERBS` token followed by a single space (`"GET /users"`) — split
//!   into `(method, path)` and emit ONE `Verb` entry. A pattern with NO leading verb token serves
//!   every method; per the task brief's own direction, this mirrors the engine's
//!   `PAGES_API_FALLBACK_VERBS` / `zzop_parser_typescript::PATHNAME_DISPATCH_FALLBACK_VERBS`
//!   established "no method visible" convention (`["GET", "POST"]`, pinned here as
//!   [`GO_HANDLEFUNC_FALLBACK_VERBS`]) rather than guessing a single verb: TWO `Verb` entries are
//!   emitted, one per fallback method, both carrying the SAME path. A pattern (after stripping any
//!   leading verb token) that does not start with `/` — Go 1.22 patterns may ALSO lead with a host,
//!   e.g. `"example.com/path"` or `"GET example.com/path"` — is skipped entirely: disambiguating a
//!   host prefix from a rooted path without reimplementing `net/http`'s own pattern grammar would be
//!   guessing, so this crate never does it (v1 narrowing, documented rather than approximated).
//! - `handler` is `Some(name)` only for a bare-identifier second argument; a closure/other expression
//!   leaves it `None` but the entry is still emitted. A non-literal pattern (first argument) skips the
//!   WHOLE call — never guessed.
//! - One `RouterMountFragment` per name (`"http"`, or a tracked `ServeMux` binding) with at least one
//!   surviving entry, in first-appearance order.

use std::collections::HashSet;

use tree_sitter::Node;
use zzop_core::{ImportMap, RouterMountEntry, RouterMountFragment, HTTP_KEY_VERBS};

use crate::util::{node_text, string_literal_text, valid_named_children};

use super::{append_entries, bare_identifier, nth_arg, single_rhs_call, single_target_name};

/// Verbs emitted for a `net/http` pattern that names no leading method token — module doc; mirrors
/// the engine's `PAGES_API_FALLBACK_VERBS` / `zzop_parser_typescript::PATHNAME_DISPATCH_FALLBACK_VERBS`
/// value exactly (documented parity only: this crate has no dependency edge to either of those crates
/// to write an executable cross-crate equality pin against, unlike `crates/engine`'s own pin test
/// against its `parser-typescript` dependency).
pub const GO_HANDLEFUNC_FALLBACK_VERBS: [&str; 2] = ["GET", "POST"];

const VERB_METHODS: &[&str] = &["HandleFunc", "Handle"];

/// Extract this file's `net/http` router-mount fragments — see module doc. Empty when the file does
/// not import `net/http` (never panics).
pub(crate) fn extract(
    tree: &tree_sitter::Tree,
    imports: &ImportMap,
    src: &str,
) -> Vec<RouterMountFragment> {
    let net_http_names = local_names(imports);
    if net_http_names.is_empty() {
        return Vec::new();
    }
    let mut collector = Collector {
        net_http_names: &net_http_names,
        servemux_names: HashSet::new(),
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
        .filter(|(_, b)| b.specifier == "net/http")
        .map(|(local, _)| local.clone())
        .collect()
}

struct Collector<'a> {
    net_http_names: &'a HashSet<String>,
    servemux_names: HashSet<String>,
    order: Vec<String>,
    entries: std::collections::HashMap<String, Vec<RouterMountEntry>>,
}

impl<'a> Collector<'a> {
    /// A single combined recursive pass (module doc): a binding site registers a `ServeMux` name
    /// BEFORE its later call sites are reached, because pre-order-DFS-over-siblings-in-source-order is
    /// exactly Go's own "declared before used" requirement for a same-scope sequence of statements.
    fn run(&mut self, node: Node, src: &str) {
        if node.is_error() || node.is_missing() {
            return;
        }
        match node.kind() {
            "short_var_declaration" => self.try_binding(
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
                src,
            ),
            "assignment_statement" => self.try_binding(
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
        let Some(rhs_call) = single_rhs_call(right) else {
            return;
        };
        if is_new_servemux_call(rhs_call, self.net_http_names, src) {
            self.servemux_names.insert(name);
        }
    }

    fn try_verb_call(&mut self, call: Node, src: &str) {
        let Some(func) = call.child_by_field_name("function") else {
            return;
        };
        if func.kind() != "selector_expression" {
            return;
        }
        let Some(operand) = func.child_by_field_name("operand") else {
            return;
        };
        let Some(field) = func.child_by_field_name("field") else {
            return;
        };
        if operand.kind() != "identifier" {
            return;
        }
        let recv = node_text(operand, src);
        let verb_method = node_text(field, src);
        if !VERB_METHODS.contains(&verb_method) {
            return;
        }
        let fragment_name = if self.net_http_names.contains(recv) {
            "http".to_string()
        } else if self.servemux_names.contains(recv) {
            recv.to_string()
        } else {
            return;
        };

        let Some(pattern_node) = nth_arg(call, 0) else {
            return;
        };
        let Some(pattern) = string_literal_text(pattern_node, src) else {
            return;
        };
        let handler = nth_arg(call, 1).and_then(|n| bare_identifier(n, src));
        let entries = pattern_entries(&pattern, handler, crate::util::line_of(call));
        append_entries(&mut self.order, &mut self.entries, fragment_name, entries);
    }
}

/// See module doc's "Pattern parsing" section.
fn pattern_entries(pattern: &str, handler: Option<String>, line: u32) -> Vec<RouterMountEntry> {
    if let Some((verb, rest)) = split_leading_verb(pattern) {
        if !rest.starts_with('/') {
            return Vec::new();
        }
        return vec![verb_entry(verb, rest, handler, line)];
    }
    if !pattern.starts_with('/') {
        return Vec::new();
    }
    GO_HANDLEFUNC_FALLBACK_VERBS
        .iter()
        .map(|v| verb_entry(v, pattern, handler.clone(), line))
        .collect()
}

fn split_leading_verb(pattern: &str) -> Option<(&str, &str)> {
    HTTP_KEY_VERBS.iter().find_map(|verb| {
        pattern
            .strip_prefix(verb)
            .and_then(|rest| rest.strip_prefix(' '))
            .map(|path| (*verb, path))
    })
}

fn verb_entry(method: &str, path: &str, handler: Option<String>, line: u32) -> RouterMountEntry {
    RouterMountEntry::Verb {
        method: method.to_string(),
        path: path.to_string(),
        handler,
        line,
        attr_keys: Vec::new(),
    }
}

fn is_new_servemux_call(call: Node, net_http_names: &HashSet<String>, src: &str) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "selector_expression" {
        return false;
    }
    let Some(operand) = func.child_by_field_name("operand") else {
        return false;
    };
    let Some(field) = func.child_by_field_name("field") else {
        return false;
    };
    operand.kind() == "identifier"
        && net_http_names.contains(node_text(operand, src))
        && node_text(field, src) == "NewServeMux"
}

#[cfg(test)]
mod tests;
