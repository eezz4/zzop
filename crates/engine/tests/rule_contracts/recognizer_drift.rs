//! The anti-drift seal for `zzop_engine::framework_recognizers`.
//!
//! A capability disclosure that can silently fall behind the code it describes is worse than none: it
//! converts "we do not know" into a confident wrong answer. The declarations live in each parser
//! crate's `lib.rs` (`FRAMEWORK_RECOGNIZERS`), while the recognizers themselves are MODULES on disk.
//! Nothing in the compiler binds the two — adding `parser/parser-go/src/adapters/echo.rs` and wiring
//! it up compiles perfectly with no declaration, and the disclosure would then state that zzop does
//! not know Echo while shipping an Echo recognizer.
//!
//! So this file crosses the declared set against the recognizer modules actually on disk, in BOTH
//! directions, with an explicit exemption list for the modules that are mechanisms rather than
//! frameworks. Same shape as `catalog_sync`'s sightline set-equality test, and for the same reason.
//!
//! # What this guard does NOT bind (stated, because a guard's silence gets read as coverage)
//! What IS bound here is the module axis, in both directions, over a subject set derived from disk per
//! crate (see [`recognizer_root`]). One axis is bound NEXT DOOR and one is still open:
//! - a row's CHANNEL set (`hono` declared only `io.consumes` while `router_mounts` fed its provides) is
//!   now the sibling contract `recognizer_channels`'s subject — and it reads the REASON strings in
//!   [`NOT_A_FRAMEWORK`] to learn which row each exempted module backs, so a reason that names no row
//!   is not merely unhelpful there, it drops that module's io evidence on the floor;
//! - the client VOCABULARY *inside* a declared module is still unguarded (`ky`, `$fetch`, `angular`,
//!   the generated-SDK families and python's `requests` all lived inside an already-declared module
//!   with no row of their own). That one is named where it can be acted on — in the parser crates'
//!   declarations — rather than guessed at here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Modules that refine or resolve what a framework row already found (or are language-layer plumbing
/// under a crate whose recognizer root IS its `src/`), rather than recognizing a framework of their
/// own. Each entry needs a reason, because an unexplained exemption is how a missing declaration
/// hides. Keyed `parser/module`.
/// `pub(crate)` because the sibling channel contract (`recognizer_channels`) reads the same table: the
/// REASON strings are where a module states which framework row(s) it backs, and that mapping is the
/// only link between a recognizer module and the `emits` set declared for it.
pub(crate) const NOT_A_FRAMEWORK: &[(&str, &str)] = &[
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
    // `parser-typescript/pathname_dispatch` was exempted here until 2026-08-01 as a "route-shape
    // heuristic shared by several framework rows". Both halves were false: no other row reads it, and
    // it mints its own `io.provides` off its own per-function evidence gates. It now carries the
    // `pathname dispatch` row instead — an exemption whose REASON is wrong is worse than a missing one,
    // because the reason is what the next reader trusts instead of re-deriving.
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
        "same, for generated SDK clients — whose row is `openapi generated client` (this reason said \
         `axios`/`fetch` by inheriting the line above until 2026-08-01, naming a row that did not \
         cover the client it excused)",
    ),
    (
        "parser-typescript/router_mounts",
        "mount composition; the framework rows are `express` and `hono` — this module is the PROVIDE \
         side of both vocabularies",
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
        "declared as the `axios`/`fetch`/`ky`/`$fetch`/`angular`/`openapi generated client` rows — SIX \
         client vocabularies in one module, which is why this reason lists them instead of saying \
         `axios`/`fetch` and leaving four undisclosed (as it did until 2026-08-01)",
    ),
    (
        "parser-python-3/guard_vocab",
        "the shared auth-guard NAME vocabulary the `fastapi` and `django` guard producers judge \
         against — no framework of its own, but part of those rows' auth-evidence seam (its type is \
         what the engine's guard gate takes as the declared-vocabulary parameter)",
    ),
    (
        "parser-python-3/django_routes",
        "the framework row is `django`",
    ),
    (
        "parser-python-3/http_clients",
        "declared as the `httpx` AND `requests` rows — one module, two client vocabularies behind the \
         same import gate",
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
    // --- crates whose recognizer root is their own `src/` (no `adapters/` directory) ---
    // These three contributed ZERO subjects until 2026-08-01, when the scan root stopped being
    // `src/adapters/` only. Their language-layer modules are the price of that widening, and each is
    // exempted on what it does, not on where it sits.
    (
        "parser-java-21/provides",
        "Spring MVC route provides (`@RestController` + `@GetMapping`/... annotation vocabulary); \
         declared as the `spring` row, which is spelled after the framework rather than the module",
    ),
    (
        "parser-java-21/http_clients",
        "declared as the `resttemplate` AND `webclient` rows — one module, two Spring client \
         vocabularies, each behind its own import gate (the same one-module-several-rows shape \
         parser-python-3's http_clients carries for httpx/requests)",
    ),
    (
        "parser-java-21/project",
        "whole-project composition over the `spring` row's per-file provides — resolves constant path \
         references across files; recognizes no framework of its own",
    ),
    (
        "parser-java-21/lang",
        "language layer (symbols, imports, calls, used names) — the substrate every recognizer reads, \
         and itself no io channel's producer",
    ),
    (
        "parser-java-21/node_kinds",
        "test-only pin of the tree-sitter grammar's node-kind names, so a grammar upgrade that renames \
         a kind fails loudly; extracts nothing",
    ),
    (
        "parser-java-21/util",
        "annotation/text helpers shared by the modules above; no framework, no channel",
    ),
    (
        "parser-prisma/analysis",
        "the schema-IR -> Common IR bridge; declared as the `prisma schema` row, which this crate \
         carries as its whole reason to exist",
    ),
    (
        "parser-prisma/parse",
        "lexical PSL parsing — the text-to-schema-IR half of the `prisma schema` row, with no \
         recognition decision of its own",
    ),
    (
        "parser-sql/extract",
        "`CREATE TABLE` table provides; declared as the `sql ddl` row",
    ),
    (
        "parser-sql/consume",
        "the DML half of the same SQL vocabulary, called by `parser-typescript`'s `raw_sql` (declared \
         THERE as the `raw sql` row) — it recognizes statement shapes, never a framework",
    ),
];

