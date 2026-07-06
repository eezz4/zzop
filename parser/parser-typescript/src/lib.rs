//! zzop-parser-typescript — native swc TS parser -> Common IR projection (0 N-API crossings). swc types
//! stay inside this crate (an swc upgrade should never leak into the public IR); only zzop-core types are
//! exposed.
//!
//! ## 2-layer layout
//! - `lang` — swc -> Common-IR LANGUAGE projection: call-graph construction (`calls`) and dependency-path
//!   resolution (`resolve`). Symbol/import extraction stays in this file since both `lang` and `adapters`
//!   depend on it.
//! - `adapters` — framework-vocabulary producers emitting `IoConsume`/`IoProvide`/fragment IR (controller
//!   decorators, FE HTTP-call egress, tRPC routers/proxy clients, Next.js `pages/api` handlers,
//!   Hono-style router mounts).

pub mod adapters;
pub mod lang;

pub use adapters::controller_decorators::{
    extract_controller_guarded_lines, extract_controller_provides,
};
pub use adapters::db_table_consume::{
    extract_db_table_consumes, extract_query_call_sites, PRISMA_CLIENT_GETTER,
};
pub use adapters::egress::{
    const_map_fragment, extract_http_egress, is_external_url, resolve_raw_path,
};
pub use adapters::hono_client::extract_hono_client_consumes;
pub use adapters::next_pages_api::{scan_pages_api_handler, PagesApiHandlerScan};
pub use adapters::router_mounts::extract_router_mount_fragments;
pub use adapters::store_binding::extract_store_bound_models;
pub use adapters::trpc_consume::extract_trpc_consumes;
pub use adapters::trpc_router::extract_trpc_router_fragments;
pub use adapters::wrapper_calls::extract_wrapper_fragments;
pub use lang::calls::parse_calls;
pub use lang::resolve::{
    build_dep, build_dep_with_workspace, resolve_file, resolve_file_with_workspace, try_ext,
    TsconfigPaths, WorkspacePkg, RESOLVE_EXTS,
};
pub use lang::write_site::{
    write_sites_for_symbol, DEFAULT_ORM_RECEIVER_PATTERN, DEFAULT_WRITE_METHODS,
};

/// Cache key ingredient for `zzop-cache`: parser id + pinned swc version + a logic-version counter, so an
/// swc upgrade or a change in this crate's projected IR shape invalidates stale cached entries. The
/// `swc_core-71.0.5` segment must match this crate's `Cargo.toml` pin exactly (TODO Phase 2: derive it
/// from the pin automatically instead of hand-syncing). Each `+name-vN` suffix marks a projection-shape
/// change — new IO kind, new fragment type, or a changed field on an existing one — that a cache entry
/// from before that marker would not reflect and must not be served as fresh:
/// - `v3` -> `v4`: NestJS-style `@Controller`/`@Get`/... route PROVIDES extraction.
/// - `late-resolve-v1`: `IoConsume::method` now set on unresolved consumes; added the late cross-file
///   constant re-resolution substrate (`const_map_fragment`/`resolve_raw_path`).
/// - `oazapfts-v1`: recognizes the oazapfts-generated-SDK call family in HTTP egress.
/// - `trpc-v1`: tRPC consume extraction plus per-file tRPC router fragments.
/// - `router-mounts-v1`: code-registered router-mount fragments (Hono-style), replacing the old
///   line-based route extractor — sees chained builders and cross-file mounts it couldn't before.
/// - `wrapper-calls-v1`: FE HTTP-call wrapper fragments, re-anchoring consumes from a wrapper's
///   internals to its real cross-file call sites.
/// - `hono-client-v1`: Hono's typed `hc<AppType>()` proxy-client call shape as HTTP consumes.
/// - `router-mounts-v2`: router-mount fragments gain an Express vocabulary alongside Hono; the
///   fragment shape and engine-side compose pass are unchanged, only the recognizer's vocabulary grew.
/// - `query-call-sites-v1`: `extract_query_call_sites` — per-file `zzop_core::QueryCallSite` facts for
///   the schema x usage JOIN rules, replacing `zzop_rules_schema::join`'s own filesystem re-walk.
/// - `store-binding-v1`: `extract_store_bound_models` — per-file store-binding model names for the
///   `schema-usage` native rule's `dead-model` check, replacing `zzop_rules_schema::usage::scan_store_map`'s
///   own `<root>/src/domains/**` filesystem re-walk.
/// - `write-sites-v1`: `SourceSymbol::write_sites` — per-symbol store-write site detection, computed once
///   here instead of `zzop_rules_graph::http_scan` re-scanning each BFS-reached symbol's raw text on every
///   analysis run.
/// - `reexport-edges-v1`: `FileArtifact`/`FileIrSlice` now carry each file's `parse_re_exports` output
///   (specifier + `type_only`) so `lang::resolve::build_dep`/`build_dep_with_workspace` can merge
///   non-type-only re-export specifiers into the dep graph as real edges — a barrel file's re-exports
///   used to be invisible to `dep`, undercounting fan-in and false-positiving `dead-candidates`.
pub const PARSER_FINGERPRINT: &str = "typescript/swc_core-71.0.5/v4+late-resolve-v1+oazapfts-v1+trpc-v1+router-mounts-v1+wrapper-calls-v1+hono-client-v1+router-mounts-v2+db-table-consume-v1+query-call-sites-v1+store-binding-v1+write-sites-v1+reexport-edges-v1";

use std::collections::{HashMap, HashSet};

use swc_core::common::{sync::Lrc, BytePos, FileName, Globals, SourceMap, Spanned, GLOBALS};
use swc_core::ecma::ast::{
    ArrowExpr, AssignExpr, AssignOp, AssignTarget, BlockStmtOrExpr, CallExpr, Callee, Class,
    ClassMember, ClassMethod, Constructor, Decl, DefaultDecl, EsVersion, ExportSpecifier, Expr,
    FnDecl, FnExpr, Function, GetterProp, Ident, ImportDecl, ImportSpecifier, Lit, MemberExpr,
    MemberProp, MethodProp, Module, ModuleDecl, ModuleExportName, ModuleItem, NamedExport,
    ObjectLit, ObjectPatProp, Pat, PrivateMethod, Prop, PropName, PropOrSpread, SetterProp,
    SimpleAssignTarget, Stmt, TsEnumDecl, TsEnumMember, TsInterfaceDecl, TsTypeAliasDecl,
    VarDeclarator,
};
use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{
    CommonIr, ImportBinding, ImportMap, IoFacts, MinimalIr, ReExport, SourceSymbol,
    SourceSymbolKind,
};

/// ModuleExportName -> name string (Ident or Str).
fn export_name(n: &ModuleExportName) -> String {
    match n {
        ModuleExportName::Ident(id) => id.sym.to_string(),
        ModuleExportName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
    }
}

/// import declarations -> `{ localName -> ImportBinding }`. Specifiers are verbatim; path resolution is
/// the caller's responsibility. Also collects CommonJS `require("literal")` bindings (top-level +
/// function-body-nested) via a tree walk, so dep-graph / circular / call resolution work on CJS trees too.
pub fn parse_imports(file: &str, source: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(module) = parse_module(file, source) else {
        return map;
    };
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else {
            continue;
        };
        let specifier = import.src.value.as_str().unwrap_or_default().to_string();
        let clause_type_only = import.type_only;
        for spec in &import.specifiers {
            match spec {
                ImportSpecifier::Named(n) => {
                    let local = n.local.sym.to_string();
                    let original = n
                        .imported
                        .as_ref()
                        .map_or_else(|| local.clone(), export_name);
                    map.insert(
                        local,
                        ImportBinding {
                            specifier: specifier.clone(),
                            original,
                            deferred: false,
                            type_only: clause_type_only || n.is_type_only,
                        },
                    );
                }
                ImportSpecifier::Default(d) => {
                    map.insert(
                        d.local.sym.to_string(),
                        ImportBinding {
                            specifier: specifier.clone(),
                            original: "default".into(),
                            deferred: false,
                            type_only: clause_type_only,
                        },
                    );
                }
                ImportSpecifier::Namespace(ns) => {
                    map.insert(
                        ns.local.sym.to_string(),
                        ImportBinding {
                            specifier: specifier.clone(),
                            original: "*".into(),
                            deferred: false,
                            type_only: clause_type_only,
                        },
                    );
                }
            }
        }
    }
    let mut requires = RequireCollector {
        map: &mut map,
        deferred: false,
        side_effect_seq: 0,
    };
    module.visit_with(&mut requires);
    map
}

