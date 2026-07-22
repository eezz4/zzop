//! SQLModel / SQLAlchemy model -> `db-table` PROVIDE and query-access -> `db-table` CONSUME extraction —
//! the Python member of the ORM db-table family (alongside TypeORM/GORM and the Prisma/SQL provide sides).
//! Import-gated on `sqlmodel`/`sqlalchemy`; a file that imports neither yields nothing.
//!
//! ## Provide side
//! A class is a table model when it either declares `table=True` in its class arguments (the SQLModel
//! idiom: `class User(UserBase, table=True):`) or assigns `__tablename__ = "…"` in its body (the
//! SQLAlchemy declarative idiom). Table name = the `__tablename__` string literal if present, else the
//! SQLModel default (the class name lowercased). Emitted as `IoProvide { kind: "db-table",
//! key: "table:<casing(name)>", symbol: Some(<class name>) }` — `symbol` carries the class name so the
//! engine resolves the consumes below, identical to the TypeORM `@Entity` / GORM struct provide.
//!
//! ## Consume side
//! A model class referenced as the first argument of a SQLAlchemy/SQLModel query call — `select(Item)`,
//! `session.get(User, id)`, `session.query(Item)`, `<stmt>.select_from(Item)` — is a table touch. Emitted
//! as `IoConsume { kind: "db-table", key: None, raw: Some(<class name>) }`, resolved engine-side against
//! the provide `symbol` index (`resolve_orm_entity_consumes`). Only a bare-`Name` first argument is taken.
//!
//! ## Precision
//! Like GORM's `db.Find(…)`, the query names (`get`/`query`/`select`) are generic — `session.get(User)`
//! shares `.get` with `dict.get`. Beyond the sqlmodel/sqlalchemy import gate the safety is downstream: a
//! consume is `key: None` and becomes a finding-bearing fact only if the engine resolves its `raw` against
//! a real model provide; a coincidental `foo.get(bar)` whose `bar` is not a model stays inert. The
//! residual accepted FP — identical to GORM's — is the case where `bar` IS a model but the call is not a
//! query (`handlers.get(User)`, `type_map.get(Item)` in a sqlalchemy-importing file): it resolves and
//! mints a spurious table touch on that function. This is the deliberate tradeoff for covering the generic
//! ORM verbs without a receiver-type check; the import gate keeps it to sqlalchemy-importing files.
//!
//! The provide side scans only the module's top-level classes (the universal model-definition convention),
//! while the consume side recurses the whole tree; a model class nested in a function or `TYPE_CHECKING`
//! guard therefore yields no provide, leaving its consumes unresolved and inert (an honest under-report,
//! never a wrong finding). GORM's provide side walks the full tree, so this is a small sibling divergence.

use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef};
use zzop_core::{ImportMap, IoConsume, IoProvide};

/// Query verbs (bare `select(…)` or method `.query(…)`) that take N model arguments — SQLAlchemy 2.0
/// multi-entity form `select(User, Item)` / `session.query(User, Item)`: EVERY bare-`Name` arg is a model.
const MULTI_MODEL_VERBS: &[&str] = &["select", "query"];
/// Query verbs (`.get(Entity, id)`, `select_from(Entity)`) that take the model as their FIRST argument
/// only — the trailing arg is an id value / non-model, so only the first bare-`Name` arg is a model.
const FIRST_ARG_VERBS: &[&str] = &["get", "select_from"];

fn imports_orm(imports: &ImportMap) -> bool {
    imports.iter().any(|(_, b)| {
        let s = b.specifier.as_str();
        s == "sqlmodel"
            || s == "sqlalchemy"
            || s.starts_with("sqlmodel.")
            || s.starts_with("sqlalchemy.")
    })
}

/// Extract SQLModel/SQLAlchemy model `db-table` provides. Empty when the file imports no ORM.
pub fn extract_sqlalchemy_db_table_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    if !imports_orm(&crate::lang::imports::parse_imports(text)) {
        return Vec::new();
    }
    let idx = crate::LineIndex::new(text);
    let mut out = Vec::new();
    for stmt in &module.body {
        if let Stmt::ClassDef(cls) = stmt {
            if let Some(table) = model_table_name(cls) {
                out.push(IoProvide {
                    kind: "db-table".to_string(),
                    key: format!("table:{}", zzop_core::db_table_channel_casing(&table)),
                    file: rel.to_string(),
                    line: idx.line_of(cls.range.start()),
                    symbol: Some(cls.name.to_string()),
                    body: None,
                });
            }
        }
    }
    out
}

