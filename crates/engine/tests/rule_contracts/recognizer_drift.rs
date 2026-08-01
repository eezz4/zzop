//! The anti-drift seal for `zzop_engine::framework_recognizers`.
//!
//! A capability disclosure that can silently fall behind the code it describes is worse than none: it
//! converts "we do not know" into a confident wrong answer. The declarations live in each parser
//! crate's `lib.rs` (`FRAMEWORK_RECOGNIZERS`), while the recognizers themselves are adapter MODULES on
//! disk. Nothing in the compiler binds the two — adding `parser/parser-go/src/adapters/echo.rs` and
//! wiring it up compiles perfectly with no declaration, and the disclosure would then state that zzop
//! does not know Echo while shipping an Echo recognizer.
//!
//! So this file crosses the declared set against the adapter modules actually on disk, in BOTH
//! directions, with an explicit exemption list for the modules that are mechanisms rather than
//! frameworks. Same shape as `catalog_sync`'s sightline set-equality test, and for the same reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Adapter modules that refine or resolve what a framework row already found, rather than recognizing
/// a framework of their own. Each entry needs a reason, because an unexplained exemption is how a
/// missing declaration hides. Keyed `parser/module`.
const NOT_A_FRAMEWORK: &[(&str, &str)] = &[
    (
        "parser-python-3/const_map",
        "reads project constants into the language-neutral const map; recognizes no framework and \
         emits no io channel — the map is what OTHER adapters' unresolved references resolve against",
    ),
    (
        "parser-typescript/class_shapes",
        "shape refinement over rows other adapters produced",
    ),
    (
        "parser-typescript/wrapper_calls",
        "resolves a call through a user wrapper; no framework of its own",
    ),
    (
        "parser-typescript/pathname_dispatch",
        "route-shape heuristic shared by several framework rows",
    ),
    (
        "parser-typescript/global_prefix",
        "prefix composition applied to rows already found",
    ),
    (
        "parser-typescript/client_base",
        "resolves a client's base URL; the client row is `axios`/`fetch`",
    ),
    (
        "parser-typescript/client_base_generated",
        "same, for generated SDK clients",
    ),
    (
        "parser-typescript/router_mounts",
        "mount composition; the framework row is `express`",
    ),
    (
        "parser-typescript/controller_decorators",
        "the framework row is `nestjs`",
    ),
    (
        "parser-typescript/nest_middleware",
        "the framework row is `nestjs`",
    ),
    (
        "parser-typescript/entity_decorators",
        "the framework row is `typeorm`",
    ),
    (
        "parser-typescript/typeorm_repository",
        "the framework row is `typeorm`",
    ),
    (
        "parser-typescript/db_table_consume",
        "the framework row is `prisma client`",
    ),
    ("parser-typescript/raw_sql", "declared as the `raw sql` row"),
    (
        "parser-typescript/next_pages_api",
        "the framework row is `next.js`",
    ),
    (
        "parser-typescript/hono_client",
        "the framework row is `hono`",
    ),
    (
        "parser-typescript/trpc_router",
        "the framework row is `trpc`",
    ),
    (
        "parser-typescript/trpc_consume",
        "the framework row is `trpc`",
    ),
    (
        "parser-typescript/egress",
        "declared as the `axios`/`fetch` rows",
    ),
    (
        "parser-python-3/guard_vocab",
        "auth-guard vocabulary, not an io recognizer",
    ),
    (
        "parser-python-3/django_routes",
        "the framework row is `django`",
    ),
    (
        "parser-python-3/http_clients",
        "declared as the `httpx` row",
    ),
    (
        "parser-go/http_clients",
        "declared under the `net/http` consume row",
    ),
    (
        "parser-go/net_http",
        "declared as the `net/http` provide row",
    ),
    ("parser-rust/http_clients", "declared as the `reqwest` row"),
    (
        "parser-csharp/http_clients",
        "declared as the `httpclient` row",
    ),
    (
        "parser-csharp/provides",
        "declared as the `asp.net core` row",
    ),
];

