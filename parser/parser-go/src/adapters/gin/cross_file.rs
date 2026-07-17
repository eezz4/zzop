//! The cross-file half of gin recognition (module doc's "Cross-file mount calls" and
//! "Function-parameter receivers" sections) — split from the parent purely for the 300-line cap;
//! the behavior contract lives on the parent module doc and these items' own docs.

use tree_sitter::Node;
use zzop_core::{ImportMap, RouterMountEntry};

use crate::util::{node_text, string_literal_text, valid_named_children};

use super::super::{append_entries, bare_identifier, nth_arg};
use super::{selector_call, Collector};

/// gin's two router-receiver PARAMETER types recognized by [`Collector::register_receiver_params`] (the
/// definition-side of the function-parameter idiom, module doc): a plain router group
/// (`*gin.RouterGroup`, the dominant idiom in the wild) and the top-level engine itself (`*gin.Engine`,
/// rarer as a parameter but symmetric with the call-side `gin.Default()`/`gin.New()` receivers already
/// tracked there). This is a disjoint, grammar-derived (gin's own exported type names) vocabulary, not a
/// duplicate of [`super::GIN_VERB_METHODS`] and not a policy threshold — called out here for the same
/// rule-quality.md §6 inventory visibility that array gets, never merged with it.
const GIN_RECEIVER_TYPES: &[&str] = &["RouterGroup", "Engine"];

impl<'a> Collector<'a> {
    /// Cross-file mount call recognition (module doc's "Cross-file mount calls" section): `pkg.Fn(...)`
    /// / bare `Fn(...)` of ANY arity where EXACTLY ONE argument is a bare tracked receiver or
    /// `<tracked>.Group("<lit>")` (checked per-argument by [`Self::call_site_receiver`]). Every other
    /// argument (a db handle, a config struct, a literal, ...) is ignored outright — it carries no
    /// mount information and never blocks recognition of the one that does. Zero such arguments is not
    /// a candidate (nothing to mount); two-or-more is rejected as ambiguous (see the `receivers.next()`
    /// check below).
    pub(super) fn try_call_site(&mut self, call: Node, src: &str) {
        let Some(args_node) = call.child_by_field_name("arguments") else {
            return;
        };
        let args = valid_named_children(args_node);
        if args.is_empty() {
            return;
        }
        let Some(func) = call.child_by_field_name("function") else {
            return;
        };
        let Some((callee_name, specifier)) = callee_ref(func, self.imports, src) else {
            return;
        };
        let mut receivers = args
            .iter()
            .filter_map(|&arg| self.call_site_receiver(arg, src));
        let Some(first) = receivers.next() else {
            return; // no mountable receiver argument at all — not a candidate.
        };
        if receivers.next().is_some() {
            // Two or more mountable receivers in the same call (e.g. `pkg.Wire(a.Group("/a"),
            // b.Group("/b"))`) — genuinely ambiguous which one this registration function actually
            // mounts onto; a wrong guess would misattribute someone else's routes, so the WHOLE call
            // is rejected, never guessed (module doc). This crate has no parser-layer disclosure/
            // self-report channel for a known blind spot like this one (disclosure for router-mount
            // gaps lives downstream in `crates/engine`/`rules-cross-layer`, e.g. route-near-miss/
            // prefix-drift — out of reach from a parser adapter) — this comment is the only record.
            return;
        }
        let (recv_fragment, prefix) = first;
        let mount = RouterMountEntry::Mount {
            prefix,
            ident: callee_name.to_string(),
            specifier,
            attr_keys: Vec::new(),
        };
        append_entries(
            &mut self.order,
            &mut self.entries,
            recv_fragment,
            vec![mount],
        );
    }

    /// The `(fragment name, prefix)` a call-site argument resolves to: a bare tracked-receiver
    /// identifier (`prefix: ""`), or `<tracked>.Group("<literal>")` (`prefix`: the literal). Any other
    /// argument shape, an untracked receiver, or a non-literal `Group` prefix -> `None` (skip the whole
    /// call, module doc's never-guess rule).
    fn call_site_receiver(&self, arg: Node, src: &str) -> Option<(String, String)> {
        if let Some(name) = bare_identifier(arg, src) {
            let fragment = self.known.get(&name)?.clone();
            return Some((fragment, String::new()));
        }
        if arg.kind() != "call_expression" {
            return None;
        }
        let (recv, method) = selector_call(arg, src)?;
        if method != "Group" {
            return None;
        }
        let fragment = self.known.get(recv)?.clone();
        let prefix_node = nth_arg(arg, 0)?;
        let prefix = string_literal_text(prefix_node, src)?; // non-literal — skip the whole call.
        Some((fragment, prefix))
    }

