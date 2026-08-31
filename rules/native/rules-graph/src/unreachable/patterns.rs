//! Path-shape predicates behind `unreachable`, `dead-candidates` and `unimported-export` — the entry,
//! tool-entry, tool-CONFIG and framework-route pattern sets, each carrying the rationale for its own
//! boundary. Split out of `unreachable.rs` for the 300-line cap, along the seam that was already
//! there: the parent keeps the ANALYSIS (`find_unreachable` and its forward closure) and this module
//! keeps the VOCABULARY it judges with. One owner per set, which is the property
//! `is_tool_config_file`'s doc rests on.

use std::sync::OnceLock;

use regex::Regex;

/// Files loaded directly by a dev tool or `tsc` rather than imported by app code — so `fan_in == 0` on them
/// is "not the kind of file the import graph would ever point at", not a "no importers" signal (e.g.
/// `.eslintrc.cjs`, `vite.config.ts`, `vite-env.d.ts`; see `dead_candidates.rs`). Shared here so
/// `dead_candidates`/`dead_exports` don't each duplicate the pattern list.
pub(crate) fn is_tool_entry_file(path: &str) -> bool {
    tool_entry_patterns().iter().any(|re| re.is_match(path))
}

/// The tool-CONFIG subset of [`is_tool_entry_file`], public because it has a reader OUTSIDE the
/// exemption: a config is not only a file whose `fan_in == 0` is excused, it is the one place in a tree
/// that DECLARES which files a bundler loads, and `zzop_engine`'s `analyze::assemble::rules::
/// config_entries` harvests those declarations from exactly this set — so the set excused because a
/// tool loads them IS the set read for what that tool loads, and widening it cannot leave the harvest
/// behind.
///
/// **Widening it moves THREE CONSUMERS — two rules and the harvest — not one**, because the exemption
/// reader itself fans out: [`is_tool_entry_file`] is read by `dead_candidates` AND by
/// `dead_exports::is_entry_or_test`, so an admitted shape goes silent on `dead-candidates` and on
/// `unimported-export`, while the harvest moves the OTHER way and reads one more file. `unreachable`,
/// the rule this file is named for, is deliberately NOT among them: `find_unreachable` below asks
/// `is_entry_file`/`is_test_file`/`fan_in == 0` and never consults this predicate.
///
/// The silence is the intended answer on both rules — a config's `export default {…}` is consumed by
/// its tool — but it was MEASURED rather than assumed, in `dead_exports/tests.rs`, against a control
/// carrying a byte-equivalent export. `unimported-export` has its own precision record, and a widening
/// that moves it silently is a widening nobody priced.
///
/// ⚠ A cross-rule fixture for the qualifier shape must NOT use a `vite.`/`vitest.`/`jest.` stem —
/// `zzop_core::is_test_file` already carries those, so such a row proves nothing about this widening.
/// `dead_exports/tests.rs`'s pin owns that reasoning and the stem it uses instead.
///
/// Two shapes, both derived rather than enumerated per tool:
/// - `<name>.config[.<qualifier>].<ext>` — vite/vitest/jest/rollup/eslint/... A name is REQUIRED
///   before `.config`, so an ordinary `config.ts` module does not match. The qualifier segments in the
///   middle are what admits a SECOND config for the same tool (`vite.config.sw.js`,
///   `webpack.config.prod.js`) — a tool reaches those through its own `--config` flag, so nothing in
///   the tree imports them and nothing else would name them. Measured on koel 3f5213d4, where the
///   service worker's entire build — its config, its entry source, and its output — hung off
///   `vite.config.sw.js`, which the fixed-shape pattern could not see.
/// - `config.<ext>` sitting DIRECTLY inside a dot-directory — VitePress's `docs/.vitepress/config.mts`,
///   Storybook's `.storybook/config.js`. A leading dot marks a directory its tool owns, and the `config`
///   stem is what makes the file a DECLARATION SOURCE rather than merely tool-owned.
///
/// **Why the `config` stem survives here and not in the exemption.** On 2026-08-21 the exemption side
/// widened to every file at depth 1 inside a dot-directory, because `.storybook/main.mjs` was being
/// reported as dead and Storybook simply does not name its files `config`. Doing that HERE as well was
/// wrong and was caught in review: this predicate feeds the harvest, whose question is not "does a tool
/// own this file" but "does this file DECLARE which sources a build loads". A `.storybook/copyAssets.ts`
/// is tool-owned and is not a declaration — and it is a real file:
/// `corpus/frameworks/grafana/packages/grafana-ui/.storybook/copyAssets.ts` names
/// `'../src/components/Icon/utils.ts'` in a string, which the widened harvest read as an entry
/// declaration and exempted from `dead-candidates` on the strength of an INFERENCE about the
/// directory's owner. That is `rule-quality.md` §24's erasing direction with no declaration under it.
///
/// So the two readers now hold DIFFERENT sets on purpose, and [`tool_entry_patterns`] carries the wider
/// one. The sentence above — "the set excused because a tool loads them IS the set read for what that
/// tool loads" — was true while one shape served both questions and stopped being true the moment they
/// diverged; a predicate with two consumers is only one predicate for as long as both consumers want
/// the same answer.
///
/// **What this set deliberately does not claim**: that every file under a dot-directory declares
/// anything. `docs/.vitepress/theme/index.ts` is out on depth, `.storybook/main.mjs` is out on stem, and
/// both are still EXEMPT through [`is_tool_entry_file`] — exempt and harvested are different questions
/// with different answers, which is the whole point of the split.
pub fn is_tool_config_file(path: &str) -> bool {
    tool_config_patterns().iter().any(|re| re.is_match(path))
}

