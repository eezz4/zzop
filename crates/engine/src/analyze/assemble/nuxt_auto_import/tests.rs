//! Unit pins for the pieces the end-to-end suite (`crates/engine/tests/integration/
//! analyze_nuxt_auto_import.rs`) exercises through the whole pipeline: the app-dir anchor, the
//! candidate shape, and the token scan's own boundaries.

use super::dirs::{default_dirs, is_candidate, normalize_declared_dir, AutoImportDir};
use super::referrers::in_app_dir;
use super::*;

fn set(xs: &[&str]) -> HashSet<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn app_dirs_finds_every_nuxt_config_and_nothing_else() {
    assert_eq!(
        app_dirs(&set(&[
            "packages/nc-gui/nuxt.config.ts",
            "apps/site/nuxt.config.mjs",
            "packages/nc-gui/composables/useX.ts",
            "vite.config.ts",
        ])),
        vec!["apps/site".to_string(), "packages/nc-gui".to_string()]
    );
    assert!(app_dirs(&set(&["src/index.ts", "vite.config.ts"])).is_empty());
}

/// A config at the analysis root anchors the whole tree — the empty-string directory case.
#[test]
fn a_root_config_anchors_everything() {
    assert_eq!(app_dirs(&set(&["nuxt.config.ts"])), vec![String::new()]);
    assert!(in_app_dir("composables/useX.ts", ""));
}

#[test]
fn app_dir_membership_stops_at_a_separator() {
    assert!(in_app_dir("packages/nc-gui/utils/a.ts", "packages/nc-gui"));
    assert!(!in_app_dir(
        "packages/nc-gui-legacy/utils/a.ts",
        "packages/nc-gui"
    ));
    assert!(!in_app_dir("packages/nocodb/src/a.ts", "packages/nc-gui"));
    // The app dir itself is not "inside" itself, and neither is a path that merely shares its prefix.
    assert!(!in_app_dir("packages/nc-gui", "packages/nc-gui"));
}

/// The referrer side pairs a file with ONE app — the deepest `nuxt.config.*` directory containing it,
/// because that is the app whose auto-import table compiles it. A union over the app dirs is what let
/// one app's call site vouch for another app's composable.
#[test]
fn a_referrer_is_paired_with_the_deepest_app_that_contains_it() {
    let apps = vec![
        String::new(),
        "apps/admin".to_string(),
        "apps/web".to_string(),
    ];
    assert_eq!(
        referrer_app_dir("apps/web/pages/index.vue", &apps),
        Some("apps/web")
    );
    assert_eq!(
        referrer_app_dir("apps/admin/composables/useUser.ts", &apps),
        Some("apps/admin")
    );
    // Not inside either nested app: it belongs to the root app, and to nothing else.
    assert_eq!(referrer_app_dir("shared/util.ts", &apps), Some(""));
    // With no root config, a file outside every app dir pairs with no app at all.
    let nested = vec!["apps/admin".to_string(), "apps/web".to_string()];
    assert_eq!(referrer_app_dir("shared/util.ts", &nested), None);
}

/// `<app>/public/` is served VERBATIM — Nuxt's auto-import transform never runs over it, so no token
/// there can be a call site. The app-dir wall alone does not exclude it: a vendored bundle the app
/// ships itself is INSIDE the app dir (measured: `nc-gui/public/js/swagger-ui-bundle.min.js`).
#[test]
fn the_apps_own_public_dir_never_vouches() {
    let apps = vec!["packages/nc-gui".to_string()];
    assert_eq!(
        referrer_app_dir("packages/nc-gui/public/js/swagger-ui-bundle.min.js", &apps),
        None
    );
    assert_eq!(
        referrer_app_dir("packages/nc-gui/components/Cell.vue", &apps),
        Some("packages/nc-gui")
    );
    // A root-anchored app has a `public/` too, and a directory that merely starts with the name is a
    // different directory.
    assert_eq!(
        referrer_app_dir("public/js/vendor.js", &[String::new()]),
        None
    );
    assert_eq!(
        referrer_app_dir("publicApi/client.ts", &[String::new()]),
        Some("")
    );
}

/// Only Nuxt's own component dialect vouches through the whole-text token roster. The pre-scan hands
/// this pass four other filetypes ([`zzop_parser_typescript::PRESCAN_IMPORT_HOSTS`]) and none of them
/// is a place a Nuxt auto-import resolves.
#[test]
fn only_a_vue_file_is_a_token_roster_host() {
    assert!(is_token_roster_host("app/pages/index.vue"));
    assert!(is_token_roster_host("app/pages/Index.VUE"));
    for other in [
        "app/content/doc.md",
        "app/content/doc.mdx",
        "app/w.svelte",
        "app/p.astro",
        "app/noext",
    ] {
        assert!(!is_token_roster_host(other), "{other} must not vouch");
    }
}

