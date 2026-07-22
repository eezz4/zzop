//! Django ORM model -> `db-table` PROVIDE and `.objects` manager-access -> `db-table` CONSUME extraction —
//! the second Python member of the ORM db-table family (alongside `sqlalchemy`, and TypeORM/GORM/Prisma).
//!
//! ## Provide side (import-gated on `django.db`'s `models`)
//! Django models rarely inherit `models.Model` DIRECTLY — the near-universal idiom is a project-local
//! abstract base (`class Article(TimestampedModel)`, `class User(AbstractBaseUser, …, TimestampedModel)`),
//! so a base-class-name check would miss them. Instead a class is recognized field-driven — the same shape
//! GORM uses for its `gorm:"…"` tags: a top-level class is a model when its body assigns at least one
//! `models.<Field>(…)` attribute (`slug = models.SlugField(…)`, `author = models.ForeignKey(…)`), where
//! `models` is the local name bound to `django.db`'s `models`. A class whose nested `class Meta` sets
//! `abstract = True` is an abstract base (`TimestampedModel`) and projects NO table; a manager/helper class
//! with no `models.<Field>(…)` assign (`UserManager(BaseUserManager)`) is likewise not a model.
//!
//! Table name = the `class Meta` `db_table = "…"` literal if present, else Django's default
//! `<app_label>_<model_lower>` (`articles_article`), with `app_label` taken from the model file's parent
//! directory — the Django app-package convention (`…/apps/articles/models.py` -> `articles`). This is a
//! best-effort key (a custom `AppConfig.label` or a non-conventional layout is not read); an off-by-app_label
//! key is an honest under-key, never a wrong finding, because the cross-layer link is symbol-based, not
//! key-based. Emitted as `IoProvide { kind: "db-table", key: "table:<casing(name)>", symbol: Some(<class>) }`.
//!
//! ## Consume side (NOT import-gated — see below)
//! Django's query entrypoint is the default manager `<Model>.objects` (`Article.objects.filter(…)`,
//! `Comment.objects.get(…)`, `Tag.objects.all()`). Every `<Name>.objects` attribute access names a touched
//! model. Emitted as `IoConsume { kind: "db-table", key: None, raw: Some(<Name>) }`, resolved engine-side
//! against the provide `symbol` index (`resolve_orm_entity_consumes`), deduped per model per file.
//!
//! Unlike the `sqlalchemy`/GORM siblings (whose generic query verbs `get`/`query`/`Find` force a
//! per-file framework import gate), the consume side here is deliberately NOT import-gated: the query
//! files (Django REST `views.py`/`serializers.py`) import `rest_framework` and `.models`, NOT `django`
//! directly, so a `text.contains("django")` gate would drop the majority of query sites. The `.objects`
//! manager access is itself the Django-specific signal (far narrower than a bare `.get`), and the real
//! guard is the same downstream resolution-drop the whole family leans on — a `<Name>.objects` whose
//! `<Name>` is not a real model provide stays unresolved and inert. A cheap `text.contains(".objects")`
//! pre-skip keeps files that never touch a manager free.

use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::{Expr, Stmt, StmtClassDef};
use zzop_core::{ImportMap, IoConsume, IoProvide};

/// Local names bound to `django.db`'s `models` module (`from django.db import models` -> `models`;
/// `import django.db.models as m` -> `m`). Empty when the file does not import it.
fn django_models_names(imports: &ImportMap) -> std::collections::HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| {
            (b.specifier == "django.db" && b.original == "models")
                || b.specifier == "django.db.models"
        })
        .map(|(local, _)| local.clone())
        .collect()
}

