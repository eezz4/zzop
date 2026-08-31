//! Unit tests for the bare-ESM (`.mdx`) and frontmatter (`.astro`) import pre-scan arms. Split out of
//! `prescan.rs` for the 300-line cap, same convention as `sfc_imports/tests.rs` beside it.
//!
//! The `<script>`-block arm is not re-tested here — `sfc_imports/tests.rs` owns it. What this file pins
//! is the pair of arms that arm cannot reach, and above all that they stay TWO arms: MDX imports are
//! interleaved with prose and need a slicer plus a fence strip, Astro's are fenced by the dialect and
//! need neither, so the tests that would pass under one shared body are exactly the ones that must fail.

use super::{extract_prescan_imports, prescan_mode, PrescanMode, PRESCAN_IMPORT_HOSTS};

/// U1 — the ordinary MDX shape: YAML frontmatter, a bare top-level `import`, then prose and JSX. The
/// import is a real module edge; before this arm existed nothing read it and the target false-fired
/// `dead-candidates`/`unimported-export`.
#[test]
fn a_top_level_mdx_import_binds() {
    let src = "---\ntitle: Guide\n---\n\nimport { Callout } from '../src/callout'\n\n\
               # Guide\n\n<Callout>hi</Callout>\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert_eq!(m["Callout"].specifier, "../src/callout");
    assert_eq!(m["Callout"].original, "Callout");
    assert_eq!(m.len(), 1, "{m:?}");
}

/// U2 — a fenced example is documentation, not an edge. The one measured in-corpus fenced RELATIVE
/// specifier is `typeorm/docs/docs/performance-optimization/3-using-indexes.mdx:288`
/// (`import { User } from "./User"` inside a ```` ```typescript ```` fence); admitting it would mint a
/// fabricated edge AND seed `unreachable`'s forward closure off a documentation sample, which is the
/// cost `sfc_imports::extract_sfc_script_imports`'s doc measured for `.md`.
#[test]
fn a_fenced_mdx_example_import_is_not_an_edge() {
    let src = "import { live } from '../src/live'\n\n```tsx\nimport { fenced } from '../src/fenced'\n```\n\n\
               ~~~ts\nimport { tilde } from '../src/tilde'\n~~~\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert_eq!(
        m["live"].specifier, "../src/live",
        "the real import survives"
    );
    assert!(
        !m.contains_key("fenced") && !m.contains_key("tilde"),
        "a fenced example is not an import, backtick or tilde: {m:?}"
    );
}

/// U3 — a multi-line statement. Two of the 357 in-corpus `.mdx` import statements are this shape
/// (`typeorm/docs/docs/transactions.mdx:5`, `.../3-using-indexes.mdx:7`); a one-line-per-statement
/// slicer binds neither name and drops both edges.
#[test]
fn a_multi_line_mdx_import_binds_every_specifier() {
    let src = "import {\n  A,\n  B,\n} from '@site/x'\n\n# Title\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert_eq!(m["A"].specifier, "@site/x");
    assert_eq!(m["B"].specifier, "@site/x");
    assert_eq!(m.len(), 2, "{m:?}");
}

/// U4 — the ANTI-IMPLEMENTATION test. A real import followed by a prose line that also starts with
/// `import` at column 0, verbatim from `fastapi/docs/es/docs/tutorial/server-sent-events.md:40`. The
/// obvious implementation — concatenate every `^import` line and parse the lot once — hands swc a
/// syntax error, gets `None` back, and loses the WHOLE page's real imports. Four such prose lines were
/// measured in the corpus's `.md` files (`rg -n '^import\b'` outside fences); the terminator test is
/// what tells them from a statement.
#[test]
fn a_prose_line_starting_with_import_does_not_take_the_real_import_with_it() {
    let src = "import { live } from '../src/live'\n\n\
               import `EventSourceResponse` de `fastapi.sse`:\n\nmore prose\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert_eq!(m["live"].specifier, "../src/live");
    assert_eq!(m.len(), 1, "prose must be dropped, not parsed: {m:?}");
}

/// U5 — a side-effect import binds no name, so it enters under the synthetic
/// `imports::side_effect_key` spelling. This shape was PLANTED before it was measured, and which way
/// round that went is worth saying: the corpus held ZERO of them until `withastro/astro` was cloned
/// into it on 2026-08-21, and holds 5 now — all CSS side-effect imports in Astro's own MDX fixtures
/// (`^imports*['\"]` at column 0 across the 218 files). The terminator regex's SECOND alternative is
/// what admits them; without it every one is prose.
#[test]
fn a_side_effect_mdx_import_binds_the_synthetic_key() {
    let src = "import './polyfill'\n\n# Title\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert_eq!(m.len(), 1, "{m:?}");
    let binding = m.values().next().expect("one binding");
    assert_eq!(binding.specifier, "./polyfill");
    assert_eq!(binding.original, "_");
}

