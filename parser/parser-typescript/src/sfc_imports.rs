//! SFC `<script>`-block import pre-scan — `.vue`/`.svelte`, and every other filetype a framework
//! compiles to one ([`SFC_SCRIPT_HOST_EXTENSIONS`] is the roster and lives here). A lexical block
//! extract (NOT a full SFC/template parse) feeding the EXISTING `parse_imports` — the import syntax is
//! identical across `<script>`/`<script setup>`/`<script lang="ts">` (all ES `import`), and swc parses the
//! TS-shaped concatenated text, so no SFC-flavor-specific handling is needed here.
//!
//! ## Why this exists
//! SFC-host files reach the engine as lexical-only `SourceFile`s (dispatch routes them to `None` —
//! `zzop_engine::dispatch`), so a `.ts` symbol imported and used ONLY inside a component's `<script>`
//! block has zero visible fan-in through the normal fused pipeline, false-firing `unimported-export`/
//! `dead-candidates`. This helper is called by the engine at ASSEMBLE time (uncached, off-disk, mirroring
//! `dead_exports.rs`'s own re-read-off-disk pattern) so the win adds no cached FIELD and moves no
//! `CACHE_SCHEMA_VERSION`: it never touches the cached fused-pipeline projection for the SFC-host file
//! itself. That is a statement about the cache's SHAPE, not about invalidation — since the 2026-07-29
//! derived-fingerprint change, editing any `.rs` under this crate moves `FP_TYPESCRIPT` and wipes the
//! TypeScript lane in every tree (`crates/engine/build.rs`, `cache::parser_fingerprint`). Accepted
//! over-invalidation, and worth spelling out because "no fingerprint bump" used to mean "nobody has to
//! remember to bump one" and now reads as "nothing is invalidated", which is the opposite of true.
//!
//! A `.vue` file may carry TWO script blocks (`<script>` for options-API exports plus `<script setup>`
//! for the component body) — both are extracted and concatenated before the single `parse_imports` call,
//! so imports from either block are captured.
//!
//! ## Which files this runs over
//! [`extract_sfc_script_imports`] itself takes text and returns imports — it never looks at a path's
//! extension. The roster of filetypes it is RUN over is [`SFC_SCRIPT_HOST_EXTENSIONS`], one module down,
//! and that doc owns the membership question whole — including why `.md` is on it, which is the one
//! entry a reader will not guess. Nothing about the extract below changes per filetype.
//!
//! This roster is now ONE ARM of a wider dispatch: `crate::prescan` pairs each pre-scanned extension
//! with the reader that can find its imports, and `.mdx`/`.astro` sit on the OTHER two arms rather than
//! here. That module is the engine's entry point; this one is what it calls for a `<script>`-block host.
//!
//! ## Known lexical limits (bounded on purpose)
//! Because this is a raw block extract and not a real SFC parser, a `<script>…import…</script>` that is
//! commented out (`<!-- … -->`) or embedded inside a template string is still captured, so its imports
//! count as live. The failure direction is conservative: it can only SUPPRESS a `unimported-export`/
//! `dead-candidates` finding (mark a possibly-dead export alive), never mint a new false positive — an
//! acceptable trade for a pre-scan whose whole job is removing SFC-blindness false positives.
//!
//! Markdown brings a hazard the two component filetypes do not have, because a docs page's whole job is
//! to SHOW code it does not run — and unlike the limits above, this one does not stay in the
//! suppression direction:
//! - A FENCED example is the shape that made the trade stop being conservative, and it is the one shape
//!   here that is FIXED rather than accepted: [`extract_sfc_script_imports`] blanks fences before
//!   scanning. Its doc owns the model, the island the capture erased when it was priced against the
//!   wrong consumer, and what the strip deliberately does not reach — not restated here, because a
//!   second copy of that model is the thing that goes stale first (it did: this bullet asserted the
//!   opposite behaviour for a day after the strip landed).
//! - Two shapes the strip does NOT touch, both still captured, both suppression-direction like the
//!   comment case above: a `<script>` inside an HTML comment, and one inside an INLINE CODE SPAN —
//!   "add `` `<script>import a from './a'</script>` `` to your page" is an ordinary docs sentence, and
//!   it yields a real binding.
//! - The direction that is a LOSS rather than a suppression: prose mentioning `` `<script>` `` ABOVE a
//!   real block pairs the first opening tag with the first closing one, swallowing the block between
//!   them. This repo's own `docs/rules/catalog.md` has the bracketing shape. `parse_module` is
//!   panic-guarded and returns `None` on the resulting slab, so the result is an empty `ImportMap` —
//!   never a wrong edge, but the page's REAL imports go with it, which means the false positive this
//!   whole pass exists to remove survives on precisely the docs pages that discuss `<script>`. Measured
//!   on koel `3f5213d4`: 0 of the 10 real blocks are shadowed this way.
//!   `prose_script_mention_bracketing_a_closing_tag_yields_nothing` pins the direction.

