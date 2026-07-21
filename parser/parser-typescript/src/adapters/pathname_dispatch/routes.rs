// Association: IfStmt / SwitchStmt -> IoProvide

use swc_core::common::{BytePos, SourceMap};
use swc_core::ecma::ast::{
    ArrowExpr, ClassMethod, FnDecl, FnExpr, GetterProp, IfStmt, MethodProp, SetterProp, Stmt,
    SwitchStmt,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, IoProvide, HTTP_KEY_VERBS};

use super::classify::{classify_conjunct, path_literal, split_and, verb_literal, Conjunct};
use super::ctx::{is_method_receiver, is_pathname_receiver, FnCtx};
use super::{fallback_verbs, push_unique};

/// Walks a whole function body (module doc "Association algorithm"), evaluating every `IfStmt`
/// and pathname-keyed `SwitchStmt` reachable without crossing a nested function boundary.
pub(super) struct RouteCollector<'a> {
    pub(super) ctx: &'a FnCtx,
    pub(super) cm: &'a SourceMap,
    pub(super) rel: &'a str,
    pub(super) out: &'a mut Vec<IoProvide>,
}

impl Visit for RouteCollector<'_> {
    fn visit_fn_decl(&mut self, _: &FnDecl) {}
    fn visit_fn_expr(&mut self, _: &FnExpr) {}
    fn visit_arrow_expr(&mut self, _: &ArrowExpr) {}
    fn visit_class_method(&mut self, _: &ClassMethod) {}
    fn visit_method_prop(&mut self, _: &MethodProp) {}
    fn visit_getter_prop(&mut self, _: &GetterProp) {}
    fn visit_setter_prop(&mut self, _: &SetterProp) {}

    fn visit_if_stmt(&mut self, n: &IfStmt) {
        process_if(n, self.ctx, self.cm, self.rel, self.out);
        n.visit_children_with(self);
    }

    fn visit_switch_stmt(&mut self, n: &SwitchStmt) {
        process_switch(n, self.ctx, self.cm, self.rel, self.out);
        n.visit_children_with(self);
    }
}

fn process_if(n: &IfStmt, ctx: &FnCtx, cm: &SourceMap, rel: &str, out: &mut Vec<IoProvide>) {
    let conjuncts = split_and(&n.test);
    let mut paths: Vec<(String, BytePos)> = Vec::new();
    let mut verbs: Vec<String> = Vec::new();
    for c in &conjuncts {
        match classify_conjunct(c, ctx) {
            Conjunct::Paths(p) => paths.extend(p),
            Conjunct::Verbs(vs) => {
                for v in vs {
                    push_unique(&mut verbs, v);
                }
            }
            Conjunct::Other => {}
        }
    }
    if paths.is_empty() {
        return;
    }
    let final_verbs = if !verbs.is_empty() {
        verbs
    } else {
        let scanned = scan_verb_mentions(&n.cons, ctx);
        if !scanned.is_empty() {
            scanned
        } else {
            fallback_verbs()
        }
    };
    for (path, pos) in &paths {
        let line = crate::line_of(cm, *pos);
        emit_routes(rel, path, line, ctx.symbol.clone(), &final_verbs, out);
    }
}

fn process_switch(
    sw: &SwitchStmt,
    ctx: &FnCtx,
    cm: &SourceMap,
    rel: &str,
    out: &mut Vec<IoProvide>,
) {
    if !is_pathname_receiver(&sw.discriminant, ctx) {
        return;
    }
    let mut i = 0;
    while i < sw.cases.len() {
        // Group consecutive empty-body cases with the next non-empty body (fallthrough).
        let mut end = i;
        while sw.cases[end].cons.is_empty() && end + 1 < sw.cases.len() {
            end += 1;
        }
        let mut verbs = Vec::new();
        scan_block_for_verbs(&sw.cases[end].cons, ctx, &mut verbs);
        let verbs = if verbs.is_empty() {
            fallback_verbs()
        } else {
            verbs
        };
        for case in &sw.cases[i..=end] {
            if let Some(test) = &case.test {
                if let Some(path) = path_literal(test) {
                    if path.starts_with('/') {
                        let line = crate::line_of(cm, case.span.lo);
                        emit_routes(rel, &path, line, ctx.symbol.clone(), &verbs, out);
                    }
                }
            }
            // A `default:` case (no test) contributes no path.
        }
        i = end + 1;
    }
}

fn emit_routes(
    rel: &str,
    path: &str,
    line: u32,
    symbol: Option<String>,
    verbs: &[String],
    out: &mut Vec<IoProvide>,
) {
    for verb in verbs {
        out.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key: http_interface_key(verb, path),
            file: rel.to_string(),
            line,
            symbol: symbol.clone(),
        });
    }
}

// Fallback verb-mention scan (module doc: "recursively scanning the if's consequent block")

fn scan_verb_mentions(stmt: &Stmt, ctx: &FnCtx) -> Vec<String> {
    let mut out = Vec::new();
    scan_stmt_for_verbs(stmt, ctx, &mut out);
    out
}

fn scan_block_for_verbs(stmts: &[Stmt], ctx: &FnCtx, out: &mut Vec<String>) {
    for s in stmts {
        scan_stmt_for_verbs(s, ctx, out);
    }
}

fn scan_stmt_for_verbs(stmt: &Stmt, ctx: &FnCtx, out: &mut Vec<String>) {
    match stmt {
        Stmt::Block(b) => scan_block_for_verbs(&b.stmts, ctx, out),
        Stmt::If(i) => {
            let conjuncts = split_and(&i.test);
            let classified: Vec<Conjunct> = conjuncts
                .iter()
                .map(|c| classify_conjunct(c, ctx))
                .collect();
            if classified.iter().any(|c| matches!(c, Conjunct::Paths(_))) {
                // A separate route lives here (module doc: skip the whole subtree so its verbs
                // never leak into this scan).
                return;
            }
            for c in classified {
                if let Conjunct::Verbs(vs) = c {
                    for v in vs {
                        push_unique(out, v);
                    }
                }
            }
            scan_stmt_for_verbs(&i.cons, ctx, out);
            if let Some(alt) = &i.alt {
                scan_stmt_for_verbs(alt, ctx, out);
            }
        }
        Stmt::Switch(sw) if is_method_receiver(&sw.discriminant, ctx) => {
            for case in &sw.cases {
                let Some(test) = &case.test else { continue };
                let Some(v) = verb_literal(test) else {
                    continue;
                };
                if HTTP_KEY_VERBS.contains(&v.as_str()) {
                    push_unique(out, v);
                }
            }
        }
        _ => {}
    }
}