/// The two DECLARATION-SOURCE shapes [`is_tool_config_file`] documents — the set whose TEXT is read.
/// Held apart from [`tool_entry_patterns`] (which extends itself with these and adds its own wider
/// dot-directory row) so each question has one owner.
///
/// The extension set matches `dead_candidates::is_ts_dispatch_extension` — case-insensitive, and
/// carrying `jsx`/`tsx`. It did not until 2026-08-21: a `.storybook/preview.jsx` fell outside a
/// tool-file predicate whose own rationale named that very file, because the candidacy population is
/// `(?i)(ts|tsx|js|jsx|mjs|cjs|mts|cts)` and this list was a narrower, case-sensitive copy. A predicate
/// that exempts from a population must be able to reach every member of it.
pub(super) fn tool_config_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"(?i)(^|/)[^/]+\.config(\.[^/.]+)*\.(js|jsx|ts|tsx|mjs|cjs|mts|cts)$",
            r"(?i)(^|/)\.[^/]+/config\.(js|jsx|ts|tsx|mjs|cjs|mts|cts)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

/// Next.js App Router convention files — `app/**/{page,layout,route,error,not-found,…}.tsx` plus the
/// metadata routes (`sitemap`/`robots`/`manifest`/`opengraph-image`/…). The framework loads these by
/// filename, never through an import, so zero in-repo importers is expected — not a dead signal. Shared
/// here so `dead_candidates` and `dead_exports` reference ONE convention set and cannot drift (they did:
/// `dead_exports` carried this set while `dead_candidates` was missing it entirely).
pub(crate) fn framework_route_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"(^|/)(page|layout|loading|error|global-error|not-found|template|default|route)\.(ts|tsx)$",
            r"(^|/)(sitemap|robots|manifest|opengraph-image|twitter-image|icon|apple-icon)\.(ts|tsx)$",
            // SvelteKit route/hook convention files — `load`/`actions` (in `+page(.server)`/
            // `+layout(.server)`), `handle`/`handleError`/`handleFetch` (in `hooks.{server,client}`), and
            // `GET`/`POST`/… (in `+server`) are invoked by SvelteKit by EXACT name via its file-based
            // routing + hooks contract, never through an in-repo import — so the import graph shows zero
            // importers and they read as dead/unreachable. Whole-file exemption, same as the Next.js App
            // Router files above (dogfood fe-svelte: these were 20/26 dead-export + ~13 dead-candidate FPs).
            // `.js` and `.ts` both, since SvelteKit projects use either.
            r"(^|/)\+(page|layout)(\.server)?\.(js|ts)$",
            r"(^|/)\+server\.(js|ts)$",
            // `.server`/`.client` REQUIRED — a bare `hooks.ts`/`hooks.js` is an extremely common React
            // hooks-barrel filename that is NOT a framework entry, so exempting it would hide real dead
            // exports. SvelteKit's universal `src/hooks.ts` (rare vs `hooks.server`/`hooks.client`) is the
            // accepted miss.
            r"(^|/)hooks\.(server|client)\.(js|ts)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