/// `Some(table_name)` when `cls` is a table model: `table=True` in the class arguments OR a
/// `__tablename__ = "…"` body assignment. Name = the `__tablename__` literal if present, else the class
/// name lowercased (SQLModel's default `__tablename__`).
fn model_table_name(cls: &StmtClassDef) -> Option<String> {
    let explicit = tablename_literal(cls);
    let is_table = declares_table_true(cls) || explicit.is_some();
    if !is_table {
        return None;
    }
    Some(explicit.unwrap_or_else(|| cls.name.to_lowercase()))
}

/// True when the class arguments carry `table=True`.
fn declares_table_true(cls: &StmtClassDef) -> bool {
    let Some(args) = &cls.arguments else {
        return false;
    };
    args.keywords.iter().any(|kw| {
        kw.arg.as_ref().is_some_and(|a| a.as_str() == "table")
            && matches!(&kw.value, Expr::BooleanLiteral(b) if b.value)
    })
}

/// The string literal of a `__tablename__ = "…"` assignment in the class body, if any. Handles both the
/// plain assign (`__tablename__ = "users"`) and the annotated form (`__tablename__: str = "users"`, a
/// `Stmt::AnnAssign` — valid on a SQLAlchemy declarative model).
fn tablename_literal(cls: &StmtClassDef) -> Option<String> {
    for stmt in &cls.body {
        let (targets_tablename, value) = match stmt {
            Stmt::Assign(a) => (
                a.targets
                    .iter()
                    .any(|t| matches!(t, Expr::Name(n) if n.id.as_str() == "__tablename__")),
                Some(&*a.value),
            ),
            Stmt::AnnAssign(a) => (
                matches!(&*a.target, Expr::Name(n) if n.id.as_str() == "__tablename__"),
                a.value.as_deref(),
            ),
            _ => continue,
        };
        if targets_tablename {
            if let Some(Expr::StringLiteral(s)) = value {
                return Some(s.value.to_str().to_string());
            }
        }
    }
    None
}

/// Extract SQLModel/SQLAlchemy query `db-table` consumes (`key: None`, `raw: <model class name>`).
pub fn extract_sqlalchemy_db_table_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    if !imports_orm(&crate::lang::imports::parse_imports(text)) {
        return Vec::new();
    }
    let idx = crate::LineIndex::new(text);
    let mut collector = ConsumeCollector {
        rel,
        idx: &idx,
        seen: std::collections::BTreeSet::new(),
        out: Vec::new(),
    };
    for stmt in &module.body {
        collector.visit_stmt(stmt);
    }
    collector.out
}

struct ConsumeCollector<'a> {
    rel: &'a str,
    idx: &'a crate::LineIndex,
    seen: std::collections::BTreeSet<String>,
    out: Vec<IoConsume>,
}

impl<'a> Visitor<'a> for ConsumeCollector<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            for model in query_call_models(call) {
                if self.seen.insert(model.clone()) {
                    self.out.push(IoConsume {
                        client: None,
                        body: None,
                        kind: "db-table".to_string(),
                        key: None,
                        file: self.rel.to_string(),
                        line: self.idx.line_of(call.range.start()),
                        raw: Some(model),
                        method: None,
                        retry_configured: None,
                    });
                }
            }
        }
        walk_expr(self, expr); // recurse into nested calls (e.g. `session.exec(select(Item))`)
    }
}

/// The model class names when `call` is a query call. A `MULTI_MODEL_VERBS` call (`select`/`query`) yields
/// EVERY bare-`Name` argument (the 2.0 multi-entity form `select(User, Item)`); a `FIRST_ARG_VERBS` call
/// (`get`/`select_from`) yields only its first argument if that is a bare `Name` (the trailing `get` arg
/// is an id value, never a model). Empty for a non-query call or a non-`Name` argument (a computed/dotted
/// entity like `models.User` is never guessed — argument-level never-guess, mirroring the provide side).
fn query_call_models(call: &ruff_python_ast::ExprCall) -> Vec<String> {
    let verb = match &*call.func {
        Expr::Name(n) => n.id.as_str(),
        Expr::Attribute(a) => a.attr.as_str(),
        _ => return Vec::new(),
    };
    let name_arg = |arg: &Expr| match arg {
        Expr::Name(n) => Some(n.id.to_string()),
        _ => None,
    };
    if MULTI_MODEL_VERBS.contains(&verb) {
        call.arguments.args.iter().filter_map(name_arg).collect()
    } else if FIRST_ARG_VERBS.contains(&verb) {
        call.arguments
            .args
            .first()
            .and_then(name_arg)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
