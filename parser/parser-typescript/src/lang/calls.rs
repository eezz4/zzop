//! parse_calls — attributes intra-file `CallExpression`s to their enclosing top-level symbol, plus
//! class heritage (`extends`/`implements`) edges.
//!
//! Same-file only: no `importedNames`-style parameter for cross-file disambiguation, which belongs
//! to a whole-tree orchestrator. Cross-file resolution (RawCall -> SymbolEdge via ImportMap) is
//! `zzop_core::callgraph::resolve_calls_for_file`'s job, not this module's.

use std::collections::HashMap;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    BindingIdent, CallExpr, Callee, ClassDecl, Expr, MemberProp, Module, TsEntityName, TsType,
    TsTypeAnn, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use zzop_core::callgraph::RawCall;
use zzop_core::SourceSymbol;

/// Parses `source` and returns same-file call attributions plus class heritage edges.
pub fn parse_calls(file: &str, source: &str) -> Vec<RawCall> {
    let Some((cm, module)) = crate::parse_with_cm(file, source) else {
        return Vec::new();
    };
    let symbols = crate::parse_symbols(file, source);
    let bodies: Vec<&SourceSymbol> = symbols
        .iter()
        .filter(|s| s.body_start.is_some() && s.body_end.is_some())
        .collect();

    let class_var_types = collect_class_var_types(&module);

    let mut collector = CallCollector {
        cm: &cm,
        bodies: &bodies,
        class_var_types: &class_var_types,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    let mut out = collector.out;

    collect_heritage(&cm, file, &module, &mut out);
    out
}

/// Innermost enclosing top-level symbol whose body span covers `line`: the smallest
/// `bodyEnd - bodyStart` range wins when spans nest.
fn find_enclosing<'a>(line: u32, bodies: &[&'a SourceSymbol]) -> Option<&'a SourceSymbol> {
    let mut best: Option<&SourceSymbol> = None;
    let mut best_range = u32::MAX;
    for s in bodies {
        let (Some(start), Some(end)) = (s.body_start, s.body_end) else {
            continue;
        };
        if line < start || line > end {
            continue;
        }
        let range = end - start;
        if range < best_range {
            best = Some(s);
            best_range = range;
        }
    }
    best
}

struct CallCollector<'a> {
    cm: &'a SourceMap,
    bodies: &'a [&'a SourceSymbol],
    class_var_types: &'a HashMap<String, String>,
    out: Vec<RawCall>,
}

impl CallCollector<'_> {
    fn push_call(&mut self, node: &CallExpr, callee_name: &str, receiver_type: Option<String>) {
        let line = crate::line_of(self.cm, node.span.lo);
        if let Some(enclosing) = find_enclosing(line, self.bodies) {
            self.out.push(RawCall {
                from_symbol: enclosing.id.clone(),
                callee_name: callee_name.to_string(),
                line,
                receiver_type,
                is_heritage: false,
            });
        }
    }

    /// `obj.method()`: a typed class receiver (`new X()` / `: X` annotation) resolves as a cross-file
    /// candidate; otherwise it's collected only when `method` matches a local top-level symbol name.
    fn handle_method_call(&mut self, node: &CallExpr, obj: &Expr, prop: &MemberProp) {
        let MemberProp::Ident(name_ident) = prop else {
            return;
        };
        let method_name = name_ident.sym.to_string();
        let recv = match obj {
            Expr::Ident(id) => Some(id.sym.to_string()),
            _ => None,
        };
        let receiver_type = recv
            .as_ref()
            .and_then(|r| self.class_var_types.get(r).cloned());
        if let Some(rt) = receiver_type {
            self.push_call(node, &method_name, Some(rt));
            return;
        }
        if self.bodies.iter().any(|s| s.name == method_name) {
            self.push_call(node, &method_name, None);
        }
    }
}

impl Visit for CallCollector<'_> {
    fn visit_call_expr(&mut self, node: &CallExpr) {
        if let Callee::Expr(expr) = &node.callee {
            match &**expr {
                Expr::Ident(id) => self.push_call(node, id.sym.as_str(), None),
                Expr::Member(m) => self.handle_method_call(node, &m.obj, &m.prop),
                _ => {}
            }
        }
        node.visit_children_with(self);
    }
}

/// File-wide `varName -> ClassName` map, from `new X()` initializers and `: X` type annotations on
/// any binding identifier (covers both variable and parameter bindings via swc's `Pat::Ident`).
fn collect_class_var_types(module: &Module) -> HashMap<String, String> {
    let mut collector = ClassVarTypeCollector {
        map: HashMap::new(),
    };
    module.visit_with(&mut collector);
    collector.map
}

struct ClassVarTypeCollector {
    map: HashMap<String, String>,
}

impl Visit for ClassVarTypeCollector {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let Some(Expr::New(new_expr)) = d.init.as_deref() {
            if let Expr::Ident(id) = &*new_expr.callee {
                if let swc_core::ecma::ast::Pat::Ident(bi) = &d.name {
                    self.map.insert(bi.id.sym.to_string(), id.sym.to_string());
                }
            }
        }
        d.visit_children_with(self);
    }

    fn visit_binding_ident(&mut self, n: &BindingIdent) {
        if let Some(cls) = type_ref_name(n.type_ann.as_deref()) {
            self.map.insert(n.id.sym.to_string(), cls);
        }
    }
}

fn type_ref_name(ann: Option<&TsTypeAnn>) -> Option<String> {
    let ann = ann?;
    if let TsType::TsTypeRef(tr) = &*ann.type_ann {
        if let TsEntityName::Ident(id) = &tr.type_name {
            return Some(id.sym.to_string());
        }
    }
    None
}

/// Emits `RawCall(isHeritage)` for class `extends`/`implements`; cross-file resolution is `resolveCalls`'s job.
fn collect_heritage(cm: &SourceMap, file: &str, module: &Module, out: &mut Vec<RawCall>) {
    let mut collector = HeritageCollector { cm, file, out };
    module.visit_with(&mut collector);
}

struct HeritageCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: &'a mut Vec<RawCall>,
}

impl Visit for HeritageCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        let from_symbol = format!("{}#{}", self.file, n.ident.sym);
        let line = crate::line_of(self.cm, n.class.span.lo);
        if let Some(super_class) = &n.class.super_class {
            if let Expr::Ident(id) = &**super_class {
                self.out.push(RawCall {
                    from_symbol: from_symbol.clone(),
                    callee_name: id.sym.to_string(),
                    line,
                    receiver_type: None,
                    is_heritage: true,
                });
            }
        }
        for impl_clause in &n.class.implements {
            if let Expr::Ident(id) = &*impl_clause.expr {
                self.out.push(RawCall {
                    from_symbol: from_symbol.clone(),
                    callee_name: id.sym.to_string(),
                    line,
                    receiver_type: None,
                    is_heritage: true,
                });
            }
        }
        n.visit_children_with(self);
    }
}

#[cfg(test)]
mod tests;
