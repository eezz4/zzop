// Association: IfStmt / SwitchStmt -> IoProvide

use swc_core::common::{BytePos, SourceMap};
use swc_core::ecma::ast::{
    ArrowExpr, ClassMethod, FnDecl, FnExpr, GetterProp, IfStmt, MethodProp, SetterProp, SwitchStmt,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_interface_key, IoProvide};

use super::classify::{classify_conjunct, path_literal, split_and, Conjunct};
use super::ctx::{is_pathname_receiver, FnCtx};
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
    let mut final_verbs = if !verbs.is_empty() {
        verbs
    } else {
        let scanned = scan_verb_mentions(&n.cons, ctx);
        if !scanned.is_empty() {
            scanned
        } else {
            fallback_verbs()
        }
    };
    // Sorted, not source-appearance order — see `sort_verbs`.
    sort_verbs(&mut final_verbs);
    // Per-branch symbol (module doc `symbol`): every path in THIS `if` shares one consequent, so
    // one lookup covers them all.
    let symbol = branch_target_symbol(&n.cons).or_else(|| ctx.symbol.clone());
    for (path, pos) in &paths {
        let line = crate::line_of(cm, *pos);
        emit_routes(rel, path, line, symbol.clone(), &final_verbs, out);
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
        let mut verbs = if verbs.is_empty() {
            fallback_verbs()
        } else {
            verbs
        };
        // Sorted, not source-appearance order — see `sort_verbs`.
        sort_verbs(&mut verbs);
        // The GROUPED (fallthrough) body is this branch's body: every case path in the group is
        // genuinely served by it, so they legitimately share whatever symbol it yields.
        let symbol = single_stmt(&sw.cases[end].cons)
            .and_then(branch_target_symbol)
            .or_else(|| ctx.symbol.clone());
        for case in &sw.cases[i..=end] {
            if let Some(test) = &case.test {
                if let Some(path) = path_literal(test) {
                    if path.starts_with('/') {
                        let line = crate::line_of(cm, case.span.lo);
                        emit_routes(rel, &path, line, symbol.clone(), &verbs, out);
                    }
                }
            }
            // A `default:` case (no test) contributes no path.
        }
        i = end + 1;
    }
}

/// Alphabetical verb order for one branch's provide burst, matching the sibling adapters' existing
/// convention (`next_pages_api::collector::collect_verbs`'s own `verbs.sort()`, whose doc likewise
/// promises "sorted, deduped UPPERCASE verbs"). Applied at BOTH decision sites — the `if`-conjunct/
/// consequent scan and the `switch`-case-body scan each build their own list.
///
/// Not merely cosmetic, and not a join fix either. The JOIN side is genuinely neutral: one branch's
/// N verbs produce N provides with N DISTINCT `http_interface_key`s, and every consumer joins by
/// key. What the order does reach is the ORDER OF THE REPORT. `assemble` sorts the tree-wide provide
/// array only at the very end (`helpers::sort_io_provides`), AFTER `rules::run`, so every whole-tree
/// rule sees provides in adapter-emission order (`assemble/rules/io_scan.rs`'s own doc states this as
/// its determinism contract). A rule that emits one finding per provide — `mutating-route-no-auth`'s
/// `for p in mutating` loop, and any `IoScan` pack rule — then produces N findings that tie on ALL
/// FOUR of `merge_findings`' sort keys (severity, file, line, rule_id), because one branch's N verbs
/// share one file and one line. `merge_findings` uses a STABLE sort, so the tie resolves to emission
/// order: `DELETE` before `GET` or the other way round, decided by which `if` the author wrote first.
/// Sorting here makes that tie resolve the same way for every dispatcher in every tree.
fn sort_verbs(verbs: &mut [String]) {
    verbs.sort();
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
            response: None,
            body: None,
            kind: "http".to_string(),
            key: http_interface_key(verb, path),
            file: rel.to_string(),
            line,
            symbol: symbol.clone(),
        });
    }
}

mod scan;

use scan::{branch_target_symbol, scan_block_for_verbs, scan_verb_mentions, single_stmt};