/// Walks the whole tree collecting CommonJS `require("literal")` bindings.
/// - `const X = require("./y")` -> X bound as a namespace ("*") of the module.
/// - `const { a, b: c } = require("./y")` -> a / c bound to their original export names.
/// - inline `require("./y").foo()` / bare `require("./y")` -> a synthetic key so the edge still enters the graph.
///
/// `deferred` tracks whether the require sits inside a function/method/accessor body (lazy — no load-order edge).
struct RequireCollector<'a> {
    map: &'a mut ImportMap,
    deferred: bool,
    side_effect_seq: u32,
}

impl RequireCollector<'_> {
    /// Runs `f` with `deferred` forced true for its duration — used when descending into a function-like scope.
    fn with_deferred(&mut self, f: impl FnOnce(&mut Self)) {
        let prev = self.deferred;
        self.deferred = true;
        f(self);
        self.deferred = prev;
    }
}

impl Visit for RequireCollector<'_> {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let Some(Expr::Call(call)) = d.init.as_deref() {
            if let Some(specifier) = require_specifier(call) {
                bind_require_target(&d.name, &specifier, self.deferred, self.map);
                return; // fully handled — require("literal")'s own subtree has nothing further of interest
            }
        }
        d.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(specifier) = require_specifier(call) {
            let key = format!("__require{}__", self.side_effect_seq);
            self.side_effect_seq += 1;
            self.map.insert(
                key,
                ImportBinding {
                    specifier,
                    original: "*".into(),
                    deferred: self.deferred,
                    type_only: false,
                },
            );
        }
        call.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, n: &FnDecl) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_fn_expr(&mut self, n: &FnExpr) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_arrow_expr(&mut self, n: &ArrowExpr) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_class_method(&mut self, n: &ClassMethod) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_private_method(&mut self, n: &PrivateMethod) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_constructor(&mut self, n: &Constructor) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_getter_prop(&mut self, n: &GetterProp) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_setter_prop(&mut self, n: &SetterProp) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_method_prop(&mut self, n: &MethodProp) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
}

/// The literal specifier of a `require("literal")` call, or None.
fn require_specifier(call: &CallExpr) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(id) = &**callee else {
        return None;
    };
    if id.sym != "require" {
        return None;
    }
    let arg = call.args.first()?;
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// Binds the LHS of a `... = require(spec)` declarator — identifier as namespace, or destructured names (array-destructured requires are not handled).
fn bind_require_target(name: &Pat, specifier: &str, deferred: bool, map: &mut ImportMap) {
    match name {
        Pat::Ident(bi) => {
            map.insert(
                bi.id.sym.to_string(),
                ImportBinding {
                    specifier: specifier.into(),
                    original: "*".into(),
                    deferred,
                    type_only: false,
                },
            );
        }
        Pat::Object(obj) => {
            for prop in &obj.props {
                match prop {
                    // `{ a }` — shorthand: local name doubles as the original export name.
                    ObjectPatProp::Assign(a) => {
                        let local = a.key.id.sym.to_string();
                        map.insert(
                            local.clone(),
                            ImportBinding {
                                specifier: specifier.into(),
                                original: local,
                                deferred,
                                type_only: false,
                            },
                        );
                    }
                    // `{ b: c }` — c is the local name, b is the original export name.
                    ObjectPatProp::KeyValue(kv) => {
                        let Pat::Ident(local) = &*kv.value else {
                            continue;
                        };
                        let PropName::Ident(original) = &kv.key else {
                            continue;
                        };
                        map.insert(
                            local.id.sym.to_string(),
                            ImportBinding {
                                specifier: specifier.into(),
                                original: original.sym.to_string(),
                                deferred,
                                type_only: false,
                            },
                        );
                    }
                    ObjectPatProp::Rest(_) => {}
                }
            }
        }
        _ => {}
    }
}

/// Extracts only `export { A as B } from "./y"` / `export * from "./y"` / `export * as ns from "./y"` — a
/// bare `export { x }` with no from-clause is a local declaration and is excluded. `type_only` is set from
/// the export clause's own `type_only`/`export type * from` flag AND, per named specifier, its own
/// per-specifier `export { type X } from "./y"` marker (mirrors `parse_imports`'s
/// `clause_type_only || n.is_type_only` combination) — a type-only re-export is erased by TS at compile
/// time and must not become a dep-graph edge (`lang::resolve::build_dep`/`build_dep_with_workspace` skip
/// it entirely rather than merging it into `resolved`).
pub fn parse_re_exports(file: &str, source: &str) -> Vec<ReExport> {
    let mut out = Vec::new();
    let Some(module) = parse_module(file, source) else {
        return out;
    };
    for item in &module.body {
        let ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };
        match decl {
            // `export * from "..."` / `export type * from "..."`
            ModuleDecl::ExportAll(all) => out.push(ReExport {
                specifier: all.src.value.as_str().unwrap_or_default().to_string(),
                original: "*".into(),
                local_alias: "*".into(),
                type_only: all.type_only,
            }),
            // `export { ... } from "..."` / `export * as ns from "..."` / `export type { ... } from "..."`
            ModuleDecl::ExportNamed(named) => {
                let Some(src) = &named.src else {
                    continue; // no from-clause -> local export, not a re-export
                };
                let specifier = src.value.as_str().unwrap_or_default().to_string();
                for spec in &named.specifiers {
                    match spec {
                        ExportSpecifier::Named(n) => {
                            let original = export_name(&n.orig);
                            let local_alias = n
                                .exported
                                .as_ref()
                                .map_or_else(|| original.clone(), export_name);
                            out.push(ReExport {
                                specifier: specifier.clone(),
                                original,
                                local_alias,
                                type_only: named.type_only || n.is_type_only,
                            });
                        }
                        ExportSpecifier::Namespace(ns) => out.push(ReExport {
                            specifier: specifier.clone(),
                            original: "*".into(),
                            local_alias: export_name(&ns.name),
                            type_only: named.type_only,
                        }),
                        ExportSpecifier::Default(_) => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Extracts dynamic `import("./x")` / `await import("./x")` specifiers (recursive walk); which exports run at runtime is unknown, so the whole target file is treated as a wildcard.
pub fn parse_dynamic_imports(file: &str, source: &str) -> Vec<String> {
    let Some(module) = parse_module(file, source) else {
        return Vec::new();
    };
    let mut collector = DynImportCollector { out: Vec::new() };
    module.visit_with(&mut collector);
    collector.out
}

struct DynImportCollector {
    out: Vec<String>,
}

impl Visit for DynImportCollector {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if matches!(call.callee, Callee::Import(_)) {
            if let Some(first) = call.args.first() {
                if let Expr::Lit(Lit::Str(s)) = &*first.expr {
                    self.out
                        .push(s.value.as_str().unwrap_or_default().to_string());
                }
            }
        }
        call.visit_children_with(self); // recurse into nested calls (lazy(() => import()))
    }
}

/// Collects all identifier references in a file, excluding import/export declarations, member-access
/// property names, and declaration names — used by dead-export analysis to find symbols that are
/// exported but only referenced within the same file. Each name appears once; scope shadowing is ignored
/// (names are compared statically only).
/// Property-like names are excluded for free (swc types them `IdentName`/`PropName`, not `Ident`, so they
/// never reach `visit_ident`); binding names are excluded via `visit_binding_ident`. Destructuring-
/// *assignment* targets (`[a, b] = arr;`) reuse the declaration `Pat` shape and are excluded too, even
/// though they're plain references rather than bindings — untested either way.
pub fn parse_local_identifier_refs(file: &str, source: &str) -> std::collections::BTreeSet<String> {
    let Some(module) = parse_module(file, source) else {
        return std::collections::BTreeSet::new();
    };
    let mut collector = LocalRefCollector {
        refs: std::collections::BTreeSet::new(),
    };
    module.visit_with(&mut collector);
    collector.refs
}

struct LocalRefCollector {
    refs: std::collections::BTreeSet<String>,
}

impl Visit for LocalRefCollector {
    fn visit_ident(&mut self, node: &Ident) {
        self.refs.insert(node.sym.to_string());
    }

    // Import/export declarations bind names — not references.
    fn visit_import_decl(&mut self, _n: &ImportDecl) {}
    fn visit_named_export(&mut self, _n: &NamedExport) {}

    // `id` here is the declared binding name (var/param/destructuring) — not a reference. Type annotations
    // may themselves reference other identifiers/types, so those are still visited.
    fn visit_binding_ident(&mut self, n: &swc_core::ecma::ast::BindingIdent) {
        if let Some(ann) = &n.type_ann {
            ann.visit_with(self);
        }
    }

    fn visit_fn_decl(&mut self, n: &FnDecl) {
        n.function.visit_with(self); // skip n.ident (declaration name)
    }
    fn visit_class_decl(&mut self, n: &swc_core::ecma::ast::ClassDecl) {
        n.class.visit_with(self); // skip n.ident (declaration name)
    }
    fn visit_ts_interface_decl(&mut self, n: &TsInterfaceDecl) {
        if let Some(tp) = &n.type_params {
            tp.visit_with(self);
        }
        n.extends.visit_with(self);
        n.body.visit_with(self); // skip n.id (declaration name)
    }
    fn visit_ts_type_alias_decl(&mut self, n: &TsTypeAliasDecl) {
        if let Some(tp) = &n.type_params {
            tp.visit_with(self);
        }
        n.type_ann.visit_with(self); // skip n.id (declaration name)
    }
    fn visit_ts_enum_decl(&mut self, n: &TsEnumDecl) {
        n.members.visit_with(self); // skip n.id (declaration name)
    }
    fn visit_ts_enum_member(&mut self, n: &TsEnumMember) {
        if let Some(init) = &n.init {
            init.visit_with(self); // skip n.id (declaration name / string literal)
        }
    }
    fn visit_prop(&mut self, n: &Prop) {
        match n {
            // `{ x }` — shorthand property name; excluded (not a name -> value reference).
            Prop::Shorthand(_) => {}
            other => other.visit_children_with(self),
        }
    }
}

/// Top-level object literal consts (`const X = {...}`, incl. `export const X = {...}`) keyed by name — feeds factory spread flattening.
type ObjectLitMap = HashMap<String, ObjectLit>;

fn collect_top_level_object_lits(module: &Module) -> ObjectLitMap {
    let mut map = ObjectLitMap::new();
    for item in &module.body {
        let decls: &[VarDeclarator] = match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) => &v.decls,
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => match &e.decl {
                Decl::Var(v) => &v.decls,
                _ => continue,
            },
            _ => continue,
        };
        for d in decls {
            if let Pat::Ident(bi) = &d.name {
                if let Some(Expr::Object(obj)) = d.init.as_deref() {
                    map.insert(bi.id.sym.to_string(), obj.clone());
                }
            }
        }
    }
    map
}

/// Top-level declarations -> `SourceSymbol[]`: function/class/interface/type/const + `export default`
/// fn/class, factory sub-symbols, binding-pattern consts, and CommonJS exports. Declaration order preserved.
pub fn parse_symbols(file: &str, source: &str) -> Vec<SourceSymbol> {
    let Some((cm, module)) = parse_with_cm(file, source) else {
        return Vec::new();
    };
    let object_lits_by_name = collect_top_level_object_lits(&module);
    let mut out = Vec::new();
    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(decl)) => emit_decl(
                &cm,
                file,
                decl,
                false,
                false,
                &object_lits_by_name,
                &mut out,
            ),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => emit_decl(
                &cm,
                file,
                &e.decl,
                true,
                false,
                &object_lits_by_name,
                &mut out,
            ),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(e)) => match &e.decl {
                DefaultDecl::Fn(fe) => {
                    let name = fe
                        .ident
                        .as_ref()
                        .map_or_else(|| "default".into(), |i| i.sym.to_string());
                    out.push(fn_symbol(&cm, file, name.clone(), &fe.function, true, true));
                    extract_factory_methods(
                        &cm,
                        file,
                        &name,
                        &fe.function,
                        &object_lits_by_name,
                        &mut out,
                    );
                }
                DefaultDecl::Class(ce) => {
                    let name = ce
                        .ident
                        .as_ref()
                        .map_or_else(|| "default".into(), |i| i.sym.to_string());
                    emit_class(&cm, file, name, &ce.class, true, true, &mut out);
                }
                DefaultDecl::TsInterfaceDecl(_) => {}
            },
            _ => {}
        }
    }
    // CommonJS exports (module.exports / exports.x = ...) — additive; names already emitted above win (deduped here).
    let declared: HashSet<String> = out.iter().map(|s| s.name.clone()).collect();
    for cjs in collect_common_js_exports(&cm, file, &module) {
        if !declared.contains(&cjs.name) {
            out.push(cjs);
        }
    }
    // Write-site detection is a pure function of (this symbol's own body span, constant vocab), so it
    // runs as a final pass over the fully-built list rather than being threaded through every symbol
    // constructor above.
    for sym in &mut out {
        sym.write_sites = lang::write_site::write_sites_for_symbol(sym, source);
    }
    out
}

