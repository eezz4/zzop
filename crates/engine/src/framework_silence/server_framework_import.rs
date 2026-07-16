//! S2: server-framework import tripwire (provide side).

use std::collections::{BTreeMap, BTreeSet};

use super::controller_silence::MIN_PROVIDES_FLOOR;

/// Server-framework package specifiers whose route-registration idiom is typically a runtime METHOD CALL
/// (`app.get(...)`, `router.post(...)`) rather than a decorator — invisible to `controller_decorator_re`
/// above. Deliberately server frameworks ONLY: an HTTP CLIENT library (axios, got, ...) says nothing about
/// whether THIS tree serves routes, so including one here would false-positive on an ordinary FE tree.
const SERVER_FRAMEWORK_SPECIFIERS: &[&str] = &[
    "express",
    "koa",
    "fastify",
    "@hapi/hapi",
    "restify",
    "polka",
    "@nestjs/core",
    "@nestjs/common",
    "hono",
    "@trpc/server",
    "fastapi",
    "flask",
    "django",
    "axum",
    // `actix-web`'s own manifest `name` is hyphenated, but every `use` specifier this census entry is
    // matched against carries the crate's IMPORT spelling (`use actix_web::...` — Rust `use` paths
    // cannot contain `-`), same as `package_import_files`' own census keys throughout this engine.
    "actix_web",
    "rocket",
    "warp",
    // Go server frameworks — the FULL import path, verbatim (the Go census's own grain, see
    // `collect::census::drain_go_candidates`'s doc: a Go import path never carries an item-level suffix
    // past the package itself, so — unlike Rust's crate-head-only census — the vocab entry here IS the
    // exact census key a real import produces). The npm slash-subpath arm below already covers a
    // version-suffixed import path for free (`"github.com/gofiber/fiber/v2"` still matches the
    // `"github.com/gofiber/fiber"` entry, same mechanism `"express/lib/router"` matching `"express"`
    // uses) — no matcher change needed, verified before adding these entries.
    "github.com/gin-gonic/gin",
    "github.com/labstack/echo",
    "github.com/go-chi/chi",
    "github.com/gofiber/fiber",
    // Spring MVC (Java) — natively supported (`extract_http_provides`/`extract_http_provides_project`
    // resolve `@RestController`/`@Controller` route registrations for real), but this entry stays: a tree
    // that imports `org.springframework.*` yet extracted near-zero http provides is still exactly the S2
    // signal — a controller shape this pass's own vocabulary doesn't cover (a functional/lambda
    // `RouterFunction` bean, a WebFlux annotation this crate doesn't recognize yet, ...). Zero-extraction
    // disclosure stays the honest floor even for a natively-supported framework, same reasoning gin's own
    // entry above already establishes. Census GRAIN note: Java's own F5 drain censuses at the
    // first-TWO-dotted-segments grain (`collect::census::drain_java_candidates`'s doc), so the census key
    // a real unresolved Spring import produces is ALWAYS exactly `"org.springframework"` — this entry
    // matches it via the plain `specifier == *vocab` arm, no subpath arm needed for the Java case (the
    // `.`-subpath arm below still fires defensively if a future change censuses a longer specifier).
    "org.springframework",
];

/// Whether `specifier` names one of `SERVER_FRAMEWORK_SPECIFIERS`, exact-segment matched: the specifier
/// itself equals the vocab entry, or is a subpath import of it, in the npm slash-subpath form
/// (`"express/lib/router"` still counts as `express`), the Python dotted-subpath form (`"fastapi.routing"`
/// still counts as `fastapi` — `from fastapi.routing import ...` arrives as specifier `fastapi.routing` per
/// `zzop_parser_python_3::lang::imports`' absolute-dotted-specifier convention), or the Rust `::`-subpath form
/// (`"axum::routing"` still counts as `axum`). In practice `package_import_files`' Rust census entries are
/// always the bare crate head (`collect::collect`'s staging censuses `rust_head(specifier)`, never the full
/// path), so the `::` arm never fires against a real census entry today — kept for defensive symmetry with
/// the npm/Python arms and so a future full-specifier census change stays correctly matched without also
/// having to remember this function. Deliberately NOT a substring match — every vocab entry here is already
/// a whole, exact package identity (unlike `sdk_import_no_visible_consume`'s fragment vocab, e.g.
/// `"sdk"`/`"openapi"`, which needs a real anchored regex to bound a free-form name), so a plain
/// equals-or-prefix check is the exact-segment-boundary equivalent without the regex overhead.
fn is_server_framework_specifier(specifier: &str) -> bool {
    SERVER_FRAMEWORK_SPECIFIERS.iter().any(|vocab| {
        specifier == *vocab
            || specifier.starts_with(&format!("{vocab}/"))
            || specifier.starts_with(&format!("{vocab}."))
            || specifier.starts_with(&format!("{vocab}::"))
    })
}

