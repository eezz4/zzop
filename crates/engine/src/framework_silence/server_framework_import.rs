//! S2: server-framework import tripwire (provide side).

use std::collections::{BTreeMap, BTreeSet};

use super::controller_silence::MIN_PROVIDES_FLOOR;

mod visible_registrations;

pub(crate) use visible_registrations::{needs_visible_scan, visible_route_registrations};

/// Server-framework package specifiers whose route-registration idiom is typically a runtime METHOD CALL
/// (`app.get(...)`, `router.post(...)`) rather than a decorator — invisible to `controller_decorator_re`
/// above. Deliberately server frameworks ONLY: an HTTP CLIENT library (axios, got, ...) says nothing about
/// whether THIS tree serves routes, so including one here would false-positive on an ordinary FE tree.
///
/// **This list is INCOMPLETE by construction and its silence proves nothing.** A hand-typed vocabulary
/// leaves every framework outside it permanently green: gogs (`d460e50`) registers hundreds of routes
/// through `gopkg.in/macaron.v1`, which no entry here covers, so S2 stayed silent while 0 http provides
/// were extracted and none of that run's 27 warnings mentioned a route. The list is KEPT because it can
/// name the framework and the idiom, which a derivable signal cannot; what changed is that it no longer
/// presents itself as the floor — the emitted message says so, and the population-complete disclosure
/// lives on the coverage surface (`ioChannels.zeroExtraction`), keyed on the tree's structural extension
/// mix crossed with this build's own recognizer table rather than on any framework name. Adding a name
/// here is therefore an improvement to the NAMING, never a fix for a class of silence.
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
pub(super) fn is_server_framework_specifier(specifier: &str) -> bool {
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
/// Qualification is the S2 tripwire's FLOOR: a source qualifies iff (a) at least one of its
/// `package_imports` specifiers is [`is_server_framework_specifier`], AND (b) its http provide count is
/// `< MIN_PROVIDES_FLOOR`. `http_provide_counts` must carry an entry for every source in this run,
/// including sources with 0 http provides — a framework-importer with 0 provides is the most blind case,
/// and omitting its entry would only be safe by accident (this function treats a missing entry as 0
/// anyway, defensively).
///
/// 🔴 **And then the same one-directional measurement S2 uses, for the same reason** (2026-09-06,
/// review ledger V24 -> V49). `visible_by_source` carries each source's lexically-visible route
/// registration count, measured during that tree's own pass by
/// [`visible_route_registrations`] — the only place `root` and the candidate file list exist. A source
/// drops out of the blind set when `0 < visible <= extracted`: positive evidence that nothing was missed.
/// A source with no entry, or an entry of 0, keeps the floor's verdict, because an idiom the scan does not
/// know must not read as an absence of routes.
///
/// **Why this half mattered more than the warning half.** S2's false positive was PROSE a reader could
/// dismiss; this one is a SEVERITY decision nobody sees: an ordinary micro-BE — a framework import and one
/// or two routes, all of them extracted — used to make the whole run downgrade
/// `cross-layer/unprovided-mutation-call` from Warning to Info, on the theory that a provider might be
/// hiding in a blind tree when nothing was hidden. 📏 The repo was demonstrating it by accident: the
/// integration fixture `provide_blind_be_tree` qualified as blind on ONE perfectly-extracted `app.get`,
/// so the test suite was pinning the bug's own by-product.
///
/// Returns a `BTreeSet` for determinism — output must be byte-stable across platforms/iteration order,
/// same convention as `majority_unresolved_http_sources`.
pub fn provide_blind_sources(
    package_imports: &[zzop_rules_cross_layer::PackageImportSite],
    http_provide_counts: &[(String, usize)],
    visible_by_source: &BTreeMap<String, usize>,
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
        .filter(|source| {
            let extracted = counts.get(source).copied().unwrap_or(0);
            if extracted >= MIN_PROVIDES_FLOOR {
                return false;
            }
            // Same suppression as S2, same direction: only a positive measurement may clear a source.
            let visible = visible_by_source.get(*source).copied().unwrap_or(0);
            !(visible > 0 && visible <= extracted)
        })
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
///
/// ## The floor guesses; the lexical count MEASURES — and only the measurement may silence
///
/// `MIN_PROVIDES_FLOOR` asks "are there suspiciously few routes?", which on a real micro-BE is the wrong
/// question: 📏 a two-route Express tree with BOTH routes extracted still drew
/// *"only 2 http route(s) were extracted tree-wide … cross-layer joins will be near-silent"* — a
/// consequence the engine asserted while holding no evidence for it (2026-09-06, review ledger V24).
/// 📏 Measured the other way too: across the dogfood corpus this entry fires on **no tree at all**, so
/// its entire observed population was the one it is wrong about.
///
/// So the floor now only OPENS the question and [`visible_route_registrations`] answers it, by counting
/// the registration lines a reader can see in the same files the extractor was handed.
///
/// 🔴 **The suppression is one-directional on purpose.** It stays quiet only when
/// `0 < visible <= extracted` — positive evidence that nothing was missed. Every other outcome keeps the
/// old behavior, including `visible == 0`, which is where an idiom this regex does not know lands: not
/// knowing must never look like knowing there is nothing. An OVER-count can only make this fire (safe,
/// and the tuning this family already chose — "silence is fatal, over-disclosure is safe"); an
/// UNDER-count is the only thing that could silence a real gap, which is why the scan covers the whole
/// candidate set rather than the framework-importing files alone.
///
/// And when it does fire with a visible count, it stops guessing in the message too: it names the GAP
/// (*"19 route-registration lines are visible, 2 were extracted"*), which is the number a reader needs
/// and the old wording could never produce.
///
/// Cost: this reads `candidate_rels` from disk, so the "pure map lookup, no disk IO" property above now
/// holds only on the success path (`http_provides_count >= MIN_PROVIDES_FLOOR`, the ordinary case, which
/// returns before any of this). Below the floor, the sibling S1 has already re-read the same files —
/// measured there at ~69 µs/file — and this is the rare path by construction.
pub fn server_framework_import_warning(
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    http_provides_count: usize,
    visible: usize,
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
    if visible > 0 && visible <= http_provides_count {
        return None;
    }
    let gap = if visible > http_provides_count {
        format!(
            " {visible} route-registration line(s) are lexically visible in these files while              {http_provides_count} were extracted, so the gap is measured rather than guessed;"
        )
    } else {
        // `visible == 0`: no line matched the shape this scan knows. That is NOT evidence of no routes —
        // it is evidence that this tree's idiom is outside the scan — so the message says nothing about a
        // gap and the tripwire fires on the import alone, exactly as it did before the scan existed.
        String::new()
    };
    let spec_list = matched
        .iter()
        .map(|(specifier, count, example)| format!("{specifier} ({count} file(s), e.g. {example})"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "server-framework package(s) imported but only {http_provides_count} http route(s) were extracted \
tree-wide: {spec_list} — this extraction pass recognizes a limited set of registration shapes, and a tree \
whose idiom falls outside it reports zero here. That set excludes runtime method calls (e.g. \
`router.get(...)`, `app.post(...)`) AND decorator forms this pass does not know: measured on a Flask \
blueprint tree, 0 of 28 `@<blueprint>.route(...)` decorators were read. So do NOT read this as \
\"the idiom is not a decorator\" — that was this sentence's own wrong guess until it was measured;{gap} if those \
are all the routes this tree declares then nothing is missing and this entry does not apply to you; if they \
are not, cross-layer joins will be near-silent for this tree — project this tree's routes with a Mode B overlay adapter (see \
the adapter examples) to restore cross-layer visibility: a partial envelope covering just the provide \
channel is enough; contract: MCP resource `zzop://contract/envelope-guide` on MCP hosts (`zzop contract envelope-guide` with the CLI binary), docs/NORMALIZED_AST.md in \
the repo. The package vocabulary this entry matched on is HAND-KEPT and is not a complete list of \
server frameworks, so the absence of this warning never means a tree serves no routes — the \
disclosure whose population is the tree rather than a vocabulary is the coverage query's \
`ioChannels.zeroExtraction` cell, which names every language this build has a route extractor for \
that contributed zero routes, with no framework name involved."
    ))
}
