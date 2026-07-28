//! `parse_calls` — Python `RawCall` extractor, matching `zzop_parser_typescript::lang::calls::
//! parse_calls`'s and `zzop_parser_java_21::lang::calls::parse_calls`'s contract exactly
//! (`crates/core/src/callgraph.rs`'s `RawCall` doc): each call site is attributed to its enclosing
//! `def`/`class` body (the smallest body span covering the call's line — the same "innermost enclosing
//! body wins" rule both siblings use), so this crate's output rides the SAME whole-repo
//! `SymbolGraph`/BFS the engine's call-graph-BFS native rules already build
//! (`crates/engine/src/analyze/native_rules/callgraph/mod.rs`).
//!
//! ## What is NOT a call edge here: everything outside a `def` BODY
//! Three positions are structurally excluded by the walk in [`CallCollector::visit_stmt_scoped`], at
//! EVERY nesting depth:
//! - a `def`'s DECORATOR list and its PARAMETER defaults/annotations — `user: User =
//!   Depends(get_current_user)` is a FastAPI route's auth guard written in the signature, and letting it
//!   in as a call edge would make the BFS clear a route for a reason the BFS cannot justify. Guard
//!   evidence of that shape is `adapters::fastapi::guard`'s job (the framework-neutral
//!   `decorator_guarded` / `auth-guarded` channel).
//! - a CLASS BODY's non-`def` statements — which means EVERYTHING in a class body except its own direct
//!   `def`s: initializers (`authentication_classes = (JWTAuthentication(),)`, `serializer_class = X()`)
//!   and, because a `class` statement is itself one of those non-`def` statements, a NESTED `class` in
//!   full, methods included. Initializers are the case that MATTERS most: a Django URLconf provide's
//!   `symbol` is the VIEW CLASS, `zzop_core::callgraph::build_symbol_graph` mints no class->method edge,
//!   so the class node's ONLY outgoing edges would be its own initializers — the edge set most likely to
//!   carry an auth-shaped NAME and least likely to mean "the handler checks authorization". A class body
//!   initializer therefore mints no edge, and a class node reached by the BFS is honestly a leaf.
//!
//!   Dropping a nested class's METHODS is not a separate policy; it is the only honest option available,
//!   and it is what keeps the leaf claim above true. `lang::symbols::parse_symbols` emits `Class` and
//!   `Class.method` symbols for TOP-LEVEL classes only — it mints no symbol for `Outer.Inner` or
//!   `Outer.Inner.m` — so [`find_enclosing`] has no body span for a nested method, and walking into one
//!   would attribute its calls to the innermost span that DOES cover them: `Outer`, the class node. That
//!   is precisely the mis-attribution the initializer exclusion exists to prevent, and it would be worse,
//!   since a whole method body's worth of names would land on it. If `parse_symbols` ever emits
//!   nested-class symbols, this exclusion should be revisited together with it — not before.
//!
//! A relative-span rule could not express any of this: a body span starts at the first STATEMENT line, so
//! the exclusion held only for a TOP-LEVEL `def` (whose decorators and defaults fall outside every
//! tracked span) and silently lapsed for a method or a nested `def`. The walk answers it structurally
//! instead, so the claim is true wherever the `def` sits.
//!
//! ## Receiver typing (`RawCall::receiver_type`)
//! A qualified call `recv.method(...)` records `recv`'s ANNOTATED type when `recv` is a tracked
//! parameter/annotated-assignment name, else `recv`'s own written text verbatim. Tracking is
//! SCOPE-STACKED (module scope, then one scope per entered `def`, innermost wins) rather than one
//! file-wide flat map: Python reuses parameter names (`session`, `db`, `user`, `repo`) across a module's
//! functions as a matter of style, and a flat map made the LAST annotation in the file win for every
//! function — `def a(session: Session)` and `def b(session: SessionValidator)` in one file typed BOTH
//! bodies' `session.x()` as `SessionValidator`. That is a mis-resolution, not a miss: it points the edge
//! at a real but wrong class. The TS/Java siblings carry the flat-map simplification; this crate does not,
//! because it also has the verbatim fallback below, which turns a wrong type into a wrong EDGE rather
//! than a dropped one.
//!
//! The verbatim fallback is what makes an imported-class or imported-module call resolve at all
//! (`Article.objects` / `jwt.create_access_token_for_user`): the name is spelled at the call site, so
//! treating an untracked identifier AS its own receiver identity lets
//! `zzop_core::callgraph::resolve_calls_for_file` match it against that name's import binding. A
//! receiver that resolves to neither an import nor a local symbol is dropped downstream (never guessed).
//!
//! `self`/`cls` are rewritten to the ENCLOSING CLASS name when the call sits inside a `Class.method`
//! body — Python spells the implicit receiver, so `self.check_permissions()` in a Django view carries
//! exactly the same information a Java `this.checkPermissions()` does, and dropping it would make every
//! intra-class guard hop invisible. Outside a `Class.method` body (a plain function that happens to have
//! a `self` parameter) there is no class to name, so the CALL IS DROPPED — the verbatim fallback is not
//! used there, since `self` is not an identity any resolver could match.
//!
//! A qualified call whose receiver is anything OTHER than a bare `Name` (`a().b()`, `x[i].y()`,
//! `a.b.c()`) is out of scope and not emitted at all — though the walk still recurses into that receiver
//! expression, so any call NESTED inside it is still collected on its own.
//!
//! ## Out of scope (documented, not attempted)
//! Class-heritage (`class X(Base)`) edges — the TS frontend emits those as `is_heritage` `RawCall`s, but
//! this crate's first cut is scoped to actual call SITES (the auth-guard-reachability need it was built
//! for), matching `parser-java-21`'s identical fence.

