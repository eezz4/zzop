//! Django REST Framework view AUTH-GUARD evidence — the Django half of the framework-neutral
//! decorator/annotation auth channel (`zzop_rules_http::mutating_route_no_auth`'s "Decorator/annotation
//! auth exemption"). The FastAPI half is `crate::adapters::fastapi::guard`.
//!
//! ## Why this is a NAME judgment, not a `(file, line)` one
//! A Django route provide is anchored in the URLconf (`super`'s `urlpatterns` scan: `url(r'^x$',
//! MyView.as_view())` in `urls.py`), but the auth evidence lives on the VIEW CLASS in another file
//! (`views.py`'s `permission_classes = (IsAuthenticated,)`). The two halves never occur in the same
//! file, so this producer emits per-class VERDICTS and the engine joins them to a provide by the
//! provide's own `symbol` (the view name the URLconf scan already recorded). That join, and the
//! same-name-conflict drop it applies, live in `run_callgraph_rules`.
//!
//! ## Recognized shape (v1 — the one the shipped corpus uses)
//! Import-gated on `rest_framework` (the `permission_classes` attribute is DRF vocabulary). For each
//! TOP-LEVEL `class`, a direct-body `permission_classes = (A, B)` / `= [A, B]` assignment is read and the
//! class is reported as guarded iff at least one element NAMES a guard
//! ([`crate::adapters::guard_vocab::is_guard_name`], which accepts `IsAuthenticated` /
//! `IsAuthenticatedOrReadOnly` / `IsAdminUser` and rejects `AllowAny`). A class with no
//! `permission_classes` assignment is not reported AT ALL — absence of the attribute is not evidence of
//! absence of auth (DRF's project-wide `DEFAULT_PERMISSION_CLASSES` setting can supply it), and reporting
//! it as `false` would be a claim this scan cannot make.
//!
//! ## Not recognized (honest under-recognition — the finding still fires)
//! `DEFAULT_PERMISSION_CLASSES` in `settings.py`, `permission_classes` inherited from a base view class,
//! `get_permissions()` computed at runtime, the `@permission_classes([...])`/`@login_required` function-
//! view decorators (absent from the shipped corpus — never built speculatively), and any element that is
//! not a plain name/dotted member.

use ruff_python_ast::{Expr, Stmt, StmtClassDef};
use zzop_core::ImportMap;

use crate::adapters::guard_vocab::is_guard_name;

/// Per-view-class auth verdicts in `text` — `(class name, guarded)` for every top-level class that
/// DECLARES `permission_classes`. See module doc for why a class without the attribute is absent from
/// the result rather than reported `false`. Empty on parse failure and whenever the file does not import
/// `rest_framework` (never panics). Declaration order preserved.
pub fn extract_django_view_guard_classes(text: &str) -> Vec<(String, bool)> {
    extract_django_view_guard_classes_with_vocab(
        text,
        &crate::adapters::guard_vocab::PythonGuardVocab::built_in(),
    )
}

/// [`extract_django_view_guard_classes`] with the run's DECLARED Python guard vocabulary.
pub fn extract_django_view_guard_classes_with_vocab(
    text: &str,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> Vec<(String, bool)> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    if !imports_rest_framework(&imports) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for stmt in &module.body {
        let Stmt::ClassDef(c) = stmt else { continue };
        if let Some(guarded) = class_permission_verdict(c, vocab) {
            out.push((c.name.to_string(), guarded));
        }
    }
    out
}

fn imports_rest_framework(imports: &ImportMap) -> bool {
    imports
        .values()
        .any(|b| b.specifier == "rest_framework" || b.specifier.starts_with("rest_framework."))
}

/// `Some(guarded)` when the class's direct body declares `permission_classes` as a literal tuple/list;
/// `None` when it declares no such attribute (or declares it as something this scan cannot read, e.g. a
/// name reference or a call — never guessed).
fn class_permission_verdict(
    c: &StmtClassDef,
    vocab: &crate::adapters::guard_vocab::PythonGuardVocab<'_>,
) -> Option<bool> {
    for stmt in &c.body {
        let value = match stmt {
            Stmt::Assign(a) if a.targets.len() == 1 => match &a.targets[0] {
                Expr::Name(n) if n.id.as_str() == "permission_classes" => &*a.value,
                _ => continue,
            },
            _ => continue,
        };
        let elements: &[Expr] = match value {
            Expr::Tuple(t) => &t.elts,
            Expr::List(l) => &l.elts,
            _ => return None, // not a literal collection — never guessed
        };
        return Some(
            elements
                .iter()
                .any(|e| element_name(e).is_some_and(|n| is_guard_name(n, vocab))),
        );
    }
    None
}

/// A permission-class element's judged name: a bare name (`IsAuthenticated`) or a dotted member's
/// terminal segment (`permissions.IsAuthenticated`). `None` for any other shape.
fn element_name(e: &Expr) -> Option<&str> {
    match e {
        Expr::Name(n) => Some(n.id.as_str()),
        Expr::Attribute(a) => Some(a.attr.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