/// U6 — column 0 is the statement gate. Measured: all 357 in-corpus `.mdx` import statements start at
/// column 0 and NONE is indented 1-3 spaces, so the rule loses nothing real while keeping the slicer
/// out of indented code blocks and list-item prose.
#[test]
fn an_indented_mdx_import_is_not_a_statement() {
    let src = "# Title\n\n    import { indented } from '../src/indented'\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert!(m.is_empty(), "{m:?}");
}

/// U7 — `.mdx` does NOT ride the `<script>`-block arm. The block's import is indented on purpose: that
/// is what makes this an assertion about the ARM rather than about the column-0 rule (U6 owns that).
/// A column-0 `import` inside a `<script>` tag in an MDX WOULD bind through the bare-ESM arm, and that
/// is a real import rather than a defect — stated so the silence here is not read as wider than it is.
#[test]
fn a_script_block_in_an_mdx_is_not_read_as_a_script_block() {
    let src = "# Title\n\n<script setup>\n  import { blocked } from '../src/blocked'\n</script>\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert!(m.is_empty(), "{m:?}");
}

/// U8 — the two extensions read the same bytes differently, and the difference is the whole point of
/// the second arm: `.md` puts its imports in a `<script>` block and has none here, `.mdx` puts them at
/// top level and has one.
#[test]
fn the_same_bytes_read_nothing_as_md_and_bind_as_mdx() {
    let src = "# Page\n\nimport { x } from '../src/x'\n";
    assert!(extract_prescan_imports("docs/p.md", src).is_empty());
    assert_eq!(
        extract_prescan_imports("docs/p.mdx", src)["x"].specifier,
        "../src/x"
    );
}

/// U9 — the Astro frontmatter fence is real TypeScript by construction, so the slab goes straight to
/// `parse_imports`; no slicer, and prose cannot appear inside it.
#[test]
fn an_astro_frontmatter_import_binds() {
    let src =
        "---\nimport Layout from '../layouts/Base.astro'\nimport { fetchAll } from '../lib/api'\n\
               const posts = await fetchAll()\n---\n<Layout>{posts.length}</Layout>\n";
    let m = extract_prescan_imports("src/pages/index.astro", src);
    assert_eq!(m["Layout"].specifier, "../layouts/Base.astro");
    assert_eq!(m["fetchAll"].specifier, "../lib/api");
    assert_eq!(m.len(), 2, "{m:?}");
}

/// U10 — below the closing `---` is the template, not the module. A `<script>` there is a CLIENT
/// script this arm deliberately does not read (`sfc_imports`'s roster doc owns why that 1% is the
/// wrong 1%), and a bare `import` there is not valid Astro at all.
#[test]
fn an_astro_import_below_the_closing_fence_is_not_read() {
    let src = "---\nimport Layout from '../layouts/Base.astro'\n---\nimport { below } from '../src/below'\n";
    let m = extract_prescan_imports("src/pages/index.astro", src);
    assert_eq!(m["Layout"].specifier, "../layouts/Base.astro");
    assert!(!m.contains_key("below"), "{m:?}");
}

/// U11 — the FALSE-COMMONALITY pin. One byte sequence, three extensions, three different answers.
/// `.md` sees no `<script>` block; `.mdx` slices the bare statement out of the prose; `.astro` reads
/// only between the fences, where this file has YAML and no import. Fold the arms into one body and
/// one of these three has to become a lie.
#[test]
fn one_byte_sequence_three_extensions_three_answers() {
    let src = "---\ntitle: G\n---\nimport { x } from './x'\n";
    assert!(extract_prescan_imports("docs/p.md", src).is_empty(), "md");
    assert_eq!(
        extract_prescan_imports("docs/p.mdx", src)["x"].specifier,
        "./x",
        "mdx"
    );
    assert!(
        extract_prescan_imports("src/p.astro", src).is_empty(),
        "astro"
    );
}

/// U12 — no closing fence means no frontmatter to read. Emitting the rest of the file instead would
/// hand `parse_imports` a template, and a lost edge is the disclosed direction here (same shape as
/// `sfc_imports`'s `an_unclosed_fence_blanks_the_rest_of_the_page`).
#[test]
fn an_unclosed_astro_fence_yields_nothing() {
    let src = "---\nimport Layout from '../layouts/Base.astro'\n<Layout />\n";
    assert!(extract_prescan_imports("src/pages/i.astro", src).is_empty());
}