use zzop_core::ImportMap;

use crate::imports::parse_imports;

/// The file extensions a framework compiles into a Vue Single-File Component, so a `<script>` /
/// `<script setup>` block inside one is real module source and its `import` statements are real module
/// edges. **The single owner of THIS roster** — [`extract_sfc_script_imports`] runs over exactly these
/// files. It is NOT the roster of everything the engine pre-scans; that is
/// [`crate::PRESCAN_IMPORT_HOSTS`], which pairs each extension with the arm that reads it and carries
/// this list as its `ScriptBlocks` third. Membership HERE is a property of `script_blocks`'s reach
/// rather than of how a file is classified: an entry qualifies only if the dialect puts its imports
/// inside a `<script>` tag, which is a question only this module's regex can answer.
///
/// A THIRD extension axis, deliberately not derivable from either half of
/// `zzop_engine::dispatch`'s classification — it cuts across them. `vue`/`svelte` are absent from
/// `NON_SOURCE_EXTENSIONS` entirely (they are plausible parser-adapter targets and must keep warning),
/// while `md` is IN it as `NoFactsToLose` and is an SFC host all the same: VitePress and Nuxt Content
/// compile every markdown page to a Vue SFC. Neither of `md`'s answers over there changes — that
/// enum's doc owns why, and this list is what makes its claim true rather than aspirational.
///
/// Measured, koel `3f5213d4`: ten `docs/**/*.md` pages carry a `<script lang="ts" setup>` block, and
/// `docs/plus/purchase-activation.md:75` holds `import config from '../config'`. With `md` off this
/// roster `docs/config.ts` was reported `dead-candidates` — a file with a live importer, unread because
/// of the importer's extension.
///
/// The consumer is [`crate::extract_prescan_imports`]'s `ScriptBlocks` arm. The engine never reads this
/// list at all — its gate (`analyze::assemble::helpers::is_prescan_ext`) asks `prescan_mode`, so the
/// "no second extension list at the call site" rule is [`crate::PRESCAN_IMPORT_HOSTS`]'s to state and
/// is stated there. What this const still owns is the narrower question: which filetypes the
/// `<script>`-block extract below can actually see.
///
/// `mdx` and `astro` are ABSENT from THIS list and present on the other two arms, for two DIFFERENT
/// reasons that were measured separately after a single sentence claiming both was found to be half
/// wrong. Neither is an exclusion from the pre-scan any more; both are statements about what
/// `script_blocks` can see:
/// - **MDX — reach.** It carries its imports as bare top-level ESM, so `script_blocks` genuinely finds
///   nothing. Measured across the dogfood corpus: 1 of 218 `.mdx` files contains a `<script` at all
///   (a raw-passthrough test fixture), while 175 of 218 carry a top-level `^import`. A roster entry
///   here would buy one file's worth of edges at most; what buys the rest is the bare-ESM arm, which
///   is a different reader, not a row on this list.
/// - **Astro — completeness, NOT reach.** `script_blocks` does reach an Astro CLIENT script, and that
///   is the wrong half of the dialect. Measured over `corpus/frameworks/astro` (1539 `.astro` files;
///   `find corpus ( -name .git -o -name node_modules -o -name .zzop ) -prune -o -type f -iname
///   '*.astro' -print | wc -l`): 1438 import statements sit inside the leading `---` frontmatter fence
///   across 827 files, against 31 import lines below a closing fence — so a `<script>`-block reading of
///   Astro would cover about 2% of its import sites while every local-path edge stayed invisible. The
///   frontmatter arm (`crate::prescan`'s `AstroFence`) is what reads the other 98%, and it deliberately
///   leaves the client `<script>` unread rather than mixing the two.
pub const SFC_SCRIPT_HOST_EXTENSIONS: &[&str] = &["vue", "svelte", "md"];

/// True if `ext` (no leading dot) names a member of [`SFC_SCRIPT_HOST_EXTENSIONS`]. Case-insensitive,
/// mirroring the engine's own extension normalization — a caller need not pre-lowercase `ext`.
pub fn is_sfc_script_host(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    SFC_SCRIPT_HOST_EXTENSIONS.contains(&lower.as_str())
}