/// Sources the cross-layer join is provide-BLIND to: a tree that imports a server framework
/// (`is_server_framework_specifier`) yet extracted fewer than `MIN_PROVIDES_FLOOR` http provides — the
/// S2 tripwire condition, lifted to a reusable set. The provide-side analog of
/// `zzop_rules_cross_layer::cross_layer::majority_unresolved_http_sources` (consume-blind): when such a
/// source exists, a confident "no provider anywhere" verdict cannot be trusted, since the provider may
/// live in the blind tree. Single definition shared by the S2 warning (`server_framework_import_warning`,
/// per-tree self-report) and `cross-layer/unprovided-mutation-call`'s severity gate (run-wide, across every
/// tree in this analysis).
///
/// Qualification mirrors S2 EXACTLY: a source qualifies iff (a) at least one of its `package_imports`
/// specifiers is [`is_server_framework_specifier`], AND (b) its http provide count is `< MIN_PROVIDES_FLOOR`
/// (same floor, same strict-less-than comparison). `http_provide_counts` must carry an entry for every
/// source in this run, including sources with 0 http provides — a framework-importer with 0 provides is
/// the most blind case, and omitting its entry would only be safe by accident (this function treats a
/// missing entry as 0 anyway, defensively).
///
/// Returns a `BTreeSet` for determinism — output must be byte-stable across platforms/iteration order,
/// same convention as `majority_unresolved_http_sources`.
pub fn provide_blind_sources(
    package_imports: &[zzop_rules_cross_layer::PackageImportSite],
    http_provide_counts: &[(String, usize)],
) -> BTreeSet<String> {
    let framework_sources: BTreeSet<&str> = package_imports
        .iter()
        .filter(|p| is_server_framework_specifier(&p.specifier))
        .map(|p| p.source.as_str())
        .collect();
    if framework_sources.is_empty() {
        return BTreeSet::new();
    }
    let counts: BTreeMap<&str, usize> = http_provide_counts
        .iter()
        .map(|(source, count)| (source.as_str(), *count))
        .collect();
    framework_sources
        .into_iter()
        .filter(|source| counts.get(source).copied().unwrap_or(0) < MIN_PROVIDES_FLOOR)
        .map(str::to_string)
        .collect()
}

/// Returns a ready-to-push `warnings` entry when at least one server-framework package (see
/// `SERVER_FRAMEWORK_SPECIFIERS`) is imported anywhere in the tree while `http_provides_count` sits below
/// `MIN_PROVIDES_FLOOR`. Pure map lookup — no disk IO, so this is cheap on every tree regardless of
/// outcome.
///
/// Determinism: `package_import_files` is a `BTreeMap<specifier, BTreeSet<importing file>>` (both levels
/// already sorted), so iteration order and the first-example-file pick are both deterministic without any
/// extra sort here.
pub fn server_framework_import_warning(
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    http_provides_count: usize,
) -> Option<String> {
    if http_provides_count >= MIN_PROVIDES_FLOOR {
        return None;
    }
    let mut matched: Vec<(&str, usize, &str)> = Vec::new();
    for (specifier, files) in package_import_files {
        if !is_server_framework_specifier(specifier) {
            continue;
        }
        let Some(example) = files.iter().next() else {
            continue;
        };
        matched.push((specifier.as_str(), files.len(), example.as_str()));
    }
    if matched.is_empty() {
        return None;
    }
    let spec_list = matched
        .iter()
        .map(|(specifier, count, example)| format!("{specifier} ({count} file(s), e.g. {example})"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "server-framework package(s) imported but only {http_provides_count} http route(s) were extracted \
tree-wide: {spec_list} — the registration idiom may be a runtime method call (e.g. `router.get(...)`, \
`app.post(...)`) rather than a decorator, which this extraction pass does not yet recognize; cross-layer \
joins will be near-silent for this tree — project this tree's routes with a Mode B overlay adapter (see \
the adapter examples) to restore cross-layer visibility: a partial envelope covering just the provide \
channel is enough; contract: `zzop-mcp contract envelope-guide` on MCP hosts, docs/NORMALIZED_AST.md in \
the repo."
    ))
}