/// Module-name ↔ framework-row bindings that hold by JUDGMENT rather than by spelling. Direction 1
/// requires a module's folded name to EQUAL a declared row's folded name — equality, because the
/// bidirectional substring `contains` this file used until 2026-08-03 let a module declare ITSELF:
/// any module whose name happened to ride inside a row's name (a stray `client.rs` would have counted
/// as `openapi generated client`'s) passed with no row and no exemption of its own, which is exactly
/// the silent-coverage hole this guard exists to close. A pair the fold cannot equate is bound here
/// instead, each with the judgment written down — the same discipline as [`NOT_A_FRAMEWORK`], and
/// staleness-guarded the same way ([`no_alias_names_a_module_or_row_that_no_longer_exists`]).
const MODULE_ROW_ALIASES: &[(&str, &str, &str)] = &[
    (
        "parser-go/net_http",
        "net/http",
        "the pair the bidirectional `contains` was originally loosened FOR — a row spelled as an \
         import path (`net/http`) where a module can only spell an identifier (`net_http`). The \
         fold's separator stripping equates the two, and the module also carries a NOT_A_FRAMEWORK \
         entry naming its row, so this entry is the pairing STATED rather than load-bearing today; \
         it stays because this pair is the recorded cause of the looseness this map replaced, and \
         the staleness test surfaces it if either side goes away",
    ),
    (
        "parser-java-21/security",
        "spring security",
        "the one binding the equality switch actually severed: this module is the method-security \
         ANNOTATION half of the `spring security` row (`@PreAuthorize`/`@Secured`/`@RolesAllowed` \
         guard extraction; `spring_security` beside it is the global-posture half and folds to the \
         row exactly), and its shortened name reached the row only through \
         `\"springsecurity\".contains(\"security\")` — the same containment arm a stray short-named \
         module could ride, so the binding is stated here instead",
    ),
];

