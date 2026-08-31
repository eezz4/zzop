//! What the generated-config aliases resolve to, and — as much of the point — what they refuse to
//! resolve. Every case here fails without this module; the nocodb shape in
//! `a_monorepo_nuxt_app_resolves_against_its_own_src_dir` is the one that motivated it.

use std::collections::HashSet;

use super::resolve_framework_alias;

fn paths(list: &[&str]) -> HashSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// The measured nocodb shape: a Nuxt app nested in a monorepo, importing through `~/`. Resolving
/// against the ANALYSIS ROOT would find nothing here (there is no top-level `components/`), which is
/// exactly why the tree read as orphaned before this existed.
#[test]
fn a_monorepo_nuxt_app_resolves_against_its_own_src_dir() {
    let all = paths(&[
        "packages/nc-gui/nuxt.config.ts",
        "packages/nc-gui/components/monaco/json.ts",
        "packages/nc-gui/components/webhook/index.vue",
    ]);
    assert_eq!(
        resolve_framework_alias(
            "~/components/monaco/json",
            "packages/nc-gui/components/webhook/index.vue",
            &all
        ),
        Some("packages/nc-gui/components/monaco/json.ts".to_string())
    );
}

/// All four spellings mean the same directory under the default layout, and a reader should not have to
/// discover that by experiment.
#[test]
fn all_four_nuxt_spellings_reach_the_same_file() {
    let all = paths(&["app/nuxt.config.ts", "app/utils/fmt.ts"]);
    for spec in ["~/utils/fmt", "@/utils/fmt", "~~/utils/fmt", "@@/utils/fmt"] {
        assert_eq!(
            resolve_framework_alias(spec, "app/pages/index.vue", &all),
            Some("app/utils/fmt.ts".to_string()),
            "{spec}"
        );
    }
}

/// The anchoring IS the safety property: two apps in one repo declaring the same relative path must not
/// bleed into each other. Resolving `~` at the analysis root would return the wrong app's file, and a
/// wrong edge is worse than no edge — it makes a genuinely dead file look alive.
#[test]
fn a_sibling_apps_same_named_file_is_never_reached() {
    let all = paths(&[
        "apps/admin/nuxt.config.ts",
        "apps/admin/utils/fmt.ts",
        "apps/site/nuxt.config.ts",
        "apps/site/utils/fmt.ts",
    ]);
    assert_eq!(
        resolve_framework_alias("~/utils/fmt", "apps/site/pages/index.vue", &all),
        Some("apps/site/utils/fmt.ts".to_string())
    );
    assert_eq!(
        resolve_framework_alias("~/utils/fmt", "apps/admin/pages/index.vue", &all),
        Some("apps/admin/utils/fmt.ts".to_string())
    );
}

/// A single-app repo puts the config at the top, so `""` is a real answer and must not read as "not
/// found" — the loop's empty-string case, which is the one an ancestor walk usually gets wrong.
#[test]
fn a_config_at_the_analysis_root_still_anchors() {
    let all = paths(&["nuxt.config.ts", "composables/useX.ts"]);
    assert_eq!(
        resolve_framework_alias("~/composables/useX", "pages/index.vue", &all),
        Some("composables/useX.ts".to_string())
    );
}

/// No Nuxt config above the importer means this is not a Nuxt alias, whatever it looks like. Falling
/// back to the root here is the tempting move and it is the one that mints wrong edges.
#[test]
fn a_tilde_specifier_with_no_nuxt_config_resolves_to_nothing() {
    let all = paths(&["src/utils/fmt.ts", "utils/fmt.ts"]);
    assert!(resolve_framework_alias("~/utils/fmt", "src/main.ts", &all).is_none());
}

/// SvelteKit's `$lib` in the single-app shape it had before anchoring: the config sits at the analysis
/// root, so the anchor is `""` and the answer must be byte-identical to the old rooted `src/lib`. This
/// is the regression pin — anchoring is only free if this case does not move.
#[test]
fn sveltekit_lib_resolves_bare_and_with_a_subpath() {
    let all = paths(&[
        "svelte.config.js",
        "src/lib/index.ts",
        "src/lib/db/client.ts",
    ]);
    assert_eq!(
        resolve_framework_alias("$lib", "src/routes/+page.svelte", &all),
        Some("src/lib/index.ts".to_string())
    );
    assert_eq!(
        resolve_framework_alias("$lib/db/client", "src/routes/+page.svelte", &all),
        Some("src/lib/db/client.ts".to_string())
    );
}

