//! What the convention exempts and — more of the point — what it refuses to.

use std::collections::HashSet;
use std::path::Path;

use super::scan;

fn paths(xs: &[&str]) -> HashSet<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

fn run(xs: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = scan(Path::new("/tree"), &paths(xs)).into_iter().collect();
    v.sort();
    v
}

/// The measured nocodb shape: a Nuxt app nested in a monorepo. Both convention directories are entries,
/// nested paths included, and the app's other source is untouched.
#[test]
fn nuxt_plugins_and_middleware_are_entries_and_nothing_else_is() {
    assert_eq!(
        run(&[
            "packages/nc-gui/nuxt.config.ts",
            "packages/nc-gui/plugins/api.ts",
            "packages/nc-gui/plugins/nested/deep.ts",
            "packages/nc-gui/middleware/03.auth.global.ts",
            "packages/nc-gui/composables/useGlobal.ts",
            "packages/nc-gui/components/Thing.vue",
        ]),
        vec![
            "packages/nc-gui/middleware/03.auth.global.ts".to_string(),
            "packages/nc-gui/plugins/api.ts".to_string(),
            "packages/nc-gui/plugins/nested/deep.ts".to_string(),
        ]
    );
}

/// The convention is ANCHORED to an app, exactly as the import alias is. A `plugins/` directory in a
/// tree with no Nuxt config is an ordinary directory, and exempting it would hide real dead code.
#[test]
fn a_plugins_directory_without_a_nuxt_config_is_not_exempt() {
    assert!(run(&["src/plugins/thing.ts", "src/index.ts"]).is_empty());
}

/// A sibling app's convention directory must not be reached from another app's config — the same
/// monorepo bleed the alias resolver refuses.
#[test]
fn one_apps_config_does_not_exempt_another_apps_plugins() {
    assert_eq!(
        run(&[
            "apps/site/nuxt.config.ts",
            "apps/site/plugins/a.ts",
            "apps/admin/plugins/b.ts",
        ]),
        vec!["apps/site/plugins/a.ts".to_string()]
    );
}

/// The prefix must end at a separator: a `plugins-legacy/` sibling is a different directory and stays
/// eligible for `dead-candidates`.
#[test]
fn a_similarly_named_sibling_directory_is_not_swept_in() {
    assert_eq!(
        run(&[
            "nuxt.config.ts",
            "plugins/real.ts",
            "plugins-legacy/old.ts",
            "middleware.ts",
        ]),
        vec!["plugins/real.ts".to_string()]
    );
}

/// A config at the analysis root is the single-app layout and must anchor — the empty-string directory
/// case an ancestor-style join usually gets wrong.
#[test]
fn a_config_at_the_root_anchors_its_own_conventions() {
    assert_eq!(
        run(&["nuxt.config.mjs", "middleware/auth.global.ts"]),
        vec!["middleware/auth.global.ts".to_string()]
    );
}

/// Auto-import directories are deliberately NOT exempt: those files are referenced by bare symbol name
/// from call sites this graph reads, so the honest fix is to resolve the reference. Exempting them would
/// also bury the genuinely dead composables the same audit found.
#[test]
fn auto_import_directories_are_not_exempted_here() {
    assert!(run(&[
        "nuxt.config.ts",
        "composables/useX.ts",
        "utils/fmt.ts",
        "store/base.ts",
        "helpers/h.ts",
    ])
    .is_empty());
}
