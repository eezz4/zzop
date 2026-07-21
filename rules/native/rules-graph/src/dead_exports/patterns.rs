//! File-path and export-name exemption patterns for `find_dead_exports` — see the parent module
//! doc's "Exemptions" section.

use std::sync::OnceLock;

use regex::Regex;

use crate::unreachable::{framework_route_patterns, is_tool_entry_file};

pub(super) fn is_entry_file(path: &str) -> bool {
    entry_patterns().iter().any(|re| re.is_match(path))
}

pub(super) fn is_excluded_file(path: &str) -> bool {
    exclude_patterns().iter().any(|re| re.is_match(path))
}

pub(super) fn is_entry_or_test(path: &str) -> bool {
    // `is_tool_entry_file` covers tool-config files loaded by their own tool rather than imported.
    // `zzop_core::is_test_file` is the SSOT for "test surface" — it recognizes the test-runner
    // DIRECTORY conventions (`e2e/`, `cypress/`, `playwright/`, `testing/`, `__tests__/`, `tests/`,
    // `spec/`) that the local `exclude_patterns()` below deliberately does NOT duplicate; a file under
    // one of those dirs (e.g. `playwright/global.setup.ts`) is loaded by the runner, never imported, so
    // its exports having zero in-repo importers is expected, not dead. Delegating here keeps the two
    // dead-code analyses from drifting out of sync with the shared predicate (they both had only the
    // `.test.`/`.spec.` FILE forms before, missing the directory forms).
    is_entry_file(path)
        || is_excluded_file(path)
        || is_tool_entry_file(path)
        || zzop_core::is_test_file(path)
}

fn entry_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        let mut v: Vec<Regex> = [
            r"(^|/)index\.(ts|tsx|js|jsx)$",
            r"(^|/)main\.(ts|tsx|js|jsx)$",
            r"(^|/)App\.(ts|tsx|js|jsx)$",
            r"Page\.(ts|tsx)$",
            r"(^|/)apiRoutes\.(ts|tsx)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect();
        // Next.js App Router convention files — shared with `dead_candidates` so the two can't drift.
        v.extend(framework_route_patterns().iter().cloned());
        v
    })
}

fn exclude_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"\.(test|spec)\.(ts|tsx|js|jsx)$",
            r"\.stories\.(ts|tsx|js|jsx)$",
            r"/__test__/",
            r"/__mocks__/",
            r"\.d\.ts$",
            // Storybook config directory — `.storybook/preview.tsx`, `.storybook/main.ts`, etc. Storybook
            // loads these itself by fixed filename/directory convention, never via an in-repo import.
            r"(^|/)\.storybook/",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

/// Framework-contract export names consumed by their framework via named-export convention rather than
/// an `import` — flagging them dead and deleting them breaks the app at runtime even though the in-repo
/// import graph shows zero importers. Kept deliberately small and unambiguous: every name here is a
/// framework-reserved *camelCase* identifier that a project would essentially never coincidentally reuse
/// for an unrelated symbol. Generic words are excluded ON PURPOSE — a bare `config`/`meta`/`parameters`/
/// `decorators`/`middleware`/`default` is plausibly a real (possibly dead) domain symbol, so a global
/// name exemption for those would cause false NEGATIVES (real dead code silently missed). They are left
/// to the normal dead-export path.
///
/// This is load-bearing for Next.js *Pages Router* files (`pages/**`, e.g. `pages/blog/[slug].tsx`),
/// whose file paths are arbitrary and unmatched by any entry/exclude pattern, so
/// `getServerSideProps`/`getStaticProps`/… in such a file would otherwise read as dead. Next.js App
/// Router convention files (`page.tsx`, `layout.tsx`, `route.ts`, …) are already wholesale-excluded via
/// `framework_route_patterns()` in `entry_patterns()`, so the list is belt-and-suspenders for those.
///
/// Storybook `decorators`/`parameters`/`globalTypes` are deliberately NOT here — they live in
/// `.storybook/`- or `.stories.`-path files, both already file-level excluded (see `exclude_patterns()`),
/// so a name exemption would only add false-negative risk for the rare re-export-from-elsewhere case.
/// Next.js root `middleware.ts` is likewise left out: `middleware` is too generic a name to exempt
/// globally — its `middleware`/`config` exports are instead exempted only when the file itself is a
/// `middleware.{ts,js}` convention file (see `is_middleware_convention_file` in `find_dead_exports`).
pub(super) fn is_framework_contract_export(name: &str) -> bool {
    matches!(
        name,
        // Next.js: reserved data-fetching/route-contract export names, read by the framework at build
        // or request time by exact identifier — never through a normal import statement.
        "getServerSideProps"
            | "getStaticProps"
            | "getStaticPaths"
            | "getInitialProps"
            | "generateMetadata"
            | "generateStaticParams"
    )
}

/// `middleware.ts`/`middleware.js` — the Next.js root-middleware convention filename, whose
/// `middleware`/`config` exports the framework reads by exact name. Deliberately NOT root-anchored: a
/// Next app inside a monorepo tree lives below the analyzed root (`apps/web/middleware.ts`). The
/// accepted false-negative is a dead symbol literally named `middleware`/`config` in a non-Next file
/// that happens to be named `middleware.ts` — far rarer than the Next convention FP this removes.
pub(super) fn is_middleware_convention_file(path: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(^|/)middleware\.(ts|js)$").unwrap())
        .is_match(path)
}