/// The measured immich shape, and the whole reason `$lib` was anchored: the Kit app lives in `web/`, so
/// `$lib` means `web/src/lib`. Rooted resolution looked for a top-level `src/lib`, found nothing, and
/// left the app reading as orphaned — 235 of 236 `dead-candidates` there were false.
#[test]
fn a_monorepo_sveltekit_app_resolves_against_its_own_lib() {
    let all = paths(&[
        "web/svelte.config.js",
        "web/src/lib/actions/shortcut.ts",
        "web/src/routes/+page.svelte",
        "server/src/main.ts",
    ]);
    assert_eq!(
        resolve_framework_alias("$lib/actions/shortcut", "web/src/routes/+page.svelte", &all),
        Some("web/src/lib/actions/shortcut.ts".to_string())
    );
}

/// Same safety property the Nuxt side already had: two Kit apps in one repo naming the same `$lib` path
/// must not bleed into each other. A wrong edge makes a genuinely dead file look alive.
#[test]
fn a_sibling_sveltekit_apps_same_named_lib_file_is_never_reached() {
    let all = paths(&[
        "apps/admin/svelte.config.js",
        "apps/admin/src/lib/fmt.ts",
        "apps/site/svelte.config.js",
        "apps/site/src/lib/fmt.ts",
    ]);
    assert_eq!(
        resolve_framework_alias("$lib/fmt", "apps/site/src/routes/+page.svelte", &all),
        Some("apps/site/src/lib/fmt.ts".to_string())
    );
    assert_eq!(
        resolve_framework_alias("$lib/fmt", "apps/admin/src/routes/+page.svelte", &all),
        Some("apps/admin/src/lib/fmt.ts".to_string())
    );
}

/// No SvelteKit config above the importer means this is not SvelteKit's `$lib`, whatever it looks like.
/// A project that genuinely maps `$lib` itself is answered earlier, by its tsconfig `paths` — see
/// `resolve_file_with_workspace`, where the governing tsconfig gets first say — so falling back to the
/// root here would only ever guess.
#[test]
fn a_lib_specifier_with_no_svelte_config_resolves_to_nothing() {
    let all = paths(&["src/lib/fmt.ts", "web/src/lib/fmt.ts"]);
    assert!(resolve_framework_alias("$lib/fmt", "web/src/routes/+page.svelte", &all).is_none());
}

/// Every spelling of the Kit config anchors, not just the one the templates emit.
#[test]
fn all_svelte_config_spellings_anchor() {
    for cfg in [
        "web/svelte.config.js",
        "web/svelte.config.ts",
        "web/svelte.config.mjs",
        "web/svelte.config.cjs",
    ] {
        let all = paths(&[cfg, "web/src/lib/fmt.ts"]);
        assert_eq!(
            resolve_framework_alias("$lib/fmt", "web/src/routes/+page.svelte", &all),
            Some("web/src/lib/fmt.ts".to_string()),
            "{cfg}"
        );
    }
}

/// An ordinary package specifier is not an alias and must stay external — `@scope/pkg` in particular
/// begins with `@` and would be caught by a careless prefix test.
#[test]
fn ordinary_specifiers_are_left_alone() {
    let all = paths(&["nuxt.config.ts", "scope/pkg.ts", "lodash.ts"]);
    for spec in ["lodash", "@scope/pkg", "./relative", "../up"] {
        assert!(
            resolve_framework_alias(spec, "pages/index.vue", &all).is_none(),
            "{spec}"
        );
    }
}

/// A Nuxt tree that does not contain the named file resolves to `None` rather than to some near miss.
#[test]
fn an_alias_naming_no_known_file_is_none() {
    let all = paths(&["app/nuxt.config.ts", "app/utils/fmt.ts"]);
    assert!(resolve_framework_alias("~/utils/missing", "app/pages/index.vue", &all).is_none());
}