fn emit_decl(
    cm: &SourceMap,
    file: &str,
    decl: &Decl,
    exported: bool,
    is_default: bool,
    object_lits_by_name: &ObjectLitMap,
    out: &mut Vec<SourceSymbol>,
) {
    match decl {
        Decl::Fn(f) => {
            let name = f.ident.sym.to_string();
            out.push(fn_symbol(
                cm,
                file,
                name.clone(),
                &f.function,
                exported,
                is_default,
            ));
            extract_factory_methods(cm, file, &name, &f.function, object_lits_by_name, out);
        }
        Decl::Class(c) => emit_class(
            cm,
            file,
            c.ident.sym.to_string(),
            &c.class,
            exported,
            is_default,
            out,
        ),
        Decl::TsInterface(i) => out.push(simple_symbol(
            cm,
            file,
            i.id.sym.to_string(),
            SourceSymbolKind::Interface,
            i.span.lo,
            exported,
        )),
        Decl::TsTypeAlias(t) => out.push(simple_symbol(
            cm,
            file,
            t.id.sym.to_string(),
            SourceSymbolKind::Type,
            t.span.lo,
            exported,
        )),
        Decl::Var(v) => {
            for d in &v.decls {
                emit_var_declarator(cm, file, d, exported, object_lits_by_name, out);
            }
        }
        _ => {}
    }
}

fn emit_var_declarator(
    cm: &SourceMap,
    file: &str,
    d: &VarDeclarator,
    exported: bool,
    object_lits_by_name: &ObjectLitMap,
    out: &mut Vec<SourceSymbol>,
) {
    match &d.name {
        Pat::Ident(bi) => {
            // `var X = require('...')` is an import alias (owned by parseImports) -> skip.
            if is_require_init(d) {
                return;
            }
            let name = bi.id.sym.to_string();
            let line = line_of(cm, d.span.lo);
            let fn_span = match d.init.as_deref() {
                Some(Expr::Arrow(a)) => Some(a.span),
                Some(Expr::Fn(f)) => Some(f.function.span),
                _ => None,
            };
            let (kind, body_start, body_end) = match fn_span {
                Some(sp) => (
                    SourceSymbolKind::Function,
                    Some(line_of(cm, sp.lo)),
                    Some(line_of(cm, sp.hi)),
                ),
                None => (SourceSymbolKind::Const, None, None),
            };
            out.push(SourceSymbol {
                id: format!("{file}#{name}"),
                file: file.into(),
                name: name.clone(),
                kind,
                line,
                exported,
                is_default: false,
                body_start,
                body_end,
                write_sites: Vec::new(),
            });
            // Factory: `const api = { m: () => {} }` -> api.m sub-symbols.
            if let Some(Expr::Object(obj)) = d.init.as_deref() {
                extract_object_methods(
                    cm,
                    file,
                    &name,
                    obj,
                    object_lits_by_name,
                    &mut HashSet::new(),
                    out,
                );
            }
        }
        Pat::Object(_) | Pat::Array(_) => {
            // `const { a, b } = ...` / `const [x] = ...` -> one const symbol per binding.
            let line = line_of(cm, d.span.lo);
            for name in collect_binding_names(&d.name) {
                out.push(SourceSymbol {
                    id: format!("{file}#{name}"),
                    file: file.into(),
                    name,
                    kind: SourceSymbolKind::Const,
                    line,
                    exported,
                    is_default: false,
                    body_start: None,
                    body_end: None,
                    write_sites: Vec::new(),
                });
            }
        }
        _ => {}
    }
}

fn fn_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    function: &Function,
    exported: bool,
    is_default: bool,
) -> SourceSymbol {
    let (body_start, body_end) = match &function.body {
        Some(b) => (Some(line_of(cm, b.span.lo)), Some(line_of(cm, b.span.hi))),
        None => (None, None),
    };
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind: SourceSymbolKind::Function,
        line: line_of(cm, function.span.lo),
        exported,
        is_default,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

fn class_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    class: &Class,
    exported: bool,
    is_default: bool,
) -> SourceSymbol {
    let line = line_of(cm, class.span.lo);
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind: SourceSymbolKind::Class,
        line,
        exported,
        is_default,
        body_start: Some(line), // class bodyStart uses the node's own start line
        body_end: Some(line_of(cm, class.span.hi)),
        write_sites: Vec::new(),
    }
}

