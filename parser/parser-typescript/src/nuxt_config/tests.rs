use super::parse_nuxt_imports_dirs;

fn dirs(source: &str) -> Vec<String> {
    parse_nuxt_imports_dirs("nuxt.config.ts", source)
}

/// The measured nocodb line (`packages/nc-gui/nuxt.config.ts:391`), verbatim shape.
#[test]
fn define_nuxt_config_wrapper() {
    assert_eq!(
        dirs(
            "export default defineNuxtConfig({\n  ssr: true,\n  imports: {\n    \
             dirs: ['./context', './utils/**', './lib', './composables/**', './store/**', './helpers'],\n    \
             imports: [{ name: 'useI18n', from: 'vue-i18n' }],\n  },\n})\n"
        ),
        vec![
            "./context",
            "./utils/**",
            "./lib",
            "./composables/**",
            "./store/**",
            "./helpers"
        ]
    );
}

#[test]
fn bare_object_default_export_and_quoted_keys() {
    assert_eq!(
        dirs("export default { imports: { dirs: ['./a'] } }\n"),
        vec!["./a"]
    );
    assert_eq!(
        dirs("export default { \"imports\": { \"dirs\": [\"./b\"] } }\n"),
        vec!["./b"]
    );
}

/// Every shape that must yield NOTHING rather than a guess. A missing declaration leaves a false
/// `dead-candidates` finding, which is recoverable; a guessed one silences a live-looking file no build
/// ever auto-imported, which is not.
#[test]
fn shapes_that_declare_nothing_read_as_nothing() {
    assert!(dirs("export default defineNuxtConfig({ ssr: true })\n").is_empty());
    assert!(dirs("export default { imports: { autoImport: false } }\n").is_empty());
    // A `dirs` key that is not under `imports` is a different setting entirely.
    assert!(dirs("export default { components: { dirs: ['./ui'] } }\n").is_empty());
    // Assembled in a variable — no object literal at the export site to read.
    assert!(dirs("const c = { imports: { dirs: ['./a'] } }\nexport default c\n").is_empty());
    // No default export at all, and an unparseable file.
    assert!(dirs("export const config = { imports: { dirs: ['./a'] } }\n").is_empty());
    assert!(dirs("export default defineNuxtConfig({ imports: { dirs: [ }\n").is_empty());
}

/// Non-literal entries are dropped, and the literal ones beside them still read.
#[test]
fn computed_entries_are_dropped_and_their_neighbours_survive() {
    assert_eq!(
        dirs("const x = './z'\nexport default { imports: { dirs: ['./a', x, ...more, './b'] } }\n"),
        vec!["./a", "./b"]
    );
}
