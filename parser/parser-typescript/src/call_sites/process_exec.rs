//! The `process-exec` half of the TypeScript/JavaScript call-site producer — binding resolution for
//! Node's `child_process`, kept out of [`super`] so neither file's judgment has to be read through the
//! other's (and so each stays inside the repo's per-file line cap).
//!
//! # Why this family needs a resolver where `console-write` did not
//! `console` and `process.env` are GLOBALS: the spelling at the site is the whole evidence, so
//! `super`'s two producers are pure syntax checks. `exec` is not a global — it is whatever the file
//! bound it to, and the bare spelling `exec(cmd)` is `RegExp.prototype.exec`'s name just as often as
//! Node's. That is precisely the false-positive class the consuming rule
//! (`security/shell-exec-interpolation`) approximated with `require_file: "child_process"` plus a
//! receiver-shaped regex. So this producer resolves the local name against the file's OWN module
//! bindings and emits nothing when it cannot: **the callee resolves, or there is no site**.
//!
//! # What is recognized
//! Bindings introduced at MODULE level from `child_process` / `node:child_process`:
//! - `import { exec, execSync as sh } from "child_process"` — a named import, alias included; the
//!   local name is what the site spells and what `callee` carries.
//! - `import cp from "child_process"` / `import * as cp from "child_process"` — a namespace-ish
//!   binding, whose member calls (`cp.exec(...)`) are sites spelled `cp.exec`.
//! - `const cp = require("child_process")` and `const { exec } = require("child_process")` — the
//!   CommonJS twins of the two above.
//!
//! A call is a site when its callee is one of those bindings (bare) or a member call on a namespace
//! binding whose property is in [`CHILD_PROCESS_EXEC_METHODS`]. `callee` is the spelling AS WRITTEN
//! (`exec`, `sh`, `cp.execSync`) — never rewritten to a canonical name, which is the channel's
//! original-spelling contract and the reason an alias stays visible to a rule that wants it.
//!
//! # Deliberate silences — each a decision, not a gap
//! - **Third-party wrappers** (`execa`, `shelljs`, `zx`'s `$`, `cross-spawn`) are NOT this family in
//!   v1. They are not the platform's API, each has its own escaping/argv semantics, and the consuming
//!   rules' claims (`exec`'s string IS handed to a shell) are stated about Node's API specifically —
//!   folding a wrapper in would attach those claims to semantics nobody verified. Adding one is a
//!   producer+rule change with its own evidence, never a quiet widening here.
//! - **A re-exported or indirected binding** (`import { exec } from "./my-exec"`, a dynamic
//!   `await import("child_process")`, `const { exec } = deps.cp`) — the module the name came from is
//!   not `child_process` at this file's own level, so nothing resolves and no site is emitted. Recall
//!   direction, never a guess.
//! - **A method reached off a non-binding receiver** (`this.cp.exec(...)`, `deps.cp.exec(...)`) — the
//!   receiver is not a resolved module binding. Same line.
//! - **`fork`** — spawns a Node child by MODULE path, not a shell command or program name; none of the
//!   three consuming rules asks about it, and including it would put a different failure mode under
//!   one word.
//! - **A shadowing local** (`function exec() {}` in the same file that imports it) is not tracked: the
//!   binding map is file-scoped with no scope analysis, so a local `exec` inside one function still
//!   reads as the import. Over-claiming in that one direction, disclosed here and pinned by test, is
//!   the same tradeoff `super`'s "Known imprecision, accepted" section states for `console`.

use std::collections::HashMap;

use swc_core::ecma::ast::{
    Callee, Decl, Expr, ImportSpecifier, Lit, MemberProp, Module, ModuleDecl, ModuleItem, Pat,
    Stmt, VarDeclarator,
};

/// The `child_process` functions that count as a PROCESS EXEC — the platform's own names (Node's
/// `child_process`), not names a project picks, so they are built in and not declarable. `fork` is
/// deliberately absent (module doc).
pub const CHILD_PROCESS_EXEC_METHODS: &[&str] = &[
    "exec",
    "execSync",
    "spawn",
    "spawnSync",
    "execFile",
    "execFileSync",
];

/// The two module specifiers that ARE Node's child-process module. A bare specifier and its
/// `node:`-prefixed form are the same module by Node's own resolution rules, so treating them alike is
/// the platform's judgment, not this producer's.
const CHILD_PROCESS_SPECIFIERS: &[&str] = &["child_process", "node:child_process"];

/// What a local name resolved to, when it resolved at all.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Binding {
    /// The module object itself (`import cp from …`, `const cp = require(…)`) — a member call on it
    /// is a site when the property is a recognized method.
    Namespace,
    /// One recognized function, bound directly (`import { exec }`, `const { execSync: sh } = …`) — a
    /// BARE call on it is a site.
    ExecFn,
}

/// Every module-level `child_process` binding in this file, keyed by the LOCAL name the source uses.
/// Empty when the file never imports the module, which is the fast path for almost every file.
pub(super) fn child_process_bindings(module: &Module) -> HashMap<String, Binding> {
    module_bindings(module, CHILD_PROCESS_SPECIFIERS, CHILD_PROCESS_EXEC_METHODS)
}

