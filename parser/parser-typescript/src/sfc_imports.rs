//! `.vue`/`.svelte` Single-File-Component `<script>`-block import pre-scan. A lexical block extract (NOT
//! a full SFC/template parse) feeding the EXISTING `parse_imports` — the import syntax is identical
//! across `<script>`/`<script setup>`/`<script lang="ts">` (all ES `import`), and swc happily parses the
//! TS-shaped concatenated text, so no SFC-flavor-specific handling is needed here.
//!
//! ## Why this exists
//! `.vue`/`.svelte` files reach the engine as lexical-only `SourceFile`s (dispatch routes them to `None` —
//! `zzop_engine::dispatch`), so a `.ts` symbol imported and used ONLY inside a component's `<script>`
//! block has zero visible fan-in through the normal fused pipeline, false-firing `dead-exports`/
//! `dead-candidates`. This helper is called by the engine at ASSEMBLE time (uncached, off-disk, mirroring
//! `dead_exports.rs`'s own re-read-off-disk pattern) so the win costs no cache-schema/fingerprint bump: it
//! never touches the cached fused-pipeline projection for the `.vue`/`.svelte` file itself.
//!
//! A `.vue` file may carry TWO script blocks (`<script>` for options-API exports plus `<script setup>`
//! for the component body) — both are extracted and concatenated before the single `parse_imports` call,
//! so imports from either block are captured.
//!
//! ## Known lexical limits (bounded on purpose)
//! Because this is a raw block extract and not a real SFC parser, a `<script>…import…</script>` that is
//! commented out (`<!-- … -->`) or embedded inside a template string is still captured, so its imports
//! count as live. The failure direction is conservative: it can only SUPPRESS a `dead-exports`/
//! `dead-candidates` finding (mark a possibly-dead export alive), never mint a new false positive — an
//! acceptable trade for a pre-scan whose whole job is removing SFC-blindness false positives.

use zzop_core::ImportMap;

use crate::imports::parse_imports;

/// Extracts every `<script ...>...</script>` block's text from an SFC (`.vue`/`.svelte`) source, in
/// document order, concatenated with a newline separator (so a name in one block never collides with a
/// name from another mid-line), then runs the EXISTING `parse_imports(rel, ...)` on the concatenated text.
/// Returns an empty `ImportMap` when the file has no `<script>` block at all.
///
/// Deliberately lexical (a simple DOTALL block extract), not a real SFC/template parser: the only thing
/// this needs from the file is the raw text between `<script ...>` and `</script>`, and swc parses that
/// text exactly as it would a standalone `.ts` module regardless of `lang="ts"`/`setup` attributes on the
/// opening tag (those attributes are never inspected).
pub fn extract_sfc_script_imports(rel: &str, text: &str) -> ImportMap {
    let mut combined = String::new();
    for block in script_blocks(text) {
        combined.push_str(block);
        combined.push('\n');
    }
    if combined.trim().is_empty() {
        return ImportMap::new();
    }
    parse_imports(rel, &combined)
}

/// Lexical `<script ...>(.*?)</script>` block extract (DOTALL — a script body routinely spans many
/// lines), case-insensitive on the tag name for robustness (real-world SFC tooling is lenient about
/// `<SCRIPT>`/`<Script>` even though the vast majority of source in the wild is lowercase). Attributes on
/// the opening tag (`setup`, `lang="ts"`, ...) are matched and discarded, never inspected — see this
/// module's doc for why no flavor-specific handling is needed.
///
/// The opening-tag attribute scan is quote-aware (`"…"`/`'…'` runs are consumed whole) so a `>` inside an
/// attribute value — e.g. a Vue 3.3+/Svelte generic `generic="T extends Record<string, unknown>"` — does
/// NOT terminate the tag early and truncate the captured body.
fn script_blocks(text: &str) -> Vec<&str> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r#"(?is)<script(?:\s(?:"[^"]*"|'[^']*'|[^>"'])*)?>(.*?)</script>"#)
            .expect("valid regex")
    });
    re.captures_iter(text)
        .filter_map(|c| c.get(1))
        .map(|m| m.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::extract_sfc_script_imports;

    #[test]
    fn script_setup_captures_named_import() {
        let src = "<script setup>\nimport { useX } from 'src/composable/use-x'\n</script>\n<template><div/></template>\n";
        let m = extract_sfc_script_imports("src/App.vue", src);
        assert_eq!(m["useX"].specifier, "src/composable/use-x");
        assert_eq!(m["useX"].original, "useX");
    }

    #[test]
    fn two_script_blocks_both_captured() {
        let src = "<script>\nimport { defineComponent } from 'vue'\n</script>\n<script setup>\nimport { useY } from './use-y'\n</script>\n<template></template>\n";
        let m = extract_sfc_script_imports("src/App.vue", src);
        assert_eq!(m["defineComponent"].specifier, "vue");
        assert_eq!(m["useY"].specifier, "./use-y");
    }

    #[test]
    fn no_script_block_is_empty() {
        let src = "<template><div>hello</div></template>\n";
        let m = extract_sfc_script_imports("src/App.vue", src);
        assert!(m.is_empty());
    }

    #[test]
    fn svelte_script_lang_ts_attribute() {
        let src =
            "<script lang=\"ts\">\nimport { onMount } from 'svelte'\n</script>\n<div>hi</div>\n";
        let m = extract_sfc_script_imports("src/App.svelte", src);
        assert_eq!(m["onMount"].specifier, "svelte");
    }

    #[test]
    fn generic_attribute_with_angle_brackets_does_not_truncate_the_body() {
        // Vue 3.3+/Svelte generic component: the `>` inside `generic="...>"` must not close the opening
        // tag early — the quote-aware attribute scan keeps the whole body, so the import is still seen.
        let src = "<script setup lang=\"ts\" generic=\"T extends Record<string, unknown>\">\nimport { useX } from './use-x'\n</script>\n<template/>\n";
        let m = extract_sfc_script_imports("src/App.vue", src);
        assert_eq!(m["useX"].specifier, "./use-x");
    }

    #[test]
    fn empty_script_block_stays_empty() {
        let src = "<script setup>\n</script>\n<template></template>\n";
        let m = extract_sfc_script_imports("src/App.vue", src);
        assert!(m.is_empty());
    }
}
