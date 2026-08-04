//! The Python half of the project-wide constant map — the same `HashMap<dotted access, value>` channel
//! `zzop_parser_typescript::adapters::egress::consts` fills, resolved by the identical engine-side
//! merge (`zzop_engine::analyze::compose::merge_const_map_fragments`).
//!
//! Nothing here is new machinery. The merge, the deterministic first-writer-wins fold, the late
//! cross-file consume re-resolution and the controller-prefix provide resolution are all written,
//! tested and language-neutral already; this crate simply had no producer, so every Python constant
//! looked unresolvable to a layer that could have resolved it.
//!
//! ## DOTTED KEYS ONLY — the same rule TS states, for the same reason
//! A bare `API_URL = "https://x"` is deliberately NOT captured. This map is project-wide and
//! scope-insensitive, so a bare common name (`path`, `url`, `base`, `prefix`) could shadow a function
//! parameter in an unrelated file and mis-key someone else's call — a guess wearing the costume of a
//! visible fact. Only accesses that carry their own namespace are emitted.
//!
//! ## The two shapes that carry a namespace in Python
//! 1. `class Settings: API_V1_STR: str = "/api/v1"` → `Settings.API_V1_STR`. A class attribute is
//!    reached through the class (or an instance), so the name is never bare at the use site.
//! 2. `settings = Settings()` at module level → ALSO `settings.API_V1_STR`, for every attribute of that
//!    class. This is the shape that matters in practice: the pydantic-settings idiom declares the class
//!    and instantiates it once in the same module, and every consumer writes `settings.X`.
//!
//! Both halves must be in ONE file, because a fragment is a one-file scan. Measured on the corpus this
//! covers `be-fastapi-fs` (`class Settings` and `settings = Settings()` both in `app/core/config.py`)
//! and does NOT cover `be-fastapi`, whose instance comes from a cached factory in another module. The
//! second one stays unresolved and its S14 warning keeps speaking — which is the correct outcome, not a
//! shortfall to paper over: never-guess is the contract, and a fragment that invented the link would be
//! the defect this whole layer exists to avoid.
//!
//! ## What is skipped, and why each is not a guess
//! * A non-string value (int, call, f-string, `os.environ[...]`) — the map's values are strings, and an
//!   environment lookup has no compile-time value at all.
//! * A class attribute whose value comes from a default_factory or a descriptor.
//! * Any binding not at module level, and any class not at module level: same top-level-only v1 scope
//!   `lang::symbols` and `adapters::fastapi` already state.
//! * Re-assignment: first writer wins WITHIN the file, matching the engine-side merge's own rule across
//!   files, so the result does not depend on iteration order anywhere.

use std::collections::HashMap;

use ruff_python_ast::{Expr, Stmt};

/// One file's constant-map fragment: dotted access -> string value. Empty for the overwhelming majority
/// of files, which is why the engine only stores non-empty fragments.
pub fn const_map_fragment(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(module) = crate::parse_module(text) else {
        return out;
    };

    // Pass 1: every top-level class's string-literal attributes, keyed by the CLASS name.
    let mut by_class: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for stmt in &module.body {
        let Stmt::ClassDef(class) = stmt else {
            continue;
        };
        let mut attrs: Vec<(String, String)> = Vec::new();
        for member in &class.body {
            if let Some((name, value)) = string_attribute(member) {
                if !attrs.iter().any(|(n, _)| *n == name) {
                    attrs.push((name, value));
                }
            }
        }
        if !attrs.is_empty() {
            by_class
                .entry(class.name.to_string())
                .or_insert_with(|| attrs);
        }
    }
    #[allow(
        clippy::iter_over_hash_type,
        reason = "iteration order cannot reach the result: the emitted keys are `{class}.{attr}`, unique across the whole fold, so `or_insert_with` never resolves a collision"
    )]
    for (class_name, attrs) in &by_class {
        for (attr, value) in attrs {
            out.entry(format!("{class_name}.{attr}"))
                .or_insert_with(|| value.clone());
        }
    }

    // Pass 2: `name = ClassName()` at module level re-keys that class's attributes under the instance
    // name — the spelling every consumer actually writes.
    for stmt in &module.body {
        let Stmt::Assign(assign) = stmt else {
            continue;
        };
        let [Expr::Name(target)] = assign.targets.as_slice() else {
            continue;
        };
        let Expr::Call(call) = assign.value.as_ref() else {
            continue;
        };
        let Expr::Name(callee) = call.func.as_ref() else {
            continue;
        };
        let Some(attrs) = by_class.get(callee.id.as_str()) else {
            continue;
        };
        for (attr, value) in attrs {
            out.entry(format!("{}.{attr}", target.id))
                .or_insert_with(|| value.clone());
        }
    }

    out
}

/// A class-body statement's `(attribute name, string value)` when it binds a plain string literal, in
/// either the annotated (`X: str = "v"`) or bare (`X = "v"`) spelling. `None` for everything else.
fn string_attribute(stmt: &Stmt) -> Option<(String, String)> {
    let (target, value) = match stmt {
        Stmt::AnnAssign(a) => (a.target.as_ref(), a.value.as_deref()?),
        Stmt::Assign(a) => {
            let [one] = a.targets.as_slice() else {
                return None;
            };
            (one, a.value.as_ref())
        }
        _ => return None,
    };
    let Expr::Name(name) = target else {
        return None;
    };
    // `Expr::StringLiteral` only — an f-string is a different node even when it interpolates nothing,
    // and its value is not knowable here.
    let Expr::StringLiteral(s) = value else {
        return None;
    };
    Some((name.id.to_string(), s.value.to_str().to_string()))
}

#[cfg(test)]
mod tests;