/// Extracts every `<script ...>...</script>` block's text from an SFC-host file's source, in
/// document order, concatenated with a newline separator (so a name in one block never collides with a
/// name from another mid-line), then runs the EXISTING `parse_imports(rel, ...)` on the concatenated text.
/// Returns an empty `ImportMap` when the file has no `<script>` block at all.
///
/// Deliberately lexical (a simple DOTALL block extract), not a real SFC/template parser: the only thing
/// this needs from the file is the raw text between `<script ...>` and `</script>`, and swc parses that
/// text exactly as it would a standalone `.ts` module regardless of `lang="ts"`/`setup` attributes on the
/// opening tag (those attributes are never inspected).
///
/// # Markdown fenced blocks are stripped first, and that is load-bearing
/// A `.md` page is prose, so a `<script>` inside a ```` ```vue ```` fence is an EXAMPLE, not an edge.
/// Admitting one is not a small over-count: the resolved target is fed to `merge_prescan_fan_in`, which
/// both bumps `fan_in` AND seeds `find_unreachable`'s entry set, and that rule takes the FORWARD
/// CLOSURE of its entries. Measured before this strip existed — a five-line `docs/guide.md` example
/// naming `../src/a` erased `unreachable` on `src/a.ts` **and** on `src/b.ts`, which the example never
/// mentions and which is only reachable from `a`. One documentation sample silenced a whole island.
///
/// The first version of this pass priced that cost against `dead-candidates`/`unimported-export` — one
/// file, no closure — because those were the consumers being read at the time. Two consumers of one
/// value, and the wider one was the one not measured.
///
/// Stripping is CommonMark-shaped rather than exact: a fence opens on a run of 3+ backticks or tildes
/// (indented up to 3 spaces) and closes on the first later run of the SAME character that is at least
/// as long, or at end of file. Info strings are ignored. What this does NOT model is stated rather than
/// discovered: fences nested inside list items or block quotes at deeper indents, and `<script>` inside
/// an INDENTED (4-space) code block. Both would still be read as edges. Non-`.md` hosts are not
/// stripped at all — `.vue`/`.svelte` have no markdown fences, and running the strip over them would
/// let a fence run inside a template literal eat real script. Precisely: a raw BACKTICK run at line
/// start cannot occur inside valid JS/TS (the first backtick closes the literal it sits in), so that
/// half was never a threat; a TILDE run is ordinary template-literal text and opens a fence here just
/// the same, which is what makes the gate load-bearing rather than defensive
/// (`a_tilde_run_inside_a_template_literal_is_a_fence_only_in_markdown` asserts both rels at once).
///
/// **The strip opens one loss channel of its own**, which is the honest price of closing the closure
/// one: "or at end of file" means a STRAY UNCLOSED fence blanks every line below it, so a real
/// `<script>` block after one is lost. That is CommonMark-correct and it is still a loss. Measured
/// across the dogfood corpus's 4021 `.md` files: 4 end with an open fence, none of the 4 holds a
/// `<script>`, and all 9 corpus `.md` that do hold one yield the same bindings before and after the
/// strip. The direction is a lost edge, not a fabricated one, which is why it is disclosed rather than
/// modelled around — `an_unclosed_fence_blanks_the_rest_of_the_page` pins it so nobody rediscovers it
/// as a mystery.
pub fn extract_sfc_script_imports(rel: &str, text: &str) -> ImportMap {
    let stripped;
    let scan = if rel
        .rsplit('.')
        .next()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        stripped = strip_fenced_blocks(text);
        stripped.as_str()
    } else {
        text
    };
    let mut combined = String::new();
    for block in script_blocks(scan) {
        combined.push_str(block);
        combined.push('\n');
    }
    if combined.trim().is_empty() {
        return ImportMap::new();
    }
    parse_imports(rel, &combined)
}

/// Blanks out CommonMark fenced code blocks, preserving line count and every non-fenced byte, so a
/// `<script>` shown as a documentation EXAMPLE never becomes a module edge. See
/// [`extract_sfc_script_imports`] for the measured cost of not doing this.
///
/// Fence content is replaced by empty lines rather than removed so any later byte offset or line
/// number derived from this text still lines up with the file on disk.
pub(crate) fn strip_fenced_blocks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<(char, usize)> = None;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let indent = body.len() - body.trim_start_matches(' ').len();
        let rest = &body[indent..];
        let marker = rest.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = marker.map_or(0, |m| rest.chars().take_while(|c| *c == m).count());
        match fence {
            // Closing fence: same character, at least as long, nothing but the run on the line.
            Some((open_char, open_len))
                if marker == Some(open_char)
                    && run >= open_len
                    && rest[run..].trim().is_empty()
                    && indent <= 3 =>
            {
                fence = None;
                push_blank(&mut out, line);
            }
            Some(_) => push_blank(&mut out, line),
            None if run >= 3 && indent <= 3 => {
                fence = Some((marker.expect("a run implies a marker"), run));
                push_blank(&mut out, line);
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Replaces `line` with its line terminator alone, keeping the file's line count intact.
fn push_blank(out: &mut String, line: &str) {
    if line.ends_with("\r\n") {
        out.push_str("\r\n");
    } else if line.ends_with('\n') {
        out.push('\n');
    }
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
mod tests;