/// U13 — Astro requires the fence at the very top. A file that opens with anything else has no
/// frontmatter, and scanning forward for a `---` would find a markdown thematic break or a YAML
/// document separator instead.
#[test]
fn an_astro_file_without_a_leading_fence_yields_nothing() {
    let src = "<h1>hi</h1>\n---\nimport { x } from './x'\n---\n";
    assert!(extract_prescan_imports("src/pages/i.astro", src).is_empty());
}

/// U14 — the fence strip is NOT applied to `.astro`, and this is the shape that proves it matters: a
/// `~~~` run at line start inside a template literal is ordinary TypeScript text, and blanking from
/// there to end of file would eat both the real import and the literal's closing backtick.
/// `sfc_imports::extract_sfc_script_imports`'s doc owns the same argument for `.vue`/`.svelte`.
#[test]
fn a_tilde_run_inside_an_astro_template_literal_is_not_a_fence() {
    let src = "---\nconst rule = `\n~~~\n`;\nimport { after } from '../src/after'\n---\n<div />\n";
    let m = extract_prescan_imports("src/pages/i.astro", src);
    assert_eq!(m["after"].specifier, "../src/after", "{m:?}");
}

/// The roster is a pair table, not a set: a filetype that gains an entry gains an ARM with it, and a
/// mode assigned by hand is the one thing a compile cannot check. Pins both halves.
#[test]
fn the_prescan_roster_pairs_every_host_with_its_arm() {
    assert_eq!(
        PRESCAN_IMPORT_HOSTS,
        [
            ("vue", PrescanMode::ScriptBlocks),
            ("svelte", PrescanMode::ScriptBlocks),
            ("md", PrescanMode::ScriptBlocks),
            ("mdx", PrescanMode::BareEsm),
            ("astro", PrescanMode::AstroFence),
        ],
        "roster drifted — update deliberately"
    );
    assert_eq!(
        prescan_mode("MDX"),
        Some(PrescanMode::BareEsm),
        "case-insensitive"
    );
    assert_eq!(prescan_mode("Astro"), Some(PrescanMode::AstroFence));
    assert_eq!(prescan_mode("ts"), None);
    assert_eq!(prescan_mode("png"), None);
}

/// A filetype with no pre-scan arm reads nothing, and a path with no extension at all does not panic.
#[test]
fn a_non_host_extension_yields_nothing() {
    let src = "import { x } from './x'\n";
    assert!(extract_prescan_imports("src/a.ts", src).is_empty());
    assert!(extract_prescan_imports("Makefile", src).is_empty());
}

/// The residual `bare_esm_imports`'s doc discloses, pinned at its BOUNDED radius — the same job
/// `sfc_imports`'s `an_unclosed_fence_blanks_the_rest_of_the_page` does for its own loss channel.
///
/// The trigger is NOT "a prose line": it is any COLUMN-0 opener whose accumulated text fails
/// `terminates_statement`, and a REAL import reaches that state whenever anything follows its closing
/// quote — here a trailing `//` comment, which the terminator's `\s*;?\s*$` tail rejects. So `Foo` is
/// dropped, and that half is the disclosed loss.
///
/// **What this test exists to hold is that `Bar` survives it.** Without the walk's new-opener guard the
/// accumulation ran past `Foo`, closed on `Bar`'s specifier, dragged the prose between them into the
/// slab, and swc rejected the slab whole — measured on these exact bytes, an EMPTY map: every edge on
/// the page gone for one trailing comment. The guard turns a page-sized loss into a statement-sized
/// one, and the two assertions below are exactly what tells the behaviours apart — an unguarded walk
/// fails the first, and a walk that discarded nothing would fail the second.
#[test]
fn an_opener_that_never_terminates_loses_only_itself() {
    let src =
        "import Foo from './foo' // used below\n\nsome prose line\n\nimport Bar from './bar'\n";
    let m = extract_prescan_imports("docs/guide.mdx", src);
    assert_eq!(
        m["Bar"].specifier, "./bar",
        "the guard must END the failed accumulation at the next opener, not let it swallow one: {m:?}"
    );
    assert!(
        !m.contains_key("Foo"),
        "and the opener that never terminated is still dropped — the disclosed loss, now one \
         statement wide rather than one page: {m:?}"
    );
    assert_eq!(m.len(), 1, "{m:?}");

    // The CONTROL that makes the pair above a statement about the TERMINATOR rather than about
    // comments in general: move the comment to its own line and both edges bind.
    let ok =
        "// used below\nimport Foo from './foo'\n\nsome prose line\n\nimport Bar from './bar'\n";
    let m2 = extract_prescan_imports("docs/guide.mdx", ok);
    assert_eq!(m2["Foo"].specifier, "./foo");
    assert_eq!(m2["Bar"].specifier, "./bar");
}