/// Class symbol + method sub-symbols (`Class.method`) — constructor/method/getter/setter/private-method
/// only, properties/computed/string-literal names skipped. Same-name pairs (e.g. get/set) emit once.
fn emit_class(
    cm: &SourceMap,
    file: &str,
    name: String,
    class: &Class,
    exported: bool,
    is_default: bool,
    out: &mut Vec<SourceSymbol>,
) {
    out.push(class_symbol(
        cm,
        file,
        name.clone(),
        class,
        exported,
        is_default,
    ));
    let mut seen = std::collections::HashSet::new();
    for member in &class.body {
        let (mname, lo, body_span) = match member {
            ClassMember::Constructor(c) => (
                "constructor".to_string(),
                c.span.lo,
                c.body.as_ref().map(|b| b.span),
            ),
            ClassMember::Method(m) => {
                let Some(n) = prop_name(&m.key) else { continue };
                (n, m.span.lo, m.function.body.as_ref().map(|b| b.span))
            }
            ClassMember::PrivateMethod(m) => (
                format!("#{}", m.key.name),
                m.span.lo,
                m.function.body.as_ref().map(|b| b.span),
            ),
            _ => continue, // properties / index signatures / etc.
        };
        if !seen.insert(mname.clone()) {
            continue;
        }
        let full = format!("{name}.{mname}");
        out.push(SourceSymbol {
            id: format!("{file}#{full}"),
            file: file.into(),
            name: full,
            kind: SourceSymbolKind::Function,
            line: line_of(cm, lo),
            exported: false,
            is_default: false,
            body_start: body_span.map(|s| line_of(cm, s.lo)),
            body_end: body_span.map(|s| line_of(cm, s.hi)),
            write_sites: Vec::new(),
        });
    }
}

/// PropName -> static name (Ident only; computed/string/num are not statically extractable -> None).
fn prop_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        _ => None,
    }
}

fn simple_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    kind: SourceSymbolKind,
    lo: BytePos,
    exported: bool,
) -> SourceSymbol {
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind,
        line: line_of(cm, lo),
        exported,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

/// A `require('...')` / `require('...').x` initializer — a CJS import alias (not a declared symbol).
fn is_require_init(d: &VarDeclarator) -> bool {
    let Some(e) = d.init.as_deref() else {
        return false;
    };
    let e = if let Expr::Member(m) = e { &*m.obj } else { e };
    let Expr::Call(c) = e else {
        return false;
    };
    let Callee::Expr(callee) = &c.callee else {
        return false;
    };
    let Expr::Ident(id) = &**callee else {
        return false;
    };
    id.sym == "require"
        && c.args
            .first()
            .is_some_and(|a| matches!(&*a.expr, Expr::Lit(Lit::Str(_))))
}

fn line_of(cm: &SourceMap, pos: BytePos) -> u32 {
    cm.lookup_char_pos(pos).line as u32
}

/// Flattens a binding pattern (`{a, b}` / `[x]`, incl. nested) into its bound identifier names, in source order (omitted array slots and rest elements' own patterns are handled).
fn collect_binding_names(pat: &Pat) -> Vec<String> {
    let mut names = Vec::new();
    collect_binding_names_into(pat, &mut names);
    names
}

fn collect_binding_names_into(pat: &Pat, out: &mut Vec<String>) {
    match pat {
        Pat::Ident(bi) => out.push(bi.id.sym.to_string()),
        Pat::Array(a) => {
            for elem in a.elems.iter().flatten() {
                collect_binding_names_into(elem, out);
            }
        }
        Pat::Object(o) => {
            for prop in &o.props {
                match prop {
                    ObjectPatProp::Assign(a) => out.push(a.key.id.sym.to_string()),
                    ObjectPatProp::KeyValue(kv) => collect_binding_names_into(&kv.value, out),
                    ObjectPatProp::Rest(r) => collect_binding_names_into(&r.arg, out),
                }
            }
        }
        Pat::Rest(r) => collect_binding_names_into(&r.arg, out),
        Pat::Assign(a) => collect_binding_names_into(&a.left, out),
        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

/// Extracts `fn.method` sub-symbols when a function body contains `return { ... }`; spread (`...other`) is flattened up to 2 hops when `other` is a same-file top-level const ObjectLit.
fn extract_factory_methods(
    cm: &SourceMap,
    file: &str,
    fn_name: &str,
    function: &Function,
    object_lits_by_name: &ObjectLitMap,
    out: &mut Vec<SourceSymbol>,
) {
    let Some(body) = &function.body else {
        return;
    };
    for stmt in &body.stmts {
        let Stmt::Return(ret) = stmt else { continue };
        let Some(Expr::Object(obj)) = ret.arg.as_deref() else {
            continue;
        };
        extract_object_methods(
            cm,
            file,
            fn_name,
            obj,
            object_lits_by_name,
            &mut HashSet::new(),
            out,
        );
    }
}

/// Extracts object-literal `key: value` properties as `parent.key` sub-symbols (method-shorthand /
/// getter / setter / plain-shorthand members are skipped). `...other` spreads are flattened when `other`
/// resolves to a same-file top-level const ObjectLit; `visited` guards against spread cycles.
fn extract_object_methods(
    cm: &SourceMap,
    file: &str,
    parent: &str,
    obj: &ObjectLit,
    object_lits_by_name: &ObjectLitMap,
    visited: &mut HashSet<String>,
    out: &mut Vec<SourceSymbol>,
) {
    let mut seen_names: HashSet<String> = HashSet::new();
    let prefix = format!("{parent}.");
    for prop in &obj.props {
        match prop {
            PropOrSpread::Spread(sp) => {
                let Expr::Ident(id) = &*sp.expr else { continue };
                let target_name = id.sym.to_string();
                if visited.contains(&target_name) {
                    continue;
                }
                let Some(target) = object_lits_by_name.get(&target_name) else {
                    continue;
                };
                visited.insert(target_name);
                let mut inner = Vec::new();
                extract_object_methods(
                    cm,
                    file,
                    parent,
                    target,
                    object_lits_by_name,
                    visited,
                    &mut inner,
                );
                for sym in inner {
                    let base_name = sym
                        .name
                        .strip_prefix(&prefix)
                        .unwrap_or(&sym.name)
                        .to_string();
                    if seen_names.contains(&base_name) {
                        continue;
                    }
                    seen_names.insert(base_name);
                    out.push(sym);
                }
            }
            PropOrSpread::Prop(p) => {
                let Prop::KeyValue(kv) = &**p else { continue };
                let PropName::Ident(name_id) = &kv.key else {
                    continue;
                };
                let name = name_id.sym.to_string();
                if seen_names.contains(&name) {
                    continue;
                }
                seen_names.insert(name.clone());
                let (is_fn, body_start, body_end) = match &*kv.value {
                    Expr::Arrow(a) => (
                        true,
                        Some(line_of(cm, a.span.lo)),
                        Some(line_of(cm, a.span.hi)),
                    ),
                    Expr::Fn(f) => (
                        true,
                        Some(line_of(cm, f.function.span.lo)),
                        Some(line_of(cm, f.function.span.hi)),
                    ),
                    _ => (false, None, None),
                };
                let full = format!("{parent}.{name}");
                out.push(SourceSymbol {
                    id: format!("{file}#{full}"),
                    file: file.into(),
                    name: full,
                    kind: if is_fn {
                        SourceSymbolKind::Function
                    } else {
                        SourceSymbolKind::Const
                    },
                    line: line_of(cm, name_id.span.lo),
                    exported: false,
                    is_default: false,
                    body_start,
                    body_end,
                    write_sites: Vec::new(),
                });
            }
        }
    }
}

/// CommonJS export-symbol extraction — the counterpart to ESM `export` parsing for `module.exports` /
/// `exports.x`. Recovers exports from the common `var Body = {}; module.exports = Body;
/// Body.create = ...;` shape, named by their bare member name, so symbol risk/hotspots/cycles aren't empty for CJS files.
fn collect_common_js_exports(cm: &SourceMap, file: &str, module: &Module) -> Vec<SourceSymbol> {
    let mut names = ExportObjNameCollector {
        names: HashSet::new(),
    };
    module.visit_with(&mut names);
    let mut collector = CjsExportCollector {
        cm,
        file,
        export_obj_names: names.names,
        seen: HashSet::new(),
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

/// Identifiers assigned to `module.exports` (the module's export object) — e.g. `Body` in `module.exports = Body`.
struct ExportObjNameCollector {
    names: HashSet<String>,
}

impl Visit for ExportObjNameCollector {
    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if n.op == AssignOp::Assign {
            if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &n.left {
                if is_module_exports(m) {
                    if let Expr::Ident(rhs) = &*n.right {
                        self.names.insert(rhs.sym.to_string());
                    }
                }
            }
        }
        n.visit_children_with(self);
    }
}

struct CjsExportCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    export_obj_names: HashSet<String>,
    seen: HashSet<String>,
    out: Vec<SourceSymbol>,
}

impl CjsExportCollector<'_> {
    fn add(&mut self, name: String, line: u32, rhs: Option<&Expr>) {
        if name.is_empty() || self.seen.contains(&name) {
            return;
        }
        self.seen.insert(name.clone());
        self.out
            .push(build_member_symbol(self.cm, self.file, &name, line, rhs));
    }
}

impl Visit for CjsExportCollector<'_> {
    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if n.op == AssignOp::Assign {
            if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &n.left {
                // `module.exports = { ... }` — each property is an export.
                if is_module_exports(m) {
                    if let Expr::Object(obj) = &*n.right {
                        for (name, line, rhs) in object_literal_member_names(self.cm, obj) {
                            self.add(name, line, rhs);
                        }
                    }
                }
                if let Some(member) = export_member_name(m, &self.export_obj_names) {
                    self.add(member, line_of(self.cm, n.span.lo), Some(&n.right));
                }
            }
        }
        n.visit_children_with(self);
    }
}

