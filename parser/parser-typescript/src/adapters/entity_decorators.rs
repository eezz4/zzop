//! TypeORM `@Entity(...)` decorator -> `db-table` PROVIDE extraction — a sibling of
//! `controller_decorators`'s NestJS route extractor (same swc-AST-visitor shape, gated on a
//! different class-level decorator) but feeding the DB-schema channel instead of the HTTP one.
//! Recognizes a class-level `@Entity('table_name')` (or `@Entity({ name: 'table_name' })`) decorator
//! and emits ONE `db-table` `IoProvide` keyed `table:<db_table_channel_casing(name)>` — the exact
//! same key shape `zzop_parser_prisma::analysis::build_common_ir` emits from a PSL `model` and
//! `zzop_parser_sql::extract` emits from a `CREATE TABLE` DDL statement, routed through the shared
//! [`zzop_core::db_table_channel_casing`] transform so all three sides join on one physical table.
//!
//! ## Never-guess boundary
//! A bare `@Entity()` (no call, or a call with no argument) is deliberately SKIPPED rather than
//! name-derived from the class name: TypeORM's default table name for a bare `@Entity()` depends on
//! the app's configured `NamingStrategy` (snake_case-by-convention, but overridable per project), so
//! guessing one would risk a wrong key more often than omitting it helps — this repo's "never guess"
//! IO convention (see `egress.rs`'s `resolve_url`, `controller_decorators.rs`'s dynamic-prefix skip).
//! Any other non-literal argument shape (a computed/dynamic first arg, or an object's `name`
//! property that isn't itself a string literal) is skipped for the same reason.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    Callee, ClassDecl, Decorator, Expr, Lit, ObjectLit, Prop, PropName, PropOrSpread, Str,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoProvide;

/// Extracts TypeORM `@Entity(...)`-decorated class `db-table` `IoProvide`s from one TS file's raw
/// source — see module doc for the recognized shapes and never-guess boundary. Returns an empty
/// `Vec` (never panics) on an unparseable file, same convention as every other swc-AST adapter in
/// this crate.
pub fn extract_entity_db_table_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut c = EntityCollector {
        cm: cm_ref,
        file: rel,
        out: Vec::new(),
    };
    module.visit_with(&mut c);
    c.out
}

struct EntityCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Vec<IoProvide>,
}

impl Visit for EntityCollector<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        if let Some((table, decorator)) = entity_table_name(&n.class.decorators) {
            self.out.push(IoProvide {
                kind: "db-table".to_string(),
                key: format!("table:{}", zzop_core::db_table_channel_casing(&table)),
                file: self.file.to_string(),
                line: crate::line_of(self.cm, decorator.span.lo),
                // The decorated class name (`ArticleEntity`) — carried so the engine can build an
                // entity-class -> table-key index and resolve TypeORM repository CONSUMES
                // (`Repository<ArticleEntity>` / `@InjectRepository(ArticleEntity)`), which reference the
                // class, not the physical table string. See `db_table_consume`'s TypeORM branch.
                symbol: Some(n.ident.sym.to_string()),
                body: None,
            });
        }
        n.visit_children_with(self); // recurse — covers any nested class declarations
    }
}

/// Scans a class's own decorators for `@Entity(...)` and returns its resolved table name plus the
/// matching decorator (for the provide's anchor line), or `None` when no `@Entity` decorator is
/// present, it's bare/argument-less, or the argument isn't a recognized literal shape (never-guess).
fn entity_table_name(decorators: &[Decorator]) -> Option<(String, &Decorator)> {
    for d in decorators {
        if decorator_name(&d.expr).as_deref() != Some("Entity") {
            continue;
        }
        let name = entity_table_name_from_expr(&d.expr)?;
        return Some((name, d));
    }
    None
}