#[test]
fn default_candidate_shape_is_one_level_or_an_index() {
    let app = "packages/nc-gui";
    let d = default_dirs();
    assert!(is_candidate("packages/nc-gui/composables/useX.ts", app, &d));
    assert!(is_candidate("packages/nc-gui/utils/fmt.mjs", app, &d));
    assert!(is_candidate(
        "packages/nc-gui/composables/group/index.ts",
        app,
        &d
    ));
    // Nested non-index: only a declared recursive glob makes this an auto-import.
    assert!(!is_candidate(
        "packages/nc-gui/composables/group/inner.ts",
        app,
        &d
    ));
    // Directories Nuxt does not auto-import by convention — `store`/`helpers`/`lib` are auto-imported
    // ONLY when `imports.dirs` names them, so admitting them by default would be a guess.
    assert!(!is_candidate("packages/nc-gui/store/base.ts", app, &d));
    assert!(!is_candidate("packages/nc-gui/lib/x.ts", app, &d));
    // Wrong extension, and a `.vue` component that happens to sit in the directory.
    assert!(!is_candidate(
        "packages/nc-gui/composables/useX.vue",
        app,
        &d
    ));
    // Another app's directory is never this app's candidate.
    assert!(!is_candidate("apps/other/composables/useX.ts", app, &d));
}

/// The DECLARED half (§24): `store`/`helpers`/`lib` become candidates only because the config says so,
/// and a `**` entry is what admits a nested non-index file.
#[test]
fn declared_dirs_widen_the_candidate_set_exactly_as_declared() {
    let app = "packages/nc-gui";
    let mut d = default_dirs();
    d.push(AutoImportDir {
        name: "store".to_string(),
        recursive: true,
    });
    d.push(AutoImportDir {
        name: "lib".to_string(),
        recursive: false,
    });
    assert!(is_candidate("packages/nc-gui/store/base.ts", app, &d));
    assert!(is_candidate(
        "packages/nc-gui/store/deep/nested.ts",
        app,
        &d
    ));
    assert!(is_candidate("packages/nc-gui/lib/x.ts", app, &d));
    // Non-recursive stays non-recursive.
    assert!(!is_candidate("packages/nc-gui/lib/deep/x.ts", app, &d));
    assert!(is_candidate("packages/nc-gui/lib/deep/index.ts", app, &d));
    // A directory nobody declared is still not a candidate.
    assert!(!is_candidate("packages/nc-gui/helpers/h.ts", app, &d));
}

#[test]
fn declared_entry_normalization() {
    let cases = [
        ("./utils/**", Some(("utils", true))),
        ("./lib", Some(("lib", false))),
        ("helpers/", Some(("helpers", false))),
        ("./composables/*", Some(("composables", false))),
        ("./nested/deep/**", Some(("nested/deep", true))),
    ];
    for (entry, want) in cases {
        let got = normalize_declared_dir(entry);
        assert_eq!(
            got,
            want.map(|(n, r)| AutoImportDir {
                name: n.to_string(),
                recursive: r
            }),
            "normalizing {entry:?}"
        );
    }
    // Unanchorable or wildcard-in-the-middle entries are dropped, never guessed at.
    for entry in ["", "/abs/path", "../outside", "./a/../b", "./a/*/b"] {
        assert_eq!(normalize_declared_dir(entry), None, "must drop {entry:?}");
    }
}

#[test]
fn identifier_tokens_splits_on_word_boundaries_and_drops_numbers() {
    let tokens = identifier_tokens("<div>{{ useBase().foo_bar }}</div> const $el = 12px + x1");
    for want in ["div", "useBase", "foo_bar", "const", "$el", "x1"] {
        assert!(tokens.contains(want), "missing {want:?} in {tokens:?}");
    }
    assert!(!tokens.contains("12px"), "a number-led token is not a name");
    assert!(
        !tokens.contains("useBase().foo_bar"),
        "member access must split: a property named like a composable is not a reference to it"
    );
}

/// The permissiveness this scan deliberately accepts, stated so it is a decision rather than a
/// surprise: a name inside a comment or a string literal counts. Measured on nocodb, that wrongly
/// silenced 0 of the 18 genuinely dead files, which is why the cheaper scan wins over a `<script>`
/// parse. It applies to PRE-SCAN hosts only — the `.ts` side reads a parsed roster, where a comment
/// does not count, which is the asymmetry the parent module's doc records against `utils/Queue.ts`.
#[test]
fn identifier_tokens_reads_comments_and_strings_too() {
    let tokens = identifier_tokens("// see useThing\nconst s = 'alsoThis'\n");
    assert!(tokens.contains("useThing"));
    assert!(tokens.contains("alsoThis"));
}
