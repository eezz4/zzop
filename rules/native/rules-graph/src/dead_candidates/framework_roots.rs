//! Framework APP ROOTS, and the paths a framework loads from underneath one.
//!
//! Held apart from [`crate::unreachable::patterns`] because it answers a different KIND of question.
//! That module owns pure path SHAPES — a regex decides on its own, from one string, whether a file is
//! a tool entry. Every predicate here needs a second fact no shape can carry: whether some OTHER file
//! in the same tree DECLARES the directory above this one to be an app root. `pages/` is a Next.js
//! router only when a `next.config.*` sits beside it, and an Astro route directory only when an
//! `astro.config.*` does. Without that half a directory name is just a directory name — MEASURED
//! across 13 trees, `pages/`-, `plugins/`-, `middleware/`- and `composables/`-named directories that
//! no framework routes hold 35 findings that must keep reporting (grafana 21, nest 6, nocodb 6,
//! typeorm 1, cal.com 1).
//!
//! **Why a config file is admissible evidence for an ERASING direction** (`rule-quality.md` §24 —
//! suppression needs a declaration, never an inference). The author wrote `next.config.ts` /
//! `astro.config.mjs` at that directory, and the framework then resolves its routing directories
//! relative to exactly that file. The pairing is not a guess about who owns a folder; it is the same
//! declaration the build itself reads. That is the grade §31 required of the locale exemption
//! (container plus tag, neither half sufficient), and the grade
//! `unreachable::patterns`'s `.storybook/copyAssets.ts` note records as MISSING when a directory's
//! owner was merely inferred. This repo already treats a framework config that way for this very
//! rule: `zzop_engine`'s `analyze::assemble::nuxt_auto_import::scan` anchors Nuxt's auto-import
//! directories on a `nuxt.config.*` and refuses to resolve anything without one.
//!
//! **What is deliberately NOT here, each with its measured harvest** (`rule-quality.md` §26 ③ — a
//! widening direction whose extra reach is measured at zero buys only false negatives, so it is
//! refused rather than shipped untested):
//! - Next.js `middleware.{ts,js}` at an app root — **0** findings across 13 trees.
//! - SvelteKit `src/routes/**` under a `svelte.config.*` — **0** (immich's SvelteKit tree reports no
//!   `dead-candidates` at all, and `+page`/`+server`/`hooks.{server,client}` are already carried by
//!   `unreachable::patterns::framework_route_patterns`).
//! - Nuxt `pages/`/`layouts/`/`middleware/` under a `nuxt.config.*` — **0** (nocodb's `nc-gui` IS a
//!   Nuxt app and none of its findings sit in those directories). Nuxt's `composables/` is NOT a
//!   missing row either: that class is resolved a better way already, engine-side, by RESOLVING the
//!   bare-name reference (`nuxt_auto_import::scan` → `merge_auto_import_fan_in`) rather than by
//!   exempting a directory. nocodb's 6 surviving `composables/` findings are the genuinely
//!   unreferenced composables that mechanism deliberately keeps — verified 2026-08-26, zero
//!   references to any of the six anywhere in the app. A directory exemption here would erase them.
//! - Next.js App Router (`app/**/{page,layout,route,…}`) — **0** beyond what
//!   `framework_route_patterns` already exempts. cal.com carries 79 `page.tsx` and 39 `route.ts`
//!   under `apps/web/app/` and reports on NONE of them; its single `app/` finding is
//!   `…/members/actions.ts`, which is NOT a path convention — a `"use server"` module becomes an
//!   endpoint only once a component imports it and the bundler mints an action id, so zero importers
//!   there is a real orphan and it must keep reporting.
//!
//! Two shapes DO fire, are genuinely of this class, and are still refused here because this anchor
//! cannot honestly reach them — the finding message carries them instead:
//! - Nextra's `_meta.*` (cal.com `apps/docs/content/`, **3**). Nextra is a PLUGIN reading a
//!   CONFIGURABLE content directory, so the `next.config.mjs` above it declares Next and says nothing
//!   about Nextra. Reading the second off the first is the exact inference §24 forbids, and
//!   `unreachable::patterns` already records what that costs.
//! - A Docusaurus doc route (typeorm `docs/src/pages/maintainers.tsx`, **1**), whose anchor would be
//!   `docusaurus.config.ts`. A third framework config with a harvest of one is a row bought at the
//!   price of a permanent claim; the message is the cheaper carrier at this size. It becomes worth
//!   adding when a measured tree makes it more than one.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use zzop_core::FileNode;

/// The app roots one tree declares, scanned once per run and then asked about every candidate.
///
/// Borrows the node paths rather than copying them: the scan is over the same slice the caller is
/// already filtering, and a tree declares a handful of roots (cal.com, the widest measured, has 4).
pub(super) struct FrameworkRoots<'a> {
    next: HashSet<&'a str>,
    astro: HashSet<&'a str>,
}

