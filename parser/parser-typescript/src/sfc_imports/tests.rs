//! Unit tests for the SFC `<script>`-block import pre-scan. Split out of `sfc_imports.rs` for the
//! 300-line cap, same convention as `signature_refs/tests.rs` beside it.

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

/// The koel defect, verbatim from `docs/plus/purchase-activation.md` (VitePress compiles the page to
/// a Vue SFC): front matter, prose, a fenced example, then the real `<script lang="ts" setup>` block
/// whose `import config from '../config'` is a live module edge that `docs/config.ts` was reported
/// dead for want of.
#[test]
fn vitepress_markdown_page_script_setup_is_an_edge_source() {
    let src = "---\nlayout: page\n---\n\n# Activation\n\n```bash\nphp artisan koel:init\n```\n\n\
               Reload Koel and it reverts.\n\n<script lang=\"ts\" setup>\nimport { onMounted } from 'vue'\n\
               import iconInfo from '../assets/icons/info.svg'\nimport config from '../config'\n\n\
               onMounted(() => { window.createLemonSqueezy() })\n</script>\n\n<style module>\n</style>\n";
    let m = extract_sfc_script_imports("docs/plus/purchase-activation.md", src);
    assert_eq!(m["config"].specifier, "../config");
    assert_eq!(m["onMounted"].specifier, "vue");
    assert_eq!(m["iconInfo"].specifier, "../assets/icons/info.svg");
}

/// A fenced code block DEMONSTRATING an SFC is NOT an edge, and this test's name and body were both
/// the opposite until 2026-08-20: the first version of this pass accepted the capture as a priced
/// cost, having priced it against `dead-candidates`/`unimported-export` — one file each. The same
/// value also reaches `find_unreachable` as an entry seed, and an entry seeds a FORWARD CLOSURE, so
/// the real price was a whole island per documentation sample. `strip_fenced_blocks` was added
/// instead, and this pin was inverted with it.
///
/// Two fences and a REAL block ride in one call, because "the fence is stripped" and "nothing in a
/// `.md` is read" are the same assertion without a live control — and it was the second that the
/// module was accused of doing for the whole of its first day on the roster.
#[test]
fn a_fenced_code_block_example_is_stripped_and_a_real_block_beside_it_is_not() {
    let src = "# Docs\n\n<script setup>\nimport { live } from './live'\n</script>\n\n\
               ```vue\n<script setup>\nimport { fenced } from './fenced'\n</script>\n```\n\n\
               ~~~vue\n<script setup>\nimport { tilde } from './tilde'\n</script>\n~~~\n";
    let m = extract_sfc_script_imports("docs/guide.md", src);
    assert_eq!(
        m["live"].specifier, "./live",
        "the page's real <script setup> block is still read"
    );
    assert!(
        !m.contains_key("fenced") && !m.contains_key("tilde"),
        "a fenced example is not an import, backtick or tilde: {m:?}"
    );
}

/// The loss channel the strip itself opens, pinned as a cost rather than left to be rediscovered:
/// a CommonMark fence with no closer runs to end of file, so a stray ``` above a real block takes
/// the block with it. Corpus incidence is 4 pages of 4021 and none of the 4 holds a `<script>` —
/// but a corpus that lacks a shape prices nothing, which is the lesson this whole pass came from,
/// so the shape is planted here instead. The control is the SAME page with the fence closed.
#[test]
fn an_unclosed_fence_blanks_the_rest_of_the_page() {
    let open =
        "# Doc\n\n```\nsome output\n\n<script setup>\nimport { real } from './real'\n</script>\n";
    let closed =
        "# Doc\n\n```\nsome output\n```\n\n<script setup>\nimport { real } from './real'\n</script>\n";
    assert!(
        extract_sfc_script_imports("docs/p.md", open).is_empty(),
        "an unclosed fence swallows the rest of the page — a LOST edge, never a wrong one"
    );
    assert_eq!(
        extract_sfc_script_imports("docs/p.md", closed)["real"].specifier,
        "./real",
        "the same page with the fence closed keeps its real edge"
    );
}

/// The shape the strip does NOT reach and the module doc names: a `<script>` inside an INLINE code
/// span is prose, but it is not a fenced block, so it still yields a binding. Suppression-direction
/// (it marks a possibly-dead export alive), and pinned so the disclosure and the behaviour cannot
/// drift apart the way the fence bullet did.
#[test]
fn a_script_inside_an_inline_code_span_is_still_captured() {
    let src = "# Doc\n\nAdd `<script>import a from './a'</script>` to your page.\n";
    let m = extract_sfc_script_imports("docs/p.md", src);
    assert_eq!(m["a"].specifier, "./a", "known cost, disclosed: {m:?}");
}