/// Where ONE parser crate's recognizers live, derived from that crate's own layout instead of assumed
/// for all of them: `src/adapters/` when the crate has that directory, its own `src/` when it does not.
/// Both shapes are real — five crates group recognizers under `adapters/`, while `parser-java-21` keeps
/// `provides/`, `security.rs` and `spring_security.rs` at the crate root, and `parser-prisma`/
/// `parser-sql` are single-recognizer crates with no such directory at all.
///
/// This function is the fix for a measured hole. The scan used to be `src/adapters/` ONLY, with a
/// `continue` for any crate lacking one — so java-21 contributed **zero** subjects and an undeclared
/// JAX-RS recognizer could have shipped with this file green, while the `continue` made that silence
/// look like a deliberate allowance for "structurally clean" crates. A subject set smaller than the
/// guard's topic answers "covered" when it means "never looked at" (working-agreements §5.5).
pub(crate) fn recognizer_root(crate_dir: &Path) -> PathBuf {
    let adapters = crate_dir.join("src/adapters");
    if adapters.is_dir() {
        adapters
    } else {
        crate_dir.join("src")
    }
}

/// Every recognizer module on disk, per parser crate. A module is one Rust module UNIT under that
/// crate's [`recognizer_root`] — `foo.rs` and `foo/` are the same `foo`, hence the inner set.
pub(crate) fn recognizer_modules_by_crate() -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    let parsers = std::fs::read_dir(repo().join("parser")).expect("parser/ is readable");
    for p in parsers.flatten() {
        let name = p.file_name().to_string_lossy().to_string();
        if !name.starts_with("parser-") {
            continue;
        }
        let root = recognizer_root(&p.path());
        // Unreadable is ABORT, never skip: a crate contributing no subject is exactly the vacuous
        // green this file exists to prevent, so it must not be reachable by a filesystem accident.
        let entries = std::fs::read_dir(&root).unwrap_or_else(|e| {
            panic!(
                "{name}: recognizer root {} is unreadable ({e}) — refusing to scan on, because a \
                 crate that contributes zero subjects makes this guard vacuously green for that \
                 whole language",
                root.display()
            )
        });
        let mut modules = BTreeSet::new();
        for e in entries.flatten() {
            let f = e.file_name().to_string_lossy().to_string();
            let module = f.strip_suffix(".rs").unwrap_or(&f).to_string();
            // Crate/module plumbing and test halves are not recognizer units.
            let plumbing = module == "mod" || module == "lib";
            let test_half = module == "tests" || module.ends_with("_tests");
            if plumbing || test_half {
                continue;
            }
            modules.insert(module);
        }
        out.insert(name, modules);
    }
    out
}

/// The flattened subject set, `parser-<lang>/<module>` — the keying `NOT_A_FRAMEWORK` uses.
fn recognizer_modules() -> BTreeSet<String> {
    recognizer_modules_by_crate()
        .into_iter()
        .flat_map(|(krate, modules)| modules.into_iter().map(move |m| format!("{krate}/{m}")))
        .collect()
}

/// THE FLOOR, per crate rather than per repo. A single global "not empty" assertion is satisfied by
/// one crate's modules while another contributes nothing — which is precisely the state this file
/// shipped in (typescript's 20 subjects kept it green while java-21 contributed 0). Each parser crate
/// must put at least one subject on the table, and the crate list itself must not shrink.
#[test]
fn every_parser_crate_contributes_at_least_one_recognizer_subject() {
    let by_crate = recognizer_modules_by_crate();
    assert!(
        by_crate.len() >= 8,
        "found {} parser crate(s) {:?} — this workspace ships 8, so a shorter list means the scan lost \
         a whole language rather than that a language left (if a parser really was removed, lower this \
         floor in the same commit that removes it)",
        by_crate.len(),
        by_crate.keys().collect::<Vec<_>>()
    );
    for (krate, modules) in &by_crate {
        assert!(
            !modules.is_empty(),
            "{krate} contributes zero recognizer subjects — its recognizers are somewhere \
             `recognizer_root` does not look, so every check below is vacuously green for it"
        );
    }
}