/// Extract Django model `db-table` provides. Empty when the file imports no `django.db` `models`.
pub fn extract_django_db_table_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let models = django_models_names(&crate::lang::imports::parse_imports(text));
    if models.is_empty() {
        return Vec::new();
    }
    let idx = crate::LineIndex::new(text);
    let app_label = app_label_from_path(rel);
    let mut out = Vec::new();
    for stmt in &module.body {
        if let Stmt::ClassDef(cls) = stmt {
            if let Some(table) = model_table_name(cls, &models, app_label) {
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

/// `Some(table_name)` when `cls` is a concrete Django model: it has at least one `models.<Field>(…)` body
/// assign AND its `class Meta` does not declare `abstract = True`. Name = the `db_table` Meta literal if
/// present, else the Django default `<app_label>_<model_lower>` (or bare `<model_lower>` when the file has
/// no app-directory parent).
fn model_table_name(
    cls: &StmtClassDef,
    models: &std::collections::HashSet<String>,
    app_label: Option<&str>,
) -> Option<String> {
    if !has_model_field(cls, models) || meta_is_abstract(cls) {
        return None;
    }
    if let Some(explicit) = meta_db_table(cls) {
        return Some(explicit);
    }
    let lower = cls.name.to_lowercase();
    Some(match app_label {
        Some(app) => format!("{app}_{lower}"),
        None => lower,
    })
}

/// True when the class body has at least one `x = models.<attr>(…)` / `x: T = models.<attr>(…)` assign.
/// NOTE: "field" here is ANY call on the `models` module, not a whitelist of `*Field` constructors — a
/// deliberately broad, robust signal (custom/third-party field types all pass). The residual (a body
/// binding only a non-field `models.*()` like `objects = models.Manager()` and NO real field) is not a
/// real-model idiom and, even if minted, only links if something does `ThatName.objects` — bounded.
fn has_model_field(cls: &StmtClassDef, models: &std::collections::HashSet<String>) -> bool {
    cls.body.iter().any(|stmt| {
        let Some(Expr::Call(call)) = assign_value(stmt) else {
            return false;
        };
        matches!(&*call.func, Expr::Attribute(attr)
            if matches!(&*attr.value, Expr::Name(n) if models.contains(n.id.as_str())))
    })
}

/// True when a nested `class Meta` sets `abstract = True` (an abstract base — no physical table).
fn meta_is_abstract(cls: &StmtClassDef) -> bool {
    meta_body(cls).is_some_and(|body| {
        body.iter().any(|stmt| {
            matches!(named_assign_value(stmt, "abstract"),
                Some(Expr::BooleanLiteral(b)) if b.value)
        })
    })
}

/// The `class Meta` `db_table = "…"` string literal, if any.
fn meta_db_table(cls: &StmtClassDef) -> Option<String> {
    meta_body(cls)?
        .iter()
        .find_map(|stmt| match named_assign_value(stmt, "db_table") {
            Some(Expr::StringLiteral(s)) => Some(s.value.to_str().to_string()),
            _ => None,
        })
}

/// The body of a nested `class Meta`, if the class declares one.
fn meta_body(cls: &StmtClassDef) -> Option<&[Stmt]> {
    cls.body.iter().find_map(|stmt| match stmt {
        Stmt::ClassDef(inner) if inner.name.as_str() == "Meta" => Some(&*inner.body),
        _ => None,
    })
}

/// The RHS value of any simple or annotated assign (`x = v` / `x: T = v`), target-agnostic; `None` for a
/// non-assign or a bare annotation (`x: T`). Handles both forms so annotated fields are not missed —
/// parity with the `sqlalchemy` sibling's `__tablename__` extraction.
fn assign_value(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Assign(a) => Some(&a.value),
        Stmt::AnnAssign(a) => a.value.as_deref(),
        _ => None,
    }
}

/// The RHS value of a simple/annotated assign whose target is exactly the `Name` `name` (`<name> = v` /
/// `<name>: T = v`), else `None`. Covers both assign forms, as `assign_value` does.
fn named_assign_value<'a>(stmt: &'a Stmt, name: &str) -> Option<&'a Expr> {
    let is_name = |e: &Expr| matches!(e, Expr::Name(n) if n.id.as_str() == name);
    match stmt {
        Stmt::Assign(a) if a.targets.iter().any(is_name) => Some(&a.value),
        Stmt::AnnAssign(a) if is_name(&a.target) => a.value.as_deref(),
        _ => None,
    }
}

/// The Django app label for a model file: the parent directory name of `<app>/models.py`
/// (`conduit/apps/articles/models.py` -> `articles`). `None` when the file has no directory parent.
fn app_label_from_path(rel: &str) -> Option<&str> {
    let normalized = rel.trim_end_matches('/');
    let (dir, _file) = normalized.rsplit_once(['/', '\\'])?;
    let app = dir.rsplit(['/', '\\']).next()?;
    (!app.is_empty()).then_some(app)
}

/// Extract Django `.objects` manager-access `db-table` consumes (`key: None`, `raw: <model class name>`).
pub fn extract_django_db_table_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    // Cheap pre-skip: the `.objects` manager access is the signal (no framework import gate — see doc).
    if !text.contains(".objects") {
        return Vec::new();
    }
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
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
        // `<Model>.objects` — an attribute access `objects` on a bare, PascalCase-`Name` base. The
        // uppercase-initial restriction excludes the non-model `self.objects` / `queryset.objects` shapes
        // (a bare `Name` base too, but never a model): Django model classes are PascalCase by universal
        // convention, so a lowercase base is never a model manager. A truly lowercased model (unheard of)
        // would be an honest miss, never a wrong finding.
        if let Expr::Attribute(attr) = expr {
            if attr.attr.as_str() == "objects" {
                if let Expr::Name(model) = &*attr.value {
                    let name = model.id.to_string();
                    let is_model = name.chars().next().is_some_and(char::is_uppercase);
                    if is_model && self.seen.insert(name.clone()) {
                        self.out.push(IoConsume {
                            client: None,
                            body: None,
                            kind: "db-table".to_string(),
                            key: None,
                            file: self.rel.to_string(),
                            line: self.idx.line_of(attr.range.start()),
                            raw: Some(name),
                            method: None,
                            retry_configured: None,
                        });
                    }
                }
            }
        }
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests;