/// Every adapter module on disk, as `parser-<lang>/<module>`.
fn adapter_modules() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let parsers = std::fs::read_dir(repo().join("parser")).expect("parser/ is readable");
    for p in parsers.flatten() {
        let name = p.file_name().to_string_lossy().to_string();
        if !name.starts_with("parser-") {
            continue;
        }
        let adapters = p.path().join("src/adapters");
        let Ok(entries) = std::fs::read_dir(&adapters) else {
            continue; // a parser with no adapters/ dir (java-21, prisma, sql) is not a failure
        };
        for e in entries.flatten() {
            let f = e.file_name().to_string_lossy().to_string();
            let module = f.strip_suffix(".rs").unwrap_or(&f).to_string();
            if module == "mod" || module == "tests" {
                continue;
            }
            out.insert(format!("{name}/{module}"));
        }
    }
    out
}

/// DIRECTION 1 — every adapter module is accounted for: it either backs a declared framework row or
/// carries an exemption saying why it is not a framework. This is the direction that catches the real
/// hazard: shipping a recognizer nobody disclosed.
#[test]
fn every_adapter_module_is_declared_or_explicitly_exempt() {
    let exempt: BTreeSet<&str> = NOT_A_FRAMEWORK.iter().map(|(m, _)| *m).collect();
    let modules = adapter_modules();
    assert!(
        !modules.is_empty(),
        "found zero adapter modules — the scan root is wrong, and an empty subject set would make \
         this whole test vacuously green (the exact failure this repo's guard-coverage rule exists for)"
    );
    let undeclared: Vec<&String> = modules
        .iter()
        .filter(|m| !exempt.contains(m.as_str()))
        .filter(|m| {
            // A module counts as declared when some row's framework name matches it, after folding the
            // separators the two vocabularies spell differently (`net/http` vs `net_http`). Keep the
            // ones that match NOTHING — those are the undeclared recognizers this test hunts.
            let fold = |s: &str| {
                s.to_ascii_lowercase()
                    .replace(['-', '_', '/', '.', ' '], "")
            };
            let m = fold(m.rsplit('/').next().unwrap_or(m));
            !zzop_engine::framework_recognizers()
                .iter()
                .any(|r| m.contains(&fold(r.framework)) || fold(r.framework).contains(&m))
        })
        .collect();
    assert!(
        undeclared.is_empty(),
        "adapter module(s) with neither a FRAMEWORK_RECOGNIZERS row nor a NOT_A_FRAMEWORK exemption: \
         {undeclared:?}\n\
         A shipped recognizer that nobody declared makes the capability disclosure state that zzop \
         does not know a framework it does know. Declare it in that parser's lib.rs, or exempt it \
         WITH a reason."
    );
}

/// DIRECTION 2 — no exemption outlives the module it excuses. A stale entry here would silently
/// re-open direction 1 for a module that later comes back under the same name.
#[test]
fn no_exemption_names_a_module_that_no_longer_exists() {
    let modules = adapter_modules();
    let stale: Vec<&str> = NOT_A_FRAMEWORK
        .iter()
        .map(|(m, _)| *m)
        .filter(|m| !modules.contains(*m))
        .collect();
    assert!(
        stale.is_empty(),
        "NOT_A_FRAMEWORK exempts module(s) that are gone: {stale:?} — delete the entries"
    );
}

/// Every exemption carries a reason. An exemption list that fills up with bare names is how the
/// judgment behind each one gets lost, and then re-litigated wrongly.
#[test]
fn every_exemption_carries_a_reason() {
    for (module, reason) in NOT_A_FRAMEWORK {
        assert!(
            reason.len() > 15,
            "{module}'s exemption reason is too thin to be a judgment: {reason:?}"
        );
    }
}
