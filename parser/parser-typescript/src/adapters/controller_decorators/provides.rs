//! The three public extraction entry points and their AST-visitor collectors — see the parent
//! module's doc for scope, gating rules, and known limits.

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{ClassDecl, ClassMember, ClassMethod, Decorator};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, ControllerPrefixRouteFragment, IoProvide};

use super::context::{controller_context, ControllerCtx};
use super::method_facts::{decorator_name, method_route, method_route_facts};

/// Extracts NestJS `@Controller`/`@Get`/`@Post`/... HTTP route `IoProvide`s from one TS file's raw
/// source — see module doc for the decorator shapes and gating rules. Returns an empty `Vec` (never
/// panics) on an unparseable file, same convention as every other swc-AST adapter in this crate.
pub fn extract_controller_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut c = ControllerCollector {
        cm: cm_ref,
        file: rel,
        out: Vec::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct ControllerCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Vec<IoProvide>,
}

impl Visit for ControllerCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if let Some(ControllerCtx::Literal { prefix }) = controller_context(&n.class.decorators) {
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    self.emit_method(&prefix, m);
                }
            }
        }
        // `ControllerCtx::DeferredRef` (a `RouteKey.Asset`-shaped prefix) emits no direct provides here
        // — see `extract_controller_prefix_route_fragments`.
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

impl ControllerCollector<'_> {
    fn emit_method(&mut self, prefix: &str, m: &ClassMethod) {
        let Some((verb, name, line, paths, body)) = method_route_facts(self.cm, m) else {
            return;
        };
        for path in paths {
            let full_path = format!("{prefix}/{path}");
            self.out.push(IoProvide {
                body: body.clone(),
                kind: "http".to_string(),
                key: http_interface_key(&verb, &full_path),
                file: self.file.to_string(),
                line,
                symbol: Some(name.clone()),
            });
        }
    }
}

/// Extracts controller-prefix route FRAGMENTS — the deferred-to-assemble counterpart of
/// `extract_controller_provides` for the `controller-prefix-ref-v1` exception (module doc): a
/// `@Controller(RouteKey.Asset)`-shaped (dotted member-expression) prefix cannot be resolved from this
/// one file alone, so each qualifying controller's methods are projected as
/// `zzop_core::ControllerPrefixRouteFragment`s instead of `IoProvide`s.
/// `zzop_engine::analyze::compose`'s controller-prefix composer resolves `prefix_ref` against the
/// project-wide merged const map (`egress::const_map_fragment`, which also folds string-valued `enum`
/// members) and emits the real `IoProvide`s at assemble time. A `@Controller('literal')` class
/// contributes nothing here (already fully resolved by `extract_controller_provides`); any other
/// non-literal prefix shape (call, template, computed member, deeper chain, `{path: ref}` object)
/// contributes nothing here either — same skip-whole-controller convention as
/// `extract_controller_provides`. Returns an empty `Vec` (never panics) on an unparseable file.
pub fn extract_controller_prefix_route_fragments(
    rel: &str,
    text: &str,
) -> Vec<ControllerPrefixRouteFragment> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut c = ControllerPrefixFragmentCollector {
        cm: cm_ref,
        out: Vec::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct ControllerPrefixFragmentCollector<'a> {
    cm: &'a SourceMap,
    out: Vec<ControllerPrefixRouteFragment>,
}

impl Visit for ControllerPrefixFragmentCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if let Some(ControllerCtx::DeferredRef { prefix_ref }) =
            controller_context(&n.class.decorators)
        {
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    self.emit_fragment(&prefix_ref, m);
                }
            }
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

impl ControllerPrefixFragmentCollector<'_> {
    fn emit_fragment(&mut self, prefix_ref: &str, m: &ClassMethod) {
        let Some((verb, name, line, paths, body)) = method_route_facts(self.cm, m) else {
            return;
        };
        for path in paths {
            self.out.push(ControllerPrefixRouteFragment {
                body: body.clone(),
                prefix_ref: prefix_ref.to_string(),
                verb: verb.clone(),
                path,
                line,
                symbol: Some(name.clone()),
            });
        }
    }
}

/// Detects NestJS `@UseGuards(...)` decorator coverage — see module doc "NestJS `@UseGuards` decorator
/// exemption". Returns the set of route-registration lines (matching `IoProvide::line` for whatever
/// `extract_controller_provides` would emit from the same file) covered by an explicit `@UseGuards(...)`
/// chain, either class-level or method-level. Empty set (never panics) on an unparseable file.
pub fn extract_controller_guarded_lines(rel: &str, text: &str) -> HashSet<u32> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return HashSet::new();
    };
    let mut c = GuardedLineCollector {
        cm: &cm,
        out: HashSet::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct GuardedLineCollector<'a> {
    cm: &'a SourceMap,
    out: HashSet<u32>,
}

impl Visit for GuardedLineCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        // Mirrors `emit_method`'s own class/method gating exactly, so a guarded line only appears
        // here if `extract_controller_provides` would also emit a real `IoProvide` for it.
        if let Some(_ctx) = controller_context(&n.class.decorators) {
            let class_guarded = has_use_guards(&n.class.decorators);
            for member in &n.class.body {
                if let ClassMember::Method(m) = member {
                    if class_guarded || has_use_guards(&m.function.decorators) {
                        if let Some((_, decorator, _)) = method_route(&m.function.decorators) {
                            self.out.insert(crate::line_of(self.cm, decorator.span.lo));
                        }
                    }
                }
            }
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

fn has_use_guards(decorators: &[Decorator]) -> bool {
    decorators
        .iter()
        .any(|d| decorator_name(&d.expr).as_deref() == Some("UseGuards"))
}