fn tool_entry_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        let mut v: Vec<Regex> = [
            // Dotfile configs consumed directly by their tool's own resolver, never imported.
            r"(^|/)\.(eslintrc|prettierrc|babelrc|stylelintrc)(\.[^/]+)?$",
            // Ambient TypeScript declarations — type-only, consumed by tsc without an import edge.
            r"\.d\.ts$",
            // Test-runner setup entries loaded via a config field (`setupFiles`/`globalSetup` in
            // vitest/jest/playwright config), not imported by app code — so `fan_in == 0` is expected.
            // Matched by conventional filename since they can live anywhere (`src/setup-tests.ts`,
            // `src/test-setup.ts`, `vitest.setup.ts`, `jest.setup.ts`, a Playwright `global.setup.ts`).
            r"(^|/)(vitest|jest)\.setup\.(js|ts|mjs|cjs|mts|cts)$",
            r"(^|/)setup-tests?\.(js|ts|mjs|cjs|mts|cts)$",
            r"(^|/)setupTests\.(js|ts|mjs|cjs|mts|cts)$",
            r"(^|/)test-setup\.(js|ts|mjs|cjs|mts|cts)$",
            r"(^|/)global\.(setup|teardown)\.(js|ts|mjs|cjs|mts|cts)$",
            // Jest preset config (`jest.preset.js`, an Nx/monorepo convention) — consumed by jest's own
            // config resolver via the `preset` field, never imported.
            r"(^|/)jest\.preset\.(js|ts|mjs|cjs)$",
            // Prisma seed script at its conventional location — run by the Prisma CLI (`prisma db seed`,
            // wired via the package.json `prisma.seed` field), never imported by app code.
            r"(^|/)prisma/seed\.(js|ts|mjs|cjs|mts|cts)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect();
        // The tool-CONFIG shapes, shared with `is_tool_config_file` rather than restated: a declaration
        // source is always also a tool entry, so the narrower set is included wholesale.
        v.extend(tool_config_patterns().iter().cloned());
        // ...and then EXEMPTION-ONLY: any file at depth 1 inside a dot-directory. This is the half that
        // must NOT reach the harvest, and the reason the two predicates stopped being one.
        //
        // A leading dot marks a directory its tool owns, so a file one level inside is loaded by that
        // tool at a convention path rather than imported — Storybook writes `main.mjs`, `preview.jsx`,
        // `manager.ts` and never `config`. Measured before widening over the dogfood corpus plus
        // apache/superset, 13,509 js/ts-family files: 10 files newly exempt, every one inside a
        // `.storybook/` (grafana x8, superset x2).
        //
        // But exempting a file is not the same as believing what it says. The first version of this
        // widening lived in `tool_config_patterns`, which also feeds `config_entries`' path harvest, and
        // that turned `.storybook/copyAssets.ts`'s ordinary string `'../src/components/Icon/utils.ts'`
        // into an entry declaration — silently exempting a file that had no importer, on the strength
        // of a guess about who owns a directory. Depth ONE on purpose: `docs/.vitepress/theme/index.ts`
        // is ordinary source that VitePress happens to load, and it stays a candidate.
        v.push(
            Regex::new(r"(?i)(^|/)\.[^/]+/[^/]+\.(js|jsx|ts|tsx|mjs|cjs|mts|cts)$")
                .expect("dot-directory depth-1 pattern is valid"),
        );
        v
    })
}

pub(super) fn entry_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"(^|/)index\.(t|j)sx?$",
            r"(^|/)main\.(t|j)sx?$",
            r"(^|/)main\.go$",
            r"(^|/)mod\.ts$",
            r"(^|/)App\.(t|j)sx?$",
            r"Page\.(t|j)sx?$",
            r"Route\.(t|j)sx?$",
            r"(^|/)routes?\.(t|j)sx?$",
            r"apiRoutes\.(t|j)sx?$",
            r"\.config\.(t|j)sx?$",
            r"(^|/)(server|app|bootstrap|worker|cli)\.(t|j)sx?$",
            r"(^|/)(cmd)/",
            r"Application\.java$",
            r"(^|/)Main\.java$",
            r"(^|/)(__main__|manage|wsgi|asgi|main|settings|conftest)\.py$",
            // Rust entry conventions: crate/binary roots (`main.rs`/`lib.rs`/`build.rs`) plus any file
            // under a `tests/`/`examples/`/`benches/`/`src/bin/` path component — cargo's own conventional
            // test-harness/example-binary/benchmark-binary/multi-binary directories, each compiled and run
            // as its own separate target rather than `use`d from elsewhere in the crate, so zero in-repo
            // importers is expected for files under them, not a dead/unreachable signal.
            r"(^|/)(main|lib|build)\.rs$",
            r"(^|/)(tests|examples|benches)/",
            r"(^|/)src/bin/",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}
