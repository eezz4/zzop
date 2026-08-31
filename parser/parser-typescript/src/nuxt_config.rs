//! `nuxt.config.*`'s `imports.dirs` — the DECLARATION that turns an ordinary directory into an
//! auto-import directory.
//!
//! ## Why a declaration read rather than a wider convention list
//! Nuxt auto-imports `composables/` and `utils/` because the framework says so; a tree gets `store/`,
//! `helpers/` or `lib/` auto-imported only because its own `nuxt.config.*` names them. Treating the
//! second group as a convention would be a guess dressed as a rule — measured on nocodb 3a5cbd5, that
//! one line (`packages/nc-gui/nuxt.config.ts:391`) is the sole reason 34 files under
//! `store`/`helpers`/`lib` are reachable at all, and no other file in the tree states it.
//!
//! ## Scope
//! Only the array's STRING elements, only under `imports` -> `dirs`, and only where the object literal
//! is the module's default export (`export default { … }` or `export default defineNuxtConfig({ … })`,
//! the two spellings Nuxt documents). A computed entry, a spread, or a config assembled in a variable
//! yields nothing rather than a guess — under-reading leaves a false `dead-candidates` finding, which
//! is the failure this whole path is trying to make rarer, while over-reading silences live-looking
//! files no build ever auto-imported.

use swc_core::ecma::ast::{
    Expr, Lit, Module, ModuleDecl, ModuleItem, ObjectLit, Prop, PropName, PropOrSpread,
};

use crate::parse_module;

/// The `imports.dirs` entries declared by `source`, verbatim and in source order (`"./utils/**"` stays
/// `"./utils/**"` — normalizing a path is the caller's business, not this walk's). Empty for a file
/// that declares none, and for an unparseable one — the same graceful degrade this crate's other
/// whole-file entrypoints perform.
pub fn parse_nuxt_imports_dirs(file: &str, source: &str) -> Vec<String> {
    let Some(module) = parse_module(file, source) else {
        return Vec::new();
    };
    let Some(config) = default_exported_object(&module) else {
        return Vec::new();
    };
    let Some(Expr::Object(imports)) = prop_value(config, "imports") else {
        return Vec::new();
    };
    let Some(Expr::Array(dirs)) = prop_value(imports, "dirs") else {
        return Vec::new();
    };
    dirs.elems
        .iter()
        .flatten()
        .filter_map(|e| match &*e.expr {
            Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
            _ => None,
        })
        .collect()
}

/// The object literal a module default-exports, unwrapping ONE call layer so
/// `export default defineNuxtConfig({ … })` reads the same as `export default { … }`.
fn default_exported_object(module: &Module) -> Option<&ObjectLit> {
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(e)) = item else {
            continue;
        };
        return match &*e.expr {
            Expr::Object(o) => Some(o),
            Expr::Call(call) => call.args.first().and_then(|a| match &*a.expr {
                Expr::Object(o) => Some(o),
                _ => None,
            }),
            _ => None,
        };
    }
    None
}

/// The value of `obj`'s `key` property. Both the bare (`imports:`) and quoted (`"imports":`) spellings
/// count; a shorthand, method, getter or spread has no literal value here and is skipped.
fn prop_value<'a>(obj: &'a ObjectLit, key: &str) -> Option<&'a Expr> {
    obj.props.iter().find_map(|p| {
        let PropOrSpread::Prop(prop) = p else {
            return None;
        };
        let Prop::KeyValue(kv) = &**prop else {
            return None;
        };
        let matches = match &kv.key {
            PropName::Ident(i) => i.sym == *key,
            PropName::Str(s) => s.value == *key,
            _ => false,
        };
        matches.then_some(&*kv.value)
    })
}

#[cfg(test)]
mod tests;