    /// Registers this `function_declaration`'s `*gin.RouterGroup`/`*gin.Engine` parameter(s)
    /// (module doc's function-parameter receivers) as tracked receivers whose fragment is the FUNCTION's
    /// own name, returning each registered param's PRIOR `known` mapping (`None` if it was not
    /// previously tracked at all) so the caller can restore file-global state once this function's body
    /// has been walked — scoped so one function's parameter name never bleeds into an unrelated sibling
    /// function that happens to reuse it.
    pub(super) fn register_receiver_params(
        &mut self,
        node: Node,
        src: &str,
    ) -> Vec<(String, Option<String>)> {
        let Some(name_node) = node.child_by_field_name("name") else {
            return Vec::new();
        };
        let fn_name = node_text(name_node, src).to_string();
        let Some(params) = node.child_by_field_name("parameters") else {
            return Vec::new();
        };
        let mut restores = Vec::new();
        for param in valid_named_children(params) {
            if param.kind() != "parameter_declaration" {
                continue;
            }
            let Some(ty) = param.child_by_field_name("type") else {
                continue;
            };
            if !is_gin_receiver_type(ty, self.imports, src) {
                continue;
            }
            let mut cursor = param.walk();
            for name_node in param.children_by_field_name("name", &mut cursor) {
                if name_node.is_error() || name_node.is_missing() {
                    continue;
                }
                let param_name = node_text(name_node, src).to_string();
                let prior = self.known.insert(param_name.clone(), fn_name.clone());
                restores.push((param_name, prior));
            }
        }
        restores
    }
}

/// A call-site callee's `(name, specifier)`: a bare `identifier` (`Fn(...)`) -> `(Fn, None)`, same-file
/// resolution by name; a `pkg.Fn(...)` selector whose operand resolves in `imports` -> `(Fn,
/// Some(pkg's specifier))`. Any other callee shape, or a selector whose operand does NOT resolve as an
/// import (a method call on a local variable, e.g. `mux.HandleFunc(...)`) -> `None`, not a candidate —
/// module doc's "Cross-file mount calls" section.
fn callee_ref<'s>(
    func: Node,
    imports: &ImportMap,
    src: &'s str,
) -> Option<(&'s str, Option<String>)> {
    match func.kind() {
        "identifier" => Some((node_text(func, src), None)),
        "selector_expression" => {
            let operand = func.child_by_field_name("operand")?;
            let field = func.child_by_field_name("field")?;
            if operand.kind() != "identifier" {
                return None;
            }
            let binding = imports.get(node_text(operand, src))?;
            Some((node_text(field, src), Some(binding.specifier.clone())))
        }
        _ => None,
    }
}

/// `*gin.RouterGroup` / `*gin.Engine` — a `pointer_type` over a `qualified_type` whose package
/// identifier resolves (this file's `ImportMap`) to the gin specifier, and whose own type name is one of
/// [`GIN_RECEIVER_TYPES`]. Any other shape (a value-typed `gin.Engine` with no `*`, a same-package bare
/// `type_identifier`, a generic/slice/map type, an unrelated qualified type like `*sql.DB`, ...) ->
/// `false`, never guessed.
fn is_gin_receiver_type(ty: Node, imports: &ImportMap, src: &str) -> bool {
    if ty.kind() != "pointer_type" {
        return false;
    }
    let Some(inner) = valid_named_children(ty).into_iter().next() else {
        return false;
    };
    if inner.kind() != "qualified_type" {
        return false;
    }
    let Some(package_node) = inner.child_by_field_name("package") else {
        return false;
    };
    let Some(type_name_node) = inner.child_by_field_name("name") else {
        return false;
    };
    let package = node_text(package_node, src);
    let type_name = node_text(type_name_node, src);
    imports
        .get(package)
        .is_some_and(|b| b.specifier == "github.com/gin-gonic/gin")
        && GIN_RECEIVER_TYPES.contains(&type_name)
}
