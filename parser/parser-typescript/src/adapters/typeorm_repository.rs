//! TypeORM repository-access -> `db-table` CONSUME extraction. Unlike the Prisma consume side
//! (`db_table_consume`), where the call accessor (`prisma.article`) IS the table name, TypeORM code
//! references the ENTITY CLASS (`Repository<ArticleEntity>`, `@InjectRepository(ArticleEntity)`,
//! `getRepository(ArticleEntity)`) and the physical table name lives in that class's `@Entity('article')`
//! decorator in ANOTHER file. So this adapter cannot key the consume itself: it emits a `db-table`
//! `IoConsume` with `key: None` and `raw: Some("ArticleEntity")` (the entity class name). The engine's
//! whole-corpus assembly resolves it against the entity-class -> table-key index built from the
//! `entity_decorators` provides (which carry the class name in their `symbol`) — see the engine's
//! `resolve_orm_entity_consumes` pass. A `db-table` consume with `key == None` is, by construction,
//! exactly a TypeORM entity-class reference (the Prisma side always sets a key), so the engine's resolver
//! keys off that.
//!
//! ## Recognized shapes (each yields the referenced entity class)
//! - `@InjectRepository(ArticleEntity)` — the NestJS/TypeORM DI decorator; first argument is the entity.
//! - `getRepository(ArticleEntity)` / `<x>.getRepository(ArticleEntity)` — the imperative form; first
//!   argument is the entity.
//!
//! Only a bare-identifier first argument is taken (never a computed/dynamic entity — that argument-level
//! never-guess mirrors `entity_decorators`). Deduped per file by entity-class name: `@InjectRepository(X)`
//! on a constructor param whose type is also `Repository<X>` is ONE table touch, not two. Test/spec files
//! are skipped before parsing (their DB access is not deployed coupling), mirroring `db_table_consume`.
//!
//! ## Framework-presence gate
//! `getRepository` is a generic method name, so — like `db_table_consume`'s receiver-import evidence gate —
//! this adapter is import-gated: it does nothing unless the file text references `typeorm` (the package
//! `'typeorm'` for `getRepository`/`Repository<>`, or `'@nestjs/typeorm'` for `@InjectRepository`, both of
//! which spell `typeorm`). Without that, a custom `foo.getRepository(Bar)` in a non-TypeORM codebase would
//! mint a spurious consume. The gate is a cheap file-text pre-skip, the same shape as `require_file`.

use std::collections::BTreeSet;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{CallExpr, Callee, Decorator, Expr};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoConsume;

/// Extract TypeORM repository-access `db-table` consumes (unresolved — `key: None`, `raw: <entity class>`)
/// from one file's raw source. Empty for a test file or an unparseable file (never panics).
pub fn extract_typeorm_repository_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    // Framework-presence gate (see module doc): both `'typeorm'` and `'@nestjs/typeorm'` spell `typeorm`,
    // so a file that never mentions it is not a TypeORM consumer — skip before parsing.
    if !text.contains("typeorm") {
        return Vec::new();
    }
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut collector = RepoCollector {
        cm: cm_ref,
        file: rel,
        // (entity_class -> first line seen), ordered so output is deterministic and deduped per class.
        seen: BTreeSet::new(),
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

struct RepoCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    seen: BTreeSet<String>,
    out: Vec<IoConsume>,
}

impl RepoCollector<'_> {
    fn record(&mut self, entity: String, span_lo: swc_core::common::BytePos) {
        if !self.seen.insert(entity.clone()) {
            return; // already emitted a consume for this entity class in this file
        }
        self.out.push(IoConsume {
            client: None,
            body: None,
            kind: "db-table".into(),
            key: None, // unresolved — the engine resolves `raw` (entity class) against the entity index
            file: self.file.into(),
            line: crate::line_of(self.cm, span_lo),
            raw: Some(entity),
            method: None,
            retry_configured: None,
        });
    }
}

impl Visit for RepoCollector<'_> {
    fn visit_decorator(&mut self, d: &Decorator) {
        // `@InjectRepository(ArticleEntity)` — a call decorator whose callee is `InjectRepository`.
        if let Expr::Call(call) = &*d.expr {
            if callee_ident(&call.callee).as_deref() == Some("InjectRepository") {
                if let Some(entity) = first_arg_ident(call) {
                    self.record(entity, d.span.lo);
                }
            }
        }
        d.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        // `getRepository(ArticleEntity)` (bare) or `<recv>.getRepository(ArticleEntity)` (member).
        if callee_method_name(&call.callee).as_deref() == Some("getRepository") {
            if let Some(entity) = first_arg_ident(call) {
                self.record(entity, call.span.lo);
            }
        }
        call.visit_children_with(self);
    }
}

/// The bare-identifier callee of a call (`getRepository(...)` -> `getRepository`), else `None`.
fn callee_ident(callee: &Callee) -> Option<String> {
    match callee {
        Callee::Expr(e) => match &**e {
            Expr::Ident(id) => Some(id.sym.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// The method/identifier name a callee resolves to, covering both a bare call (`getRepository(...)`) and
/// a member call (`manager.getRepository(...)` / `this.conn.getRepository(...)`) — the trailing property
/// name in the member case. `None` for a computed member or non-ident callee.
fn callee_method_name(callee: &Callee) -> Option<String> {
    match callee {
        Callee::Expr(e) => match &**e {
            Expr::Ident(id) => Some(id.sym.to_string()),
            Expr::Member(m) => match &m.prop {
                swc_core::ecma::ast::MemberProp::Ident(i) => Some(i.sym.to_string()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// The first call argument when it is a bare identifier (the entity class), else `None` (never guess a
/// computed/dynamic entity, a spread, or a non-identifier).
fn first_arg_ident(call: &CallExpr) -> Option<String> {
    let arg = call.args.first()?;
    if arg.spread.is_some() {
        return None;
    }
    match &*arg.expr {
        Expr::Ident(id) => Some(id.sym.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
