//! Bare-receiver anchor-form evidence gathering for `db_table_consume` — which local identifiers THIS
//! FILE carries evidence bind to a Prisma client. See the parent module doc for the two anchor forms.

use std::collections::HashSet;

use swc_core::ecma::ast::{BinExpr, BinaryOp, Expr, Module, Pat, VarDeclarator};
use swc_core::ecma::visit::{Visit, VisitWith};

use super::recognize::unwrap_expr;

/// Local identifiers THIS FILE carries evidence bind to a Prisma client — the bare-receiver consume
/// form's precision guard. Either evidence rule below is independently sufficient; a plain
/// `foo.bar.baz()` with neither present never matches (`recognize::match_prisma_model_call`'s
/// bare-receiver arm only ever sees identifiers landing in this set).
///
/// - The file imports a VALUE binding of `PrismaClient` from `@prisma/client` — a named import literally
///   called `PrismaClient`, a default import, or a namespace import (type-only imports excluded: no
///   runtime value, so no call-site evidence). That binding name itself counts as a receiver (some setups
///   re-export an already-built singleton under the class's own import name), and so does any local
///   variable initialized to `new <that binding>(...)`, optionally behind a `<fallback> ||
///   new PrismaClient(...)` guard — the exact idiom in the be-express corpus's
///   `src/prisma/prisma-client.ts` (`const prisma = global.prisma || new PrismaClient();`).
/// - The file imports ANY binding from a relative specifier ("local module") containing "prisma"
///   case-insensitively — the be-express ROUTE-file idiom, `import prisma from
///   '../../../prisma/prisma-client'`, where the actual `new PrismaClient()` lives in a different file
///   this per-file extractor never sees.
pub(super) fn prisma_bound_receivers(rel: &str, text: &str, module: &Module) -> HashSet<String> {
    let imports = crate::parse_imports(rel, text);
    let mut receivers = HashSet::new();

    for (local, binding) in imports.iter() {
        if !binding.type_only
            && binding.specifier.starts_with('.')
            && binding.specifier.to_ascii_lowercase().contains("prisma")
        {
            receivers.insert(local.clone());
        }
    }

    let prisma_client_ident = imports.iter().find_map(|(local, binding)| {
        let is_class_binding = binding.original == "PrismaClient"
            || binding.original == "default"
            || binding.original == "*";
        if !binding.type_only && binding.specifier == "@prisma/client" && is_class_binding {
            Some(local.clone())
        } else {
            None
        }
    });
    if let Some(class_ident) = prisma_client_ident {
        receivers.insert(class_ident.clone());
        let mut finder = NewClientFinder {
            class_ident: &class_ident,
            out: HashSet::new(),
        };
        module.visit_with(&mut finder);
        receivers.extend(finder.out);
    }

    receivers
}

/// Collects local variables initialized via `new <class_ident>(...)`, directly or behind a
/// `<fallback> || new <class_ident>(...)` guard (the `global.prisma || new PrismaClient()` idiom).
struct NewClientFinder<'a> {
    class_ident: &'a str,
    out: HashSet<String>,
}

impl Visit for NewClientFinder<'_> {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(name), Some(init)) = (&d.name, &d.init) {
            if is_new_call_of(init, self.class_ident) {
                self.out.insert(name.id.sym.to_string());
            }
        }
        d.visit_children_with(self);
    }
}

/// `true` when `e` (before unwrapping) is `new <ident>(...)`, or a `<left> || <right>` logical-OR whose
/// EITHER side is (recursing to allow deeper fallback chains).
fn is_new_call_of(e: &Expr, ident: &str) -> bool {
    match unwrap_expr(e) {
        Expr::New(n) => matches!(unwrap_expr(&n.callee), Expr::Ident(id) if id.sym == ident),
        Expr::Bin(BinExpr {
            op: BinaryOp::LogicalOr,
            left,
            right,
            ..
        }) => is_new_call_of(left, ident) || is_new_call_of(right, ident),
        _ => false,
    }
}