impl<'a> FrameworkRoots<'a> {
    /// Collects the directory of every framework config file in `nodes`.
    ///
    /// Deliberately NOT root-anchored, same reason as `unreachable::patterns`'s middleware row: an
    /// app inside a monorepo lives below the analyzed root (`apps/web/`,
    /// `example-apps/credential-sync/`, `packages/platform/examples/base/` — all three real cal.com
    /// Next roots). The segment must START with `next.config.`, so cal.com's real
    /// `packages/i18n/next-i18next.config.js` declares nothing.
    pub(super) fn scan(nodes: &'a [FileNode]) -> Self {
        static NEXT: OnceLock<Regex> = OnceLock::new();
        static ASTRO: OnceLock<Regex> = OnceLock::new();
        let next = NEXT.get_or_init(|| {
            Regex::new(r"(?i)(^|/)next\.config\.(js|cjs|mjs|ts|mts|cts)$")
                .expect("next.config pattern is valid")
        });
        let astro = ASTRO.get_or_init(|| {
            Regex::new(r"(?i)(^|/)astro\.config\.(js|cjs|mjs|ts|mts|cts)$")
                .expect("astro.config pattern is valid")
        });
        Self {
            next: roots_declared_by(nodes, next),
            astro: roots_declared_by(nodes, astro),
        }
    }

    /// True when a framework this tree declares loads `path` FROM THE PATH — so nothing imports it
    /// and `fan_in == 0` is the convention working, not a dead file.
    ///
    /// Three shapes, all measured:
    /// - **Next.js Pages Router** (`<root>pages/**`, or `<root>src/pages/**`). Whole-directory
    ///   rather than a filename list, because Next's rule IS the directory: `_app`, `_document` and
    ///   `_error` are reserved names and every OTHER file there becomes a URL. MEASURED (cal.com,
    ///   2026-08-26, full 334-finding enumeration): 62 findings, among them the 29
    ///   `pages/api/trpc/*/[trpc].ts` handlers serving every tRPC namespace the web app calls,
    ///   `pages/api/auth/[...nextauth].ts` (all login/session/OAuth), `pages/api/book/event.ts`, and
    ///   five payment webhooks. The prescription this rule prints — "Delete the file if it is
    ///   genuinely unused" — 404s each of them. The accepted false negative is a genuinely dead
    ///   helper someone colocated under `pages/`; Next discourages that precisely because such a
    ///   file becomes a reachable URL, so the miss is small and its subject is already being served.
    /// - **Astro routes** (`<root>src/pages/**`, and only that directory). A second framework is not
    ///   decoration: a gate measured on one tree is a gate shaped by that tree. MEASURED
    ///   (`corpus/frameworks/astro`): 11 findings across five example/benchmark projects, each
    ///   carrying its own `astro.config.*`.
    /// - **Next.js instrumentation hooks** (`<root>instrumentation{,-client}.*`, or under `src/`),
    ///   read by exact filename from the app root. Name PLUS path, the same pairing
    ///   `dead_exports::is_middleware_convention_file` uses and for the same reason — the word is
    ///   generic enough that a deeper `lib/instrumentation.ts` must keep reporting. MEASURED
    ///   (cal.com): 2 findings, both named by the 2026-08-21 blind audit as false positives of
    ///   exactly this class.
    pub(super) fn loads_by_path(&self, path: &str) -> bool {
        self.is_under_router_dir(path, &self.next)
            || self.is_under_router_dir(path, &self.astro)
            || self.is_next_instrumentation_file(path)
    }

    /// `path` sits under a `pages/` directory belonging to one of `roots`.
    ///
    /// Every occurrence of `pages/` is tried, not just the first, because a routed file can nest a
    /// same-named directory below itself. The anchor is the WHOLE prefix and never a suffix of one,
    /// which is what keeps cal.com's genuinely dead
    /// `packages/app-store/paypal/pages/setup/_getStaticProps.tsx` reporting while
    /// `apps/web/pages/...` goes silent — both prefixes end in the same six characters.
    fn is_under_router_dir(&self, path: &str, roots: &HashSet<&str>) -> bool {
        let mut from = 0;
        while let Some(rel) = path[from..].find("pages/") {
            let idx = from + rel;
            let at_component_boundary = idx == 0 || path.as_bytes()[idx - 1] == b'/';
            if at_component_boundary && declares_root(&path[..idx], roots) {
                return true;
            }
            from = idx + 1;
        }
        false
    }

    fn is_next_instrumentation_file(&self, path: &str) -> bool {
        static RE: OnceLock<Regex> = OnceLock::new();
        let named = RE
            .get_or_init(|| {
                Regex::new(r"(?i)(^|/)instrumentation(-client)?\.(js|jsx|ts|tsx|mjs|cjs|mts|cts)$")
                    .expect("instrumentation filename pattern is valid")
            })
            .is_match(path);
        named && declares_root(dir_prefix_of(path), &self.next)
    }
}

/// The directory of every node whose path matches `re`, deduplicated.
fn roots_declared_by<'a>(nodes: &'a [FileNode], re: &Regex) -> HashSet<&'a str> {
    nodes
        .iter()
        .filter(|n| re.is_match(&n.path))
        .map(|n| dir_prefix_of(&n.path))
        .collect()
}

/// `prefix` is an app root, or is one plus the optional `src/` layout segment both frameworks accept.
fn declares_root(prefix: &str, roots: &HashSet<&str>) -> bool {
    roots.contains(prefix)
        || prefix
            .strip_suffix("src/")
            .is_some_and(|outer| roots.contains(outer))
}

/// The directory prefix a path lives in — `"a/b/c.ts"` → `"a/b/"`, `"c.ts"` → `""` (the analyzed
/// root). Keeps the trailing slash so a prefix concatenates without a separator decision, and so
/// `"apps/web/"` can never be confused with the sibling `"apps/web-legacy/"`.
fn dir_prefix_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "",
    }
}