/// Every module-level binding of `specifiers` in this file, keyed by the LOCAL name the source uses:
/// a named import of one of `methods` (alias included) or a `require`-destructured one becomes
/// [`Binding::ExecFn`], a default/namespace import or a whole-module `require` becomes
/// [`Binding::Namespace`]. Generic over the module because the SHAPES are the language's, not the
/// module's — `hash_call` resolves Node `crypto` through exactly this walk, and a third module would
/// need no new code here either.
pub(super) fn module_bindings(
    module: &Module,
    specifiers: &[&str],
    methods: &[&str],
) -> HashMap<String, Binding> {
    let mut out = HashMap::new();
    for item in &module.body {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(import)) => {
                if !specifiers.contains(&import.src.value.as_str().unwrap_or_default()) {
                    continue;
                }
                for spec in &import.specifiers {
                    match spec {
                        // `import { exec, execSync as sh } from "child_process"` — `imported` is the
                        // ORIGINAL name when an alias is present, `local` otherwise.
                        ImportSpecifier::Named(n) => {
                            let original = n.imported.as_ref().map_or_else(
                                || n.local.sym.to_string(),
                                |m| match m {
                                    swc_core::ecma::ast::ModuleExportName::Ident(i) => {
                                        i.sym.to_string()
                                    }
                                    swc_core::ecma::ast::ModuleExportName::Str(s) => {
                                        s.value.as_str().unwrap_or_default().to_string()
                                    }
                                },
                            );
                            if methods.contains(&original.as_str()) {
                                out.insert(n.local.sym.to_string(), Binding::ExecFn);
                            }
                        }
                        ImportSpecifier::Default(d) => {
                            out.insert(d.local.sym.to_string(), Binding::Namespace);
                        }
                        ImportSpecifier::Namespace(ns) => {
                            out.insert(ns.local.sym.to_string(), Binding::Namespace);
                        }
                    }
                }
            }
            // `const cp = require("child_process")` / `const { exec } = require(…)`, at module level
            // only — a `require` inside a function body is not walked, the same module-scope line the
            // import arms draw.
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(var))) => {
                for d in &var.decls {
                    record_require(d, specifiers, methods, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

/// One `const … = require("child_process")` declarator, in either of its two binding shapes.
fn record_require(
    d: &VarDeclarator,
    specifiers: &[&str],
    methods: &[&str],
    out: &mut HashMap<String, Binding>,
) {
    let Some(init) = &d.init else { return };
    if !is_module_require(init, specifiers) {
        return;
    }
    match &d.name {
        Pat::Ident(id) => {
            out.insert(id.id.sym.to_string(), Binding::Namespace);
        }
        Pat::Object(obj) => {
            for prop in &obj.props {
                match prop {
                    // `const { exec } = require(…)` — shorthand, local name IS the original.
                    swc_core::ecma::ast::ObjectPatProp::Assign(a) => {
                        if methods.contains(&a.key.sym.as_str()) {
                            out.insert(a.key.sym.to_string(), Binding::ExecFn);
                        }
                    }
                    // `const { execSync: sh } = require(…)` — key is the original, value the local.
                    swc_core::ecma::ast::ObjectPatProp::KeyValue(kv) => {
                        let (Some(key), Pat::Ident(local)) = (key_name(&kv.key), &*kv.value) else {
                            continue;
                        };
                        if methods.contains(&key.as_str()) {
                            out.insert(local.id.sym.to_string(), Binding::ExecFn);
                        }
                    }
                    swc_core::ecma::ast::ObjectPatProp::Rest(_) => {}
                }
            }
        }
        _ => {}
    }
}

fn key_name(key: &swc_core::ecma::ast::PropName) -> Option<String> {
    match key {
        swc_core::ecma::ast::PropName::Ident(i) => Some(i.sym.to_string()),
        swc_core::ecma::ast::PropName::Str(s) => {
            Some(s.value.as_str().unwrap_or_default().to_string())
        }
        _ => None,
    }
}

/// `require("child_process")` — a bare `require` call with exactly a string-literal specifier. A
/// computed specifier is not a resolvable spelling and is not treated as one.
fn is_module_require(expr: &Expr, specifiers: &[&str]) -> bool {
    let Expr::Call(call) = expr else { return false };
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    if !matches!(&**callee, Expr::Ident(i) if i.sym.as_str() == "require") {
        return false;
    }
    call.args.first().is_some_and(|a| {
        matches!(&*a.expr, Expr::Lit(Lit::Str(s))
            if specifiers.contains(&s.value.as_str().unwrap_or_default()))
    })
}

/// The callee spelling to record for this call, or `None` when it is not a resolved exec binding.
/// Returns the spelling AS WRITTEN (module doc), so an alias survives into the channel.
pub(super) fn exec_callee(callee: &Expr, bindings: &HashMap<String, Binding>) -> Option<String> {
    resolved_callee(callee, bindings, CHILD_PROCESS_EXEC_METHODS)
}

/// The shared resolver both families use — see [`exec_callee`] for the contract it implements.
pub(super) fn resolved_callee(
    callee: &Expr,
    bindings: &HashMap<String, Binding>,
    methods: &[&str],
) -> Option<String> {
    if bindings.is_empty() {
        return None; // fast path: the file never imported the module.
    }
    match callee {
        Expr::Ident(i) => {
            let name = i.sym.as_str();
            matches!(bindings.get(name), Some(Binding::ExecFn)).then(|| name.to_string())
        }
        Expr::Member(m) => {
            let Expr::Ident(obj) = &*m.obj else {
                return None;
            };
            let MemberProp::Ident(prop) = &m.prop else {
                return None;
            };
            let is_ns = matches!(bindings.get(obj.sym.as_str()), Some(Binding::Namespace));
            (is_ns && methods.contains(&prop.sym.as_str()))
                .then(|| format!("{}.{}", obj.sym, prop.sym))
        }
        _ => None,
    }
}