// Bare `@Entity` (no parens at all) and empty-parens `@Entity()` both yield `None` — see module doc.
fn entity_table_name_from_expr(expr: &Expr) -> Option<String> {
    let Expr::Call(call) = expr else {
        return None; // bare `@Entity` — no arg, never-guess
    };
    let arg = call.args.first()?; // `@Entity()` — no arg, never-guess
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(str_value(s)),
        Expr::Object(obj) => object_table_name(obj),
        _ => None, // dynamic/computed arg — never guess
    }
}

/// The object-argument form's `name` property (`@Entity({ name: 'table_name' })`), read only when the
/// property value is itself a plain string literal. `None` when the property is absent or its value
/// is anything else (identifier, call, template, ...) — never guess.
fn object_table_name(obj: &ObjectLit) -> Option<String> {
    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**p else {
            continue;
        };
        let is_name = match &kv.key {
            PropName::Ident(i) => i.sym.as_str() == "name",
            PropName::Str(s) => str_value(s) == "name",
            _ => false,
        };
        if !is_name {
            continue;
        }
        return match &*kv.value {
            Expr::Lit(Lit::Str(s)) => Some(str_value(s)),
            _ => None, // non-literal `name` value — never guess
        };
    }
    None // no `name` property found
}

/// The decorator's callee/identifier name: `Entity` from both bare `@Entity` and called
/// `@Entity(...)`. `None` for any unrecognized shape (a member expression, a non-identifier
/// callee, ...). Mirrors `controller_decorators::method_facts::decorator_name` — duplicated locally
/// since that helper is private to its own sibling module.
fn decorator_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Call(call) => match &call.callee {
            Callee::Expr(callee) => match &**callee {
                Expr::Ident(id) => Some(id.sym.to_string()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn str_value(s: &Str) -> String {
    s.value.as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_entity_db_table_provides`: the string-literal happy path (with casing),
    //! the object `{ name: ... }` form, the bare-`@Entity()` never-guess skip, and the
    //! non-`@Entity`-class no-op.
    use super::*;

    #[test]
    fn literal_entity_name_yields_a_lowercase_keyed_provide() {
        let src = "@Entity('article')\nexport class ArticleEntity {}\n";
        let out = extract_entity_db_table_provides("article.entity.ts", src);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "db-table");
        assert_eq!(out[0].key, "table:article");
        assert_eq!(out[0].file, "article.entity.ts");
        assert_eq!(out[0].line, 1);
        // The provide carries the decorated class name so the engine can resolve `Repository<ArticleEntity>`
        // consumes against it.
        assert_eq!(out[0].symbol.as_deref(), Some("ArticleEntity"));
    }

    #[test]
    fn pascal_case_entity_name_is_recased_via_db_table_channel_casing() {
        let src = "@Entity('User')\nexport class UserEntity {}\n";
        let out = extract_entity_db_table_provides("user.entity.ts", src);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "table:user");
    }

    #[test]
    fn object_form_name_property_is_recognized() {
        let src = "@Entity({ name: 'follows' })\nexport class FollowsEntity {}\n";
        let out = extract_entity_db_table_provides("follows.entity.ts", src);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "table:follows");
    }

    #[test]
    fn bare_entity_with_no_argument_is_never_guessed() {
        let src = "@Entity()\nexport class CommentEntity {}\n";
        assert!(extract_entity_db_table_provides("comment.entity.ts", src).is_empty());
    }

    #[test]
    fn bare_entity_with_no_parens_is_never_guessed() {
        let src = "@Entity\nexport class CommentEntity {}\n";
        assert!(extract_entity_db_table_provides("comment.entity.ts", src).is_empty());
    }

    #[test]
    fn non_entity_class_yields_nothing() {
        let src = "@Injectable()\nexport class SomeService {}\n";
        assert!(extract_entity_db_table_provides("some.service.ts", src).is_empty());
    }

    #[test]
    fn dynamic_entity_argument_is_never_guessed() {
        let src = "@Entity(tableNameFromConfig())\nexport class DynamicEntity {}\n";
        assert!(extract_entity_db_table_provides("dynamic.entity.ts", src).is_empty());
    }
}