use std::collections::HashMap;

use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use zzop_core::callgraph::RawCall;
use zzop_core::{SourceSymbol, SourceSymbolKind};

/// Extract this file's call attributions — module doc. Empty on parse failure (never panics).
pub fn parse_calls(rel: &str, text: &str) -> Vec<RawCall> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let idx = crate::LineIndex::new(text);
    let symbols = crate::lang::symbols::parse_symbols(rel, text);
    let bodies: Vec<&SourceSymbol> = symbols
        .iter()
        .filter(|s| s.body_start.is_some() && s.body_end.is_some())
        .collect();

    let mut collector = CallCollector {
        idx: &idx,
        bodies: &bodies,
        scopes: vec![HashMap::new()], // module scope
        out: Vec::new(),
    };
    for stmt in &module.body {
        collector.visit_stmt_scoped(stmt);
    }
    collector.out
}

/// Innermost enclosing body-bearing symbol whose span covers `line`: the smallest `body_end -
/// body_start` range wins when spans nest (a method's own body beats its class's) — same rule as
/// `zzop_parser_typescript::calls::find_enclosing`.
///
/// On an EQUAL range a non-`Class` symbol wins. Python's one-line method body (`def get_queryset(self):
/// return Article.objects.all()` — a real idiom) makes the class's body span and the method's body span
/// the identical single line, and `lang::symbols` emits the class first, so a "strictly smaller wins"
/// rule handed that method's calls to its CLASS. The tie-break is on `kind`, not on emission order, so it
/// is independent of how `bodies` happens to be ordered.
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
        let wins = match range.cmp(&best_range) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Equal => {
                s.kind != SourceSymbolKind::Class
                    && best.is_some_and(|b| b.kind == SourceSymbolKind::Class)
            }
            std::cmp::Ordering::Greater => false,
        };
        if wins {
            best = Some(s);
            best_range = range;
        }
    }
    best
}

/// `varName -> AnnotatedType` for ONE lexical scope (module, or one `def`), from every annotated
/// parameter (`x: T`) and annotated assignment (`x: T = ...`) — module doc's "Receiver typing". Only a
/// BARE `Name` annotation is recorded: a subscripted/attribute annotation (`Optional[User]`,
/// `list[Item]`, `models.Model`) names no single receiver class this resolver could place, so it is
/// skipped rather than guessed at.
type Scope = HashMap<String, String>;