/// DIRECTION 1 — every recognizer module is accounted for: it either backs a declared framework row or
/// carries an exemption saying why it is not a framework. This is the direction that catches the real
/// hazard: shipping a recognizer nobody disclosed.
#[test]
fn every_adapter_module_is_declared_or_explicitly_exempt() {
    let exempt: BTreeSet<&str> = NOT_A_FRAMEWORK.iter().map(|(m, _)| *m).collect();
    let modules = recognizer_modules();
    assert!(
        !modules.is_empty(),
        "found zero recognizer modules — the scan root is wrong, and an empty subject set would make \
         this whole test vacuously green (the exact failure this repo's guard-coverage rule exists for)"
    );
    let undeclared: Vec<&String> = modules
        .iter()
        .filter(|m| !exempt.contains(m.as_str()))
        .filter(|m| {
            // A module counts as declared when some row's framework name EQUALS it after folding the
            // separators the two vocabularies spell differently (`net/http` vs `net_http`), or when
            // [`MODULE_ROW_ALIASES`] binds the pair by hand. Equality, not the bidirectional substring
            // `contains` this closure held until 2026-08-03: containment let a module whose name rode
            // inside a row's name self-declare with no row and no exemption. Keep the ones that match
            // NOTHING — those are the undeclared recognizers this test hunts.
            let rows = zzop_engine::framework_recognizers();
            let fold = |s: &str| {
                s.to_ascii_lowercase()
                    .replace(['-', '_', '/', '.', ' '], "")
            };
            let name = fold(m.rsplit('/').next().unwrap_or(m));
            let equal = rows.iter().any(|r| fold(r.framework) == name);
            // An alias only counts while its row is really declared — a row that leaves the
            // disclosure must take its aliased module back to undeclared-red, not stay green on a
            // leftover map entry.
            let aliased = MODULE_ROW_ALIASES.iter().any(|(module, row, _)| {
                *module == m.as_str() && rows.iter().any(|r| r.framework == *row)
            });
            !(equal || aliased)
        })
        .collect();
    assert!(
        undeclared.is_empty(),
        "recognizer module(s) with neither a FRAMEWORK_RECOGNIZERS row nor a NOT_A_FRAMEWORK exemption: \
         {undeclared:?}\n\
         A shipped recognizer that nobody declared makes the capability disclosure state that zzop \
         does not know a framework it does know. Declare it in that parser's lib.rs, exempt it WITH \
         a reason, or — when module and row are one recognizer under two spellings the fold cannot \
         equate — bind the pair in MODULE_ROW_ALIASES with the judgment spelled out."
    );
}

/// DIRECTION 2 — no exemption outlives the module it excuses. A stale entry here would silently
/// re-open direction 1 for a module that later comes back under the same name.
#[test]
fn no_exemption_names_a_module_that_no_longer_exists() {
    let modules = recognizer_modules();
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

/// The alias map obeys the same two-direction staleness discipline as the exemptions: an alias whose
/// MODULE is gone would silently re-arm for a future module of the same name, and an alias whose ROW
/// is gone would (but for the row-existence check in direction 1, which this test keeps honest) count
/// a shipped recognizer as declared against a disclosure that no longer mentions it.
#[test]
fn no_alias_names_a_module_or_row_that_no_longer_exists() {
    let modules = recognizer_modules();
    let rows: BTreeSet<&str> = zzop_engine::framework_recognizers()
        .iter()
        .map(|r| r.framework)
        .collect();
    for (module, row, reason) in MODULE_ROW_ALIASES {
        assert!(
            reason.len() > 15,
            "{module}'s alias reason is too thin to be a judgment: {reason:?}"
        );
        assert!(
            modules.contains(*module),
            "MODULE_ROW_ALIASES binds module {module:?}, which is no longer on disk — delete the entry"
        );
        assert!(
            rows.contains(row),
            "MODULE_ROW_ALIASES binds {module:?} to row {row:?}, which no parser declares — the \
             module lost its row, so declare it, re-alias it, or exempt it"
        );
    }
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