/// The strip is `.md`-only, and that gate carries weight — but not the weight first written down.
/// A raw BACKTICK run at line start cannot occur inside valid JS/TS at all (the first backtick
/// closes whatever template literal it is in), so the backtick half of the strip was never a threat
/// to a real script body. A TILDE run is ordinary text in a template literal, and `strip_fenced_
/// blocks` opens a fence on `~~~` exactly as it does on ```` ``` ````. So this is the shape that
/// makes the `.md` gate load-bearing, and it is asserted on BOTH rels in one call: identical bytes,
/// kept whole as `.vue`, blanked as `.md`.
#[test]
fn a_tilde_run_inside_a_template_literal_is_a_fence_only_in_markdown() {
    let src = "<script setup>\nconst doc = `\n~~~\n`;\nimport { real } from './real'\n</script>\n";
    let vue = extract_sfc_script_imports("src/App.vue", src);
    assert_eq!(
        vue["real"].specifier, "./real",
        "the .vue lane must not strip, or the tilde run eats the import below it: {vue:?}"
    );
    assert!(
        extract_sfc_script_imports("docs/p.md", src).is_empty(),
        "the same bytes in a .md ARE fenced from the tilde run on — the gate is the difference"
    );
}

/// The measured prose hazard (this repo's own `docs/rules/catalog.md`): a backticked `<script>`
/// mention and a `</script>` far below bracket a slab of markdown. swc cannot parse it, `parse_module`
/// returns `None`, and the extract yields NOTHING — the failure is cost, never a fabricated edge.
/// This is the assertion behind the module doc's "suppression-only" claim for markdown.
#[test]
fn prose_script_mention_bracketing_a_closing_tag_yields_nothing() {
    let src =
        "# Rules\n\n| rule | note |\n| --- | --- |\n| a | nothing splits the `<script>` block \
               here |\n\nMore prose, tables and | pipes | that are not TypeScript at all.\n\n\
               | b | a value containing `</script>` breaks out |\n";
    let m = extract_sfc_script_imports("docs/rules/catalog.md", src);
    assert!(
        m.is_empty(),
        "prose slab must contribute no bindings, got {m:?}"
    );
}

/// A `.md` with no script block at all — the overwhelming majority of every tree — costs one regex
/// pass and contributes nothing. Pins that widening the roster to markdown did not make prose files
/// start minting bindings.
#[test]
fn plain_prose_markdown_contributes_nothing() {
    let src = "# Title\n\nSome prose with `import x from 'y'` in backticks.\n\n```ts\nimport { a } from './a'\n```\n";
    let m = extract_sfc_script_imports("README.md", src);
    assert!(m.is_empty(), "no <script> tag means no bindings, got {m:?}");
}

#[test]
fn no_script_block_is_empty() {
    let src = "<template><div>hello</div></template>\n";
    let m = extract_sfc_script_imports("src/App.vue", src);
    assert!(m.is_empty());
}

#[test]
fn svelte_script_lang_ts_attribute() {
    let src = "<script lang=\"ts\">\nimport { onMount } from 'svelte'\n</script>\n<div>hi</div>\n";
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

/// The INDENT axis, which nothing else here constrains — found by a refutation pass that mutated
/// `strip_fenced_blocks`'s opening arm from `indent <= 3` to `indent == 0` and watched every other
/// test in this file and the engine's integration suite stay green while a real leak opened.
///
/// CommonMark lets a fence open under up to three leading spaces, and that is not a corner case: it is
/// what an ordinary numbered list looks like, where the example under `1. ` sits at three columns.
/// A docs site whose examples live inside steps is exactly the population the strip exists for, so a
/// pin set that only ever tested column-zero fences was pinning the easy half.
///
/// All four indents ride in one call: 0, 3 (the CommonMark limit, still a fence), and 4 — which is NOT
/// a fence but an indented code block, a shape `strip_fenced_blocks` deliberately does not model. The
/// fourth is asserted as the CAPTURED cost so the boundary is visible from both sides at once.
#[test]
fn a_fence_opens_under_up_to_three_spaces_of_indent_and_not_four() {
    let at = |pad: &str, name: &str| {
        format!(
            "# Doc\n\n{pad}```vue\n{pad}<script setup>\n{pad}import {{ {name} }} from './{name}'\n{pad}</script>\n{pad}```\n"
        )
    };
    for (pad, name) in [("", "zero"), (" ", "one"), ("   ", "three")] {
        let m = extract_sfc_script_imports("docs/p.md", &at(pad, name));
        assert!(
            m.is_empty(),
            "a fence indented {} space(s) still opens a fence: {m:?}",
            pad.len()
        );
    }
    // Four spaces is an INDENTED CODE BLOCK, not a fence — the strip does not model it, so the
    // `<script>` inside is still read. Disclosed in `strip_fenced_blocks`'s doc, asserted here.
    let m = extract_sfc_script_imports("docs/p.md", &at("    ", "four"));
    assert_eq!(
        m["four"].specifier, "./four",
        "known cost: a 4-space indented code block is not a fence, so its script is captured"
    );
}