struct CallCollector<'a> {
    idx: &'a crate::LineIndex,
    bodies: &'a [&'a SourceSymbol],
    /// Module scope first, one per entered `def` after it — innermost wins on lookup.
    scopes: Vec<Scope>,
    out: Vec<RawCall>,
}

impl<'a> Visitor<'a> for CallCollector<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        self.visit_stmt_scoped(stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            self.visit_call(call);
        }
        walk_expr(self, expr);
    }
}

impl<'a> CallCollector<'a> {
    /// The structural walk — module doc "What is NOT a call edge here". `walk_stmt` is used only for
    /// statement kinds that carry no declaration position of their own; `def` and `class` are intercepted
    /// so their decorators, parameter lists and (for a class) non-`def` body statements are never walked.
    fn visit_stmt_scoped(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(f) => self.visit_function(f),
            // Only this class's own direct `def`s — module doc. A nested `class` member is skipped in
            // full (there is no symbol to attribute its methods to; see the module doc's second
            // paragraph on the class-body exclusion).
            Stmt::ClassDef(c) => {
                for member in &c.body {
                    if let Stmt::FunctionDef(f) = member {
                        self.visit_function(f);
                    }
                }
            }
            Stmt::AnnAssign(a) => {
                if let (Expr::Name(target), Expr::Name(t)) = (&*a.target, &*a.annotation) {
                    self.declare(target.id.as_str(), t.id.as_str());
                }
                walk_stmt(self, stmt);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    /// Enters a `def`: its parameters open a new scope, and ONLY its body is walked.
    fn visit_function(&mut self, f: &'a StmtFunctionDef) {
        let mut scope = Scope::new();
        for p in f.parameters.iter() {
            let param = p.as_parameter();
            if let Some(Expr::Name(t)) = param.annotation.as_deref() {
                scope.insert(param.name.as_str().to_string(), t.id.as_str().to_string());
            }
        }
        self.scopes.push(scope);
        for stmt in &f.body {
            self.visit_stmt_scoped(stmt);
        }
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str, ty: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty.to_string());
        }
    }

    /// Innermost-scope-first annotated type of `name`, or `None` when no enclosing scope annotates it.
    fn lookup_type(&self, name: &str) -> Option<String> {
        self.scopes.iter().rev().find_map(|s| s.get(name)).cloned()
    }

    fn visit_call(&mut self, call: &ruff_python_ast::ExprCall) {
        let line = self.idx.line_of(call.range.start());
        let Some(enclosing) = find_enclosing(line, self.bodies) else {
            return; // outside any tracked body (module level) — dropped
        };
        let (callee_name, receiver) = match &*call.func {
            Expr::Name(n) => (n.id.as_str().to_string(), None),
            Expr::Attribute(a) => match &*a.value {
                Expr::Name(recv) => (a.attr.as_str().to_string(), Some(recv.id.as_str())),
                // Chained/subscripted/attribute receiver — out of scope, never guessed. The caller's
                // `walk_expr` still recurses, so a call nested inside the receiver is collected.
                _ => return,
            },
            _ => return,
        };
        let receiver_type = match receiver {
            None => None,
            Some("self") | Some("cls") => match enclosing_class_of(enclosing) {
                Some(class) => Some(class),
                None => return, // `self` outside a `Class.method` body — no receiver identity
            },
            Some(recv) => Some(self.lookup_type(recv).unwrap_or_else(|| recv.to_string())),
        };
        self.out.push(RawCall {
            from_symbol: enclosing.id.clone(),
            callee_name,
            line,
            receiver_type,
            is_heritage: false,
        });
    }
}

/// The class half of a `Class.method` sub-symbol's dotted name (`lang::symbols`'s own convention), or
/// `None` for a module-level `def`/`class` symbol. Used only to give `self`/`cls` a receiver identity.
fn enclosing_class_of(enclosing: &SourceSymbol) -> Option<String> {
    enclosing
        .name
        .split_once('.')
        .map(|(class, _)| class.to_string())
}

#[cfg(test)]
mod tests;