/// `module.exports` property access.
fn is_module_exports(m: &MemberExpr) -> bool {
    matches!(&*m.obj, Expr::Ident(id) if id.sym == "module")
        && matches!(&m.prop, MemberProp::Ident(name) if name.sym == "exports")
}

/// The exported member name for an assignment LHS, or None when the LHS is not an export member. Handles `exports.x`, `module.exports.x`, and `<exportObj>.x`.
fn export_member_name(m: &MemberExpr, export_obj_names: &HashSet<String>) -> Option<String> {
    let MemberProp::Ident(name) = &m.prop else {
        return None;
    };
    match &*m.obj {
        Expr::Ident(recv) => {
            if recv.sym == "exports" || export_obj_names.contains(recv.sym.as_str()) {
                Some(name.sym.to_string())
            } else {
                None
            }
        }
        Expr::Member(inner) if is_module_exports(inner) => Some(name.sym.to_string()),
        _ => None,
    }
}

fn object_literal_member_names<'e>(
    cm: &SourceMap,
    obj: &'e ObjectLit,
) -> Vec<(String, u32, Option<&'e Expr>)> {
    let mut out = Vec::new();
    for p in &obj.props {
        let PropOrSpread::Prop(prop) = p else {
            continue;
        };
        match &**prop {
            Prop::KeyValue(kv) => {
                if let PropName::Ident(id) = &kv.key {
                    out.push((
                        id.sym.to_string(),
                        line_of(cm, id.span.lo),
                        Some(&*kv.value),
                    ));
                }
            }
            Prop::Shorthand(id) => out.push((id.sym.to_string(), line_of(cm, id.span.lo), None)),
            Prop::Method(mp) => {
                if let PropName::Ident(id) = &mp.key {
                    out.push((id.sym.to_string(), line_of(cm, id.span.lo), None));
                }
            }
            _ => {}
        }
    }
    out
}

fn build_member_symbol(
    cm: &SourceMap,
    file: &str,
    name: &str,
    line: u32,
    rhs: Option<&Expr>,
) -> SourceSymbol {
    let (is_fn, body_start, body_end) = match rhs {
        Some(Expr::Fn(f)) => (
            true,
            f.function.body.as_ref().map(|b| line_of(cm, b.span.lo)),
            f.function.body.as_ref().map(|b| line_of(cm, b.span.hi)),
        ),
        Some(Expr::Arrow(a)) => {
            let (lo, hi) = match &*a.body {
                BlockStmtOrExpr::BlockStmt(b) => (b.span.lo, b.span.hi),
                BlockStmtOrExpr::Expr(e) => (e.span().lo, e.span().hi),
            };
            (true, Some(line_of(cm, lo)), Some(line_of(cm, hi)))
        }
        _ => (false, None, None),
    };
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name: name.into(),
        kind: if is_fn {
            SourceSymbolKind::Function
        } else {
            SourceSymbolKind::Const
        },
        line,
        exported: true,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

/// Project a whole source tree into a `CommonIr` — the parser -> engine bridge. `files` = (rel-path, text) for every file in the tree; produces symbols + the resolved dep graph + loc.
pub fn build_common_ir(source_id: &str, files: &[(String, String)]) -> CommonIr {
    let all_paths: std::collections::HashSet<String> =
        files.iter().map(|(rel, _)| rel.clone()).collect();
    let mut symbols = Vec::new();
    let mut import_pairs = Vec::new();
    let mut re_export_pairs = Vec::new();
    let mut loc = std::collections::HashMap::new();
    for (rel, text) in files {
        symbols.extend(parse_symbols(rel, text));
        import_pairs.push((rel.clone(), parse_imports(rel, text)));
        re_export_pairs.push((rel.clone(), parse_re_exports(rel, text)));
        loc.insert(rel.clone(), count_loc(text));
    }
    // `build_dep`'s second return value (the ephemeral type-only-edge exclusion set) feeds circular
    // detection only; this whole-tree, non-incremental projection doesn't run `circular_from_dep` itself
    // (that's an engine-side whole-graph pass), so it's discarded here.
    let (dep, _type_only_edges) =
        lang::resolve::build_dep(&import_pairs, &re_export_pairs, &all_paths);
    // Project the IO this tree consumes (HTTP egress) so the cross-layer linker can join it to BE providers.
    let consumes = adapters::egress::extract_http_egress(files);
    let io = if consumes.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides: Vec::new(),
            consumes,
        })
    };
    CommonIr {
        source: source_id.to_string(),
        parser: "typescript".to_string(),
        ir: MinimalIr {
            dep,
            symbols,
            loc,
            io,
        },
    }
}

/// Raw physical line count — the Rust equivalent of JS `content.split("\n").length`. Blank/comment-only
/// lines and lines inside block comments or multi-line strings all count; the file is never parsed, just
/// counted. A trailing newline adds 1 (`"a\nb\n".split("\n")` -> length 3) — use `str::split('\n').count()`, not `str::lines()`, which drops that trailing piece.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

fn parse_module(file: &str, source: &str) -> Option<Module> {
    parse_with_cm(file, source).map(|(_, m)| m)
}

fn parse_with_cm(file: &str, source: &str) -> Option<(Lrc<SourceMap>, Module)> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom(file.to_string())),
        source.to_string(),
    );
    let syntax = Syntax::Typescript(TsSyntax {
        tsx: file.ends_with(".tsx"),
        // Legacy/stage-2 decorator syntax (`@Component({...}) class Foo { @Input() x; }`) is
        // ubiquitous in real-world TS (Angular, NestJS, TypeORM, etc.), but swc_ecma_parser's
        // `TsSyntax::decorators` defaults to `false`. Without this, a decorated class fails to
        // parse at all (`parse_file_as_module` returns `Err`) and the caller degrades the whole file.
        decorators: true,
        ..Default::default()
    });
    let module = GLOBALS.set(&Globals::new(), || {
        let mut errors = Vec::new();
        parse_file_as_module(&fm, syntax, EsVersion::EsNext, None, &mut errors).ok()
    })?;
    Some((cm, module))
}

/// Distinguishes "parse failed" from "legitimately empty file" — a signal `parse_symbols`/`parse_imports`
/// can't give on their own, since both cases produce an empty `Vec`/`ImportMap`. Unlike TypeScript's own
/// error-tolerant parser, swc's `parse_file_as_module` returns `Err` (no `Module`) for malformed input
/// (unbalanced braces, a stray closing brace, plain syntax errors) while still parsing an empty file, a
/// comment-only file, or a merely *semantic* oddity like duplicate function declarations — see
/// `probe_parse_ok_signal` for the covered cases. Any produced `Module`, clean or not, counts as success.
/// `true` — a `Module` was produced, `parse_symbols`/`parse_imports`/etc. are meaningful for this text.
/// `false` — swc could not build one at all; the caller should treat this file as broken (degrade).
pub fn parse_ok(rel: &str, text: &str) -> bool {
    parse_with_cm(rel, text).is_some()
}

#[cfg(test)]
mod tests {
    //! Coverage for this crate's parsing/projection API: ESM import bindings, re-exports, dynamic imports,
    //! CommonJS require, local identifier refs, symbol extraction, LOC counting, and CommonIr projection.
    use super::*;

    // --- parse_ok — see the fn doc for the empirical basis of every case below ---

    #[test]
    fn probe_parse_ok_signal() {
        assert!(!parse_ok("b.ts", "function f( {\n  x\n")); // unbalanced brace/paren
        assert!(!parse_ok("s.ts", "}\nfunction foo() {}\n")); // stray closing brace
        assert!(!parse_ok("t.ts", "const x: = 1;\n")); // plain syntax error, braces balanced
        assert!(parse_ok(
            "ok.ts",
            "export function foo(a: { x: number }) {\n  return [a.x, (a.x + 1)];\n}\n"
        ));
        assert!(parse_ok("empty.ts", "")); // legitimately empty file — not broken
        assert!(parse_ok("comment.ts", "// just a comment\n"));
        assert!(parse_ok("dupe.ts", "function f() {}\nfunction f() {}\n")); // semantic, not syntax
    }

