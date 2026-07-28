// Branch-body AST scanning helpers for pathname-dispatch route attribution.
// Split out of `routes.rs` 2026-07-26 (300-line source cap); these are generic statement/expression
// walkers with no route-emission logic, which is exactly the seam the cap forced.

use swc_core::ecma::ast::{CallExpr, Callee, Expr, OptCall, Stmt};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::HTTP_KEY_VERBS;

use super::super::classify::{classify_conjunct, split_and, verb_literal, Conjunct};
use super::super::ctx::{is_method_receiver, FnCtx};
use super::super::push_unique;

// Per-branch symbol attribution (module doc `symbol`)

/// The single statement of a branch body, or `None` for an empty or multi-statement body.
pub(super) fn single_stmt(stmts: &[Stmt]) -> Option<&Stmt> {
    match stmts {
        [only] => Some(only),
        _ => None,
    }
}

/// The handler a dispatch branch delegates to, when the branch body is a SINGLE statement making
/// EXACTLY ONE call, to a bare-identifier callee (`return createGroup(request, env);`, its
/// one-statement-block/`await`/paren wrappings, or a bare expression statement). `None` for
/// anything else — a multi-statement block, an inline arrow, `new Response(...)`, a member callee,
/// a wrapper call — and the caller then keeps the enclosing function's name. Never a guess: the
/// whole point is that N sibling routes of one dispatcher must not collapse onto one symbol
/// (module doc).
pub(super) fn branch_target_symbol(stmt: &Stmt) -> Option<String> {
    match stmt {
        Stmt::Block(b) => single_stmt(&b.stmts).and_then(branch_target_symbol),
        Stmt::Return(r) => call_callee_name(r.arg.as_deref()?),
        Stmt::Expr(e) => call_callee_name(&e.expr),
        _ => None,
    }
}

/// The callee name of a bare-identifier call whose ARGUMENTS invoke nothing themselves, unwrapping
/// parens and `await`. Two rejections, both never-guess:
///
/// - A member callee (`handlers.create(...)`, `env.DB.prepare(...)`): choosing one property off a
///   member chain as "the handler" would be a guess, and a member call at a dispatch branch is at
///   least as often a response/utility helper as a route handler.
/// - An argument subtree containing any call (`return ok(handleThing(req));`,
///   `return json(await createGroup(req, env));`): there the OUTER call is a response wrapper and
///   the real handler is the INNER one, so naming the wrapper would aim every consumer's call-graph
///   BFS at the wrong subtree — actively wrong, not merely coarse. The test is lexical, so a mere
///   argument COERCION (`getRevision(req, env, m[1], Number.parseInt(m[2], 10))`) blocks attribution
///   as well even though its outer callee is the real handler; see the module doc's "Case-2
///   residual" for why that trade is taken rather than guessed around. `mutating-route-no-auth` BFSing
///   from `ok` would never see the auth guard inside `createGroup` and would ACCUSE a guarded write
///   route (a false positive on a security rule, and a detection INCREASE from a parser change);
///   `unsafe-read-endpoint`/`non-idempotent-write` would under-reach the same way. The enclosing
///   symbol at worst over-reaches exactly as it always did, so falling back to it is strictly
///   safer: never-guess beats a confident wrong answer.
fn call_callee_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Paren(p) => call_callee_name(&p.expr),
        Expr::Await(a) => call_callee_name(&a.arg),
        Expr::Call(call) => {
            let Callee::Expr(callee) = &call.callee else {
                return None;
            };
            let Expr::Ident(id) = &**callee else {
                return None; // member / computed / call-returning-call callee — not nameable
            };
            if call.args.iter().any(|a| contains_call(&a.expr)) {
                return None; // wrapper call — see this function's doc
            }
            Some(id.sym.to_string())
        }
        _ => None,
    }
}

/// Any call anywhere in `expr`'s subtree, including inside a nested arrow/function argument —
/// deliberately conservative: a branch body that invokes more than once is not a plain delegation,
/// so it is not attributable at all.
fn contains_call(expr: &Expr) -> bool {
    let mut finder = CallFinder { found: false };
    expr.visit_with(&mut finder);
    finder.found
}

struct CallFinder {
    found: bool,
}

impl Visit for CallFinder {
    fn visit_call_expr(&mut self, n: &CallExpr) {
        self.found = true;
        n.visit_children_with(self);
    }

    /// `f?.(x)` is a call too — a different AST node, the same "this argument invokes something".
    fn visit_opt_call(&mut self, n: &OptCall) {
        self.found = true;
        n.visit_children_with(self);
    }
}

// Fallback verb-mention scan (module doc: "recursively scanning the if's consequent block")

pub(super) fn scan_verb_mentions(stmt: &Stmt, ctx: &FnCtx) -> Vec<String> {
    let mut out = Vec::new();
    scan_stmt_for_verbs(stmt, ctx, &mut out);
    out
}

pub(super) fn scan_block_for_verbs(stmts: &[Stmt], ctx: &FnCtx, out: &mut Vec<String>) {
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