    /// Regression: an Angular-style decorated class (class decorator with an object-literal arg, plus
    /// property/method/parameter decorators) used to fail `parse_file_as_module` entirely and degrade the
    /// whole file, because `TsSyntax::decorators` defaults to `false` in swc_ecma_parser.
    #[test]
    fn angular_style_decorators_parse_ok_and_yield_symbols() {
        let src = r#"
import { Component, Input, Output, EventEmitter, HostListener } from '@angular/core';

@Component({
  selector: 'pivot-table',
  template: '<div></div>',
})
export class PivotTableComponent {
  @Input() data: unknown[] = [];
  @Output() changed = new EventEmitter<void>();

  constructor(@Inject(TOKEN) private el: unknown) {}

  @HostListener('window:resize')
  onResize() {
    return this.data.length;
  }
}
"#;
        assert!(parse_ok("pivot-table.component.ts", src));

        let imports = parse_imports("pivot-table.component.ts", src);
        assert_eq!(
            imports["Component"],
            binding("@angular/core", "Component", false)
        );
        assert_eq!(imports["Input"], binding("@angular/core", "Input", false));

        let symbols = parse_symbols("pivot-table.component.ts", src);
        let class = symbols
            .iter()
            .find(|s| s.name == "PivotTableComponent")
            .expect("class symbol survives decorator parsing");
        assert_eq!(class.kind, K::Class);
        assert!(class.exported);
        assert!(symbols
            .iter()
            .any(|s| s.name == "PivotTableComponent.constructor"));
        assert!(symbols
            .iter()
            .any(|s| s.name == "PivotTableComponent.onResize"));
    }

    fn binding(specifier: &str, original: &str, type_only: bool) -> ImportBinding {
        ImportBinding {
            specifier: specifier.into(),
            original: original.into(),
            deferred: false,
            type_only,
        }
    }

    #[test]
    fn named_import() {
        let m = parse_imports("x.ts", "import { foo } from \"mod\";\n");
        assert_eq!(m["foo"], binding("mod", "foo", false));
    }

    #[test]
    fn aliased_named() {
        let m = parse_imports("x.ts", "import { foo as bar } from \"mod\";\n");
        assert_eq!(m["bar"], binding("mod", "foo", false));
        assert!(!m.contains_key("foo"));
    }

    #[test]
    fn default_import() {
        let m = parse_imports("x.ts", "import Foo from \"mod\";\n");
        assert_eq!(m["Foo"], binding("mod", "default", false));
    }

    #[test]
    fn namespace_import() {
        let m = parse_imports("x.ts", "import * as ns from \"mod\";\n");
        assert_eq!(m["ns"], binding("mod", "*", false));
    }

    #[test]
    fn default_plus_named_mixed() {
        let m = parse_imports("x.ts", "import React, { useState } from \"react\";\n");
        assert_eq!(m["React"].original, "default");
        assert_eq!(m["React"].specifier, "react");
        assert_eq!(m["useState"].original, "useState");
    }

    #[test]
    fn side_effect_import_has_no_bindings() {
        let m = parse_imports("x.ts", "import \"side\";\n");
        assert!(m.is_empty());
    }

    #[test]
    fn type_only_named_binding() {
        let m = parse_imports("x.ts", "import type { T } from \"mod\";\n");
        assert_eq!(m["T"], binding("mod", "T", true));
    }

    #[test]
    fn individual_specifier_type_only() {
        let m = parse_imports("x.ts", "import { type T } from \"mod\";\n");
        assert_eq!(m["T"], binding("mod", "T", true));
    }

    #[test]
    fn namespace_type_only() {
        let m = parse_imports("x.ts", "import type * as ns from \"mod\";\n");
        assert_eq!(m["ns"], binding("mod", "*", true));
    }

    #[test]
    fn default_type_only() {
        let m = parse_imports("x.ts", "import type Foo from \"mod\";\n");
        assert_eq!(m["Foo"], binding("mod", "default", true));
    }

    #[test]
    fn mixed_type_and_value_named() {
        let m = parse_imports("x.ts", "import { type T, val } from \"mod\";\n");
        assert!(m["T"].type_only);
        assert!(!m["val"].type_only);
    }

    #[test]
    fn plain_runtime_import_not_type_only() {
        let m = parse_imports("x.ts", "import { foo } from \"mod\";\n");
        assert!(!m["foo"].type_only);
    }

    // --- parseReExportsFromAst ---

    fn reexport(specifier: &str, original: &str, local_alias: &str) -> ReExport {
        reexport_ex(specifier, original, local_alias, false)
    }

    fn reexport_ex(
        specifier: &str,
        original: &str,
        local_alias: &str,
        type_only: bool,
    ) -> ReExport {
        ReExport {
            specifier: specifier.into(),
            original: original.into(),
            local_alias: local_alias.into(),
            type_only,
        }
    }

    #[test]
    fn named_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export { A } from \"./a\";\n"),
            vec![reexport("./a", "A", "A")]
        );
    }

    #[test]
    fn aliased_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export { A as B } from \"./a\";\n"),
            vec![reexport("./a", "A", "B")]
        );
    }

    #[test]
    fn star_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export * from \"./a\";\n"),
            vec![reexport("./a", "*", "*")]
        );
    }

    #[test]
    fn namespace_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export * as ns from \"./a\";\n"),
            vec![reexport("./a", "*", "ns")]
        );
    }

    #[test]
    fn bare_export_is_not_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "const x = 1; export { x };\n"),
            Vec::<ReExport>::new()
        );
    }

    #[test]
    fn type_only_named_re_export_clause() {
        // `export type { X } from "./y"` — clause-level type-only.
        assert_eq!(
            parse_re_exports("x.ts", "export type { X } from \"./a\";\n"),
            vec![reexport_ex("./a", "X", "X", true)]
        );
    }

    #[test]
    fn type_only_per_specifier_re_export() {
        // `export { type X, y } from "./a"` — only `X` is type-only; `y` is still a runtime re-export.
        let out = parse_re_exports("x.ts", "export { type X, y } from \"./a\";\n");
        assert_eq!(
            out,
            vec![
                reexport_ex("./a", "X", "X", true),
                reexport("./a", "y", "y"),
            ]
        );
    }

    #[test]
    fn type_only_star_re_export() {
        // `export type * from "./a"` — the whole re-export is type-only.
        assert_eq!(
            parse_re_exports("x.ts", "export type * from \"./a\";\n"),
            vec![reexport_ex("./a", "*", "*", true)]
        );
    }

    #[test]
    fn type_only_namespace_re_export() {
        // `export type * as ns from "./a"` — clause-level type-only applies to the namespace alias too.
        assert_eq!(
            parse_re_exports("x.ts", "export type * as ns from \"./a\";\n"),
            vec![reexport_ex("./a", "*", "ns", true)]
        );
    }

    // --- parseDynamicImportsFromAst ---

    #[test]
    fn dynamic_import_single() {
        assert_eq!(
            parse_dynamic_imports("x.ts", "const m = import(\"./x\");\n"),
            vec!["./x".to_string()]
        );
    }

    #[test]
    fn await_import_in_chain_captured() {
        assert_eq!(
            parse_dynamic_imports(
                "x.ts",
                "async function f() { const m = await import(\"./deep\"); }\n"
            ),
            vec!["./deep".to_string()]
        );
    }

    #[test]
    fn lazy_import_multiple() {
        assert_eq!(
            parse_dynamic_imports(
                "x.ts",
                "const A = lazy(() => import(\"./a\"));\nconst B = lazy(() => import(\"./b\"));\n"
            ),
            vec!["./a".to_string(), "./b".to_string()]
        );
    }

    #[test]
    fn non_string_literal_argument_skipped() {
        assert_eq!(
            parse_dynamic_imports("x.ts", "const n = \"x\"; const m = import(n);\n"),
            Vec::<String>::new()
        );
    }

    // --- parseImports CommonJS require() ---

    #[test]
    fn top_level_var_require_binds_namespace() {
        let m = parse_imports("x.js", "var X = require('./y');\n");
        assert_eq!(m["X"], binding("./y", "*", false));
        assert!(!m["X"].deferred);
    }

    #[test]
    fn destructured_require_binds_original_names() {
        let m = parse_imports("x.js", "const { a, b: c } = require('./y');\n");
        assert_eq!(m["a"].specifier, "./y");
        assert_eq!(m["a"].original, "a");
        assert!(!m["a"].deferred);
        assert_eq!(m["c"].specifier, "./y");
        assert_eq!(m["c"].original, "b");
        assert!(!m["c"].deferred);
    }

    #[test]
    fn require_inside_function_body_is_deferred() {
        let m = parse_imports("x.js", "function f(){ require('./y').go(); }\n");
        assert_eq!(m.len(), 1);
        let entry = m.values().next().unwrap();
        assert_eq!(entry.specifier, "./y");
        assert!(entry.deferred);
    }

    #[test]
    fn top_level_and_nested_require_of_same_target_keep_both_flags() {
        let m = parse_imports(
            "x.js",
            "var Sleeping = require('../core/Sleeping');\nfunction set(){ require('../core/Sleeping').set(); }\n",
        );
        let deferred_flags: Vec<bool> = m.values().map(|b| b.deferred).collect();
        assert!(deferred_flags.contains(&false));
        assert!(deferred_flags.contains(&true));
    }

    #[test]
    fn bare_side_effect_require_records_edge() {
        let m = parse_imports("x.js", "require('./y');\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m.values().next().unwrap().specifier, "./y");
    }

    // --- parseLocalIdentifierRefsFromAst ---

    #[test]
    fn local_identifier_refs_excludes_declarations_and_property_names() {
        let refs = parse_local_identifier_refs(
            "x.ts",
            "const X = 1;\nfunction foo() { return X + Y; }\nconst obj = { bar: 2 };\nconst z = obj.bar;\n",
        );
        assert!(refs.contains("X"));
        assert!(refs.contains("Y"));
        assert!(refs.contains("obj"));
        // X's own decl name, foo's own decl name, and `bar` (property key + property access) are excluded.
        assert!(!refs.contains("foo"));
        assert!(!refs.contains("bar"));
    }

    #[test]
    fn local_identifier_refs_excludes_import_export_names() {
        let refs = parse_local_identifier_refs(
            "x.ts",
            "import { A, B } from \"m\";\nexport { A } from \"m\";\nconst c = A + B;\n",
        );
        // only A / B referenced in `const c = A + B` should appear — import/export declarations are skipped.
        let got: Vec<&String> = refs.iter().collect();
        assert_eq!(got, vec!["A", "B"]);
    }

    #[test]
    fn local_identifier_refs_dedups_repeated_reference() {
        let refs = parse_local_identifier_refs(
            "x.ts",
            "const X = 1;\nfunction f() { return X + X + X; }\n",
        );
        assert!(refs.contains("X"));
        assert_eq!(refs.len(), 1);
    }

    // --- parseSymbols (top-level; sub-symbols are follow-ups) ---
    use zzop_core::SourceSymbolKind as K;

    fn names(syms: &[SourceSymbol]) -> Vec<&str> {
        syms.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn export_function_extracted() {
        let s = parse_symbols("x.ts", "export function foo() { return 1; }\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "x.ts#foo");
        assert_eq!(s[0].name, "foo");
        assert_eq!(s[0].kind, K::Function);
        assert!(s[0].exported);
        assert_eq!(s[0].line, 1);
        assert_eq!(s[0].body_start, Some(1));
        assert!(s[0].body_end.unwrap() >= 1);
    }

    #[test]
    fn function_without_export() {
        let s = parse_symbols("x.ts", "function inner() {}\n");
        assert_eq!(s[0].name, "inner");
        assert!(!s[0].exported);
    }

    #[test]
    fn const_arrow_is_function_kind() {
        let s = parse_symbols(
            "x.ts",
            "export const bar = () => 42;\nexport const BAZ = 7;\n",
        );
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "bar");
        assert_eq!(s[0].kind, K::Function);
        assert!(s[0].exported);
        assert_eq!(s[1].name, "BAZ");
        assert_eq!(s[1].kind, K::Const);
        assert!(s[0].body_start.is_some());
        assert!(s[1].body_start.is_none());
    }

    #[test]
    fn class_body_lines() {
        let s = parse_symbols("x.ts", "export class Foo {\n  bar() {}\n}\n");
        assert_eq!(s[0].name, "Foo");
        assert_eq!(s[0].kind, K::Class);
        assert!(s[0].exported);
        assert!(s[0].body_end.unwrap() > s[0].body_start.unwrap());
    }

    #[test]
    fn interface_and_type_no_body() {
        let s = parse_symbols(
            "x.ts",
            "export interface Shape { size: number }\nexport type Id = string | number;\n",
        );
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].name.as_str(), s[0].kind), ("Shape", K::Interface));
        assert_eq!((s[1].name.as_str(), s[1].kind), ("Id", K::Type));
        assert!(s[0].body_start.is_none());
        assert!(s[1].body_start.is_none());
    }

    #[test]
    fn default_anonymous_function() {
        let s = parse_symbols("x.ts", "export default function() { return 1; }\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "default");
        assert_eq!(s[0].kind, K::Function);
        assert!(s[0].exported);
        assert!(s[0].is_default);
    }

    #[test]
    fn default_named_function() {
        let s = parse_symbols("x.ts", "export default function Foo() { return 1; }\n");
        assert_eq!(s[0].name, "Foo");
        assert!(s[0].is_default);
    }

    #[test]
    fn export_function_no_default() {
        let s = parse_symbols("x.ts", "export function Foo() {}\n");
        assert!(!s[0].is_default);
    }

    #[test]
    fn line_number_is_1_based() {
        let s = parse_symbols("x.ts", "\n\nexport function multi() {}\n");
        assert_eq!(s[0].line, 3);
    }

    #[test]
    fn multiple_declarations_preserve_order() {
        let s = parse_symbols(
            "x.ts",
            "export function a() {}\nfunction b() {}\nexport class C {}\n",
        );
        assert_eq!(names(&s), vec!["a", "b", "C"]);
    }

    #[test]
    fn require_initializer_skipped() {
        // a CJS import alias is not a symbol
        let s = parse_symbols("x.js", "const X = require('./y');\n");
        assert!(s.is_empty());
    }

    // --- parseSymbols class-method sub-symbols ---

    #[test]
    fn class_method_sub_symbols() {
        let s = parse_symbols(
            "x.ts",
            "export class Svc {\n  foo() {}\n  async bar() {}\n}\n",
        );
        assert_eq!(names(&s), vec!["Svc", "Svc.foo", "Svc.bar"]);
        assert_eq!(s[1].kind, K::Function);
        assert!(!s[1].exported);
        assert!(s[1].body_start.unwrap() > 0);
    }

    #[test]
    fn class_constructor_static_get_set_private() {
        let s = parse_symbols(
            "x.ts",
            "class C {\n  constructor() {}\n  static s() {}\n  get g() { return 1 }\n  set g(v) {}\n  #p() {}\n}\n",
        );
        // same name for get/set -> only the first
        assert_eq!(names(&s), vec!["C", "C.constructor", "C.s", "C.g", "C.#p"]);
    }

    #[test]
    fn class_property_not_extracted() {
        let s = parse_symbols("x.ts", "class C {\n  field = 1;\n  method() {}\n}\n");
        assert_eq!(names(&s), vec!["C", "C.method"]);
    }

    #[test]
    fn class_computed_and_string_names_skipped() {
        let s = parse_symbols(
            "x.ts",
            "class C {\n  [\"dyn\"]() {}\n  \"str\"() {}\n  ok() {}\n}\n",
        );
        assert_eq!(names(&s), vec!["C", "C.ok"]);
    }

    #[test]
    fn anonymous_default_class_methods() {
        let s = parse_symbols("x.ts", "export default class { foo() {} bar() {} }\n");
        assert_eq!(names(&s), vec!["default", "default.foo", "default.bar"]);
    }

    // --- parseSymbols binding patterns ---

    #[test]
    fn object_destructuring_each_binding_extracted() {
        let s = parse_symbols("x.ts", "export const { a, b } = obj;\n");
        assert_eq!(names(&s), vec!["a", "b"]);
        assert_eq!(s[0].kind, K::Const);
        assert!(s[0].exported);
        assert_eq!(s[1].kind, K::Const);
        assert!(s[1].exported);
    }

    #[test]
    fn array_destructuring_skips_empty_slots() {
        let s = parse_symbols("x.ts", "export const [first, , third] = arr;\n");
        assert_eq!(names(&s), vec!["first", "third"]);
    }

    #[test]
    fn nested_destructuring_flattened() {
        let s = parse_symbols("x.ts", "const { outer: { inner }, sibling } = obj;\n");
        let mut got = names(&s);
        got.sort_unstable();
        assert_eq!(got, vec!["inner", "sibling"]);
    }

    // --- parseSymbols factory sub-symbols ---

    #[test]
    fn factory_const_object_literal_methods() {
        let s = parse_symbols(
            "x.ts",
            "export const api = {\n  getA: () => 1,\n  getB: async () => 2,\n};\n",
        );
        assert_eq!(names(&s), vec!["api", "api.getA", "api.getB"]);
        assert_eq!(
            s.iter().find(|s| s.name == "api.getA").unwrap().kind,
            K::Function
        );
    }

    #[test]
    fn factory_function_return_object() {
        let s = parse_symbols(
            "x.ts",
            "export function createApi(deps) {\n  return {\n    deleteMe: async () => 1,\n    getUser: async () => 2,\n  };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec!["createApi", "createApi.deleteMe", "createApi.getUser"]
        );
    }

    #[test]
    fn factory_spread_two_hop_flatten() {
        let s = parse_symbols(
            "x.ts",
            "const base = {\n  shared: () => 1,\n};\nfunction createApi() {\n  return { ...base, direct: () => 2 };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec![
                "base",
                "base.shared",
                "createApi",
                "createApi.shared",
                "createApi.direct"
            ]
        );
    }

    #[test]
    fn factory_spread_target_not_in_file_skipped() {
        let s = parse_symbols(
            "x.ts",
            "function createApi() {\n  return { ...unknown, direct: () => 1 };\n}\n",
        );
        assert_eq!(names(&s), vec!["createApi", "createApi.direct"]);
    }

    #[test]
    fn factory_spread_cycle_prevented() {
        // depth-first flattening — visited guard cuts a->b->a after one pass; no infinite loop, each key appears exactly once.
        let s = parse_symbols(
            "x.ts",
            "const a = { ...b, x: 1 };\nconst b = { ...a, y: 1 };\nfunction createApi() {\n  return { ...a, z: () => 1 };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec![
                "a",
                "a.x",
                "a.y",
                "b",
                "b.y",
                "b.x",
                "createApi",
                "createApi.y",
                "createApi.x",
                "createApi.z"
            ]
        );
    }

    #[test]
    fn factory_spread_priority_later_key_wins() {
        // spread processed first so `foo` is inserted; a subsequent PropertyAssignment `foo` is a duplicate id -> skipped.
        let s = parse_symbols(
            "x.ts",
            "const base = { foo: () => 1 };\nfunction createApi() {\n  return { ...base, foo: () => 2, bar: () => 3 };\n}\n",
        );
        assert_eq!(
            names(&s),
            vec![
                "base",
                "base.foo",
                "createApi",
                "createApi.foo",
                "createApi.bar"
            ]
        );
    }

    #[test]
    fn factory_const_object_lit_spread_two_hop() {
        let s = parse_symbols(
            "x.ts",
            "const base = { foo: () => 1 };\nexport const ext = { ...base, bar: () => 2 };\n",
        );
        assert_eq!(
            names(&s),
            vec!["base", "base.foo", "ext", "ext.foo", "ext.bar"]
        );
    }

    // --- parseSymbols CommonJS exports (module.exports / exports.x) ---

    #[test]
    fn cjs_module_exports_obj_then_member_assignment_bare_name() {
        let src =
            "var Body = {};\nmodule.exports = Body;\nBody.create = function (o) { return o; };\n";
        let s = parse_symbols("body/Body.js", src);
        let create = s
            .iter()
            .find(|s| s.name == "create")
            .expect("create exported");
        assert_eq!(create.id, "body/Body.js#create");
        assert_eq!(create.kind, K::Function);
        assert!(create.exported);
        assert!(create.body_start.is_some());
    }

    #[test]
    fn cjs_members_inside_iife_are_found() {
        let src = "var Body = {};\nmodule.exports = Body;\n(function () { Body.update = function () {}; Body.scale = function () {}; })();\n";
        let syms = parse_symbols("x.js", src);
        assert!(syms.iter().any(|s| s.name == "update"));
        assert!(syms.iter().any(|s| s.name == "scale"));
    }

    #[test]
    fn cjs_exports_x_and_module_exports_y_are_members() {
        let src = "exports.foo = function () {};\nmodule.exports.bar = 1;\n";
        let syms = parse_symbols("x.js", src);
        assert!(syms.iter().any(|s| s.name == "foo"));
        assert!(syms.iter().any(|s| s.name == "bar"));
    }

    #[test]
    fn cjs_module_exports_object_literal_members() {
        let src = "function fn(){}\nmodule.exports = { a: 1, b: fn };\n";
        let syms = parse_symbols("x.js", src);
        assert!(syms.iter().any(|s| s.name == "a"));
        assert!(syms.iter().any(|s| s.name == "b"));
    }

    #[test]
    fn cjs_non_export_object_property_assignment_ignored() {
        let src = "var local = {};\nlocal.foo = function () {};\n";
        let syms = parse_symbols("x.js", src);
        assert!(!syms.iter().any(|s| s.name == "foo"));
    }

    #[test]
    fn cjs_require_alias_vars_not_symbols_export_members_are() {
        let src = "var Vector = require('../geometry/Vector');\nvar Body = {};\nmodule.exports = Body;\nBody.create = function () {};\n";
        let syms = parse_symbols("body/Body.js", src);
        assert!(!syms.iter().any(|s| s.name == "Vector")); // import alias, not a symbol
        assert!(syms.iter().any(|s| s.name == "create")); // CJS export member
    }

    // --- count_loc / build_common_ir --- (see count_loc's doc for the exact semantics these encode)

    #[test]
    fn count_loc_counts_blank_and_comment_lines() {
        assert_eq!(count_loc("export const x = 1;\n\n// comment\nfoo();\n"), 5);
    }

    #[test]
    fn count_loc_counts_block_comment_interior_lines() {
        assert_eq!(count_loc("/* block\n still block */\ncode();\n"), 4);
    }

    #[test]
    fn count_loc_trailing_newline_adds_one() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc("a\nb"), 2);
    }

    #[test]
    fn count_loc_empty_text_is_one() {
        assert_eq!(count_loc(""), 1);
    }

    #[test]
    fn detects_circular_imports_end_to_end() {
        // Vertical slice: parse TS -> build_common_ir -> circular_from_dep -> cycle found.
        let files = vec![
            (
                "a.ts".to_string(),
                "import { b } from './b';\nexport const a = 1;\n".to_string(),
            ),
            (
                "b.ts".to_string(),
                "import { a } from './a';\nexport const b = 1;\n".to_string(),
            ),
        ];
        let ir = build_common_ir("app", &files);
        let cycles = zzop_core::circular_from_dep(&ir.ir.dep);
        assert_eq!(cycles.len(), 1);
        let mut got = cycles[0].clone();
        got.sort();
        assert_eq!(got, vec!["a.ts".to_string(), "b.ts".to_string()]);
    }

    #[test]
    fn cross_layer_fe_to_be_end_to_end() {
        // Crown-jewel slice: FE TS egress -> IoFacts -> cross-layer join to a BE provider.
        use zzop_core::{link_cross_layer_io, IoFacts, IoProvide, SourceIo};
        let fe_files = vec![(
            "Ctx.tsx".to_string(),
            r#"axios.get("/authen/getUserInfo")"#.to_string(),
        )];
        let fe_ir = build_common_ir("fe", &fe_files);
        let fe = SourceIo {
            source: "fe".to_string(),
            io: fe_ir.ir.io.clone().expect("FE consumes the route"),
        };
        let be = SourceIo {
            source: "be".to_string(),
            io: IoFacts {
                provides: vec![IoProvide {
                    kind: "http".to_string(),
                    key: "GET /authen/getUserInfo".to_string(),
                    file: "CtrlAuthen.java".to_string(),
                    line: 40,
                    symbol: Some("getUserInfo".to_string()),
                }],
                consumes: Vec::new(),
            },
        };
        let r = link_cross_layer_io(&[fe, be], &zzop_core::LinkOptions::default());
        assert_eq!(r.edges.len(), 1);
        assert!(r.edges[0].cross_source);
        assert_eq!(r.edges[0].key, "GET /authen/getUserInfo");
        assert_eq!(r.edges[0].to.source, "be");
    }

    #[test]
    fn build_common_ir_projects_symbols_dep_loc() {
        let files = vec![
            (
                "a.ts".to_string(),
                "import { x } from './b';\nexport function foo() {}\n".to_string(),
            ),
            ("b.ts".to_string(), "export const x = 1;\n".to_string()),
        ];
        let ir = build_common_ir("app", &files);
        assert_eq!(ir.source, "app");
        assert_eq!(ir.parser, "typescript");
        assert_eq!(ir.ir.dep["a.ts"], vec!["b.ts".to_string()]);
        assert!(ir.ir.symbols.iter().any(|s| s.name == "foo"));
        assert!(ir.ir.symbols.iter().any(|s| s.name == "x"));
        assert_eq!(ir.ir.loc["a.ts"], 3); // trailing-newline artifact, see count_loc's doc
    }
}
