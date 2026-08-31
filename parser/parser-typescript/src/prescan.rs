//! Import PRE-SCAN dispatch — the one entry point the engine calls for every filetype that hides real
//! module edges somewhere a structural parser frontend never looks.
//!
//! Three dialect families put their `import` statements in three different places, and this module is
//! the table that says which is which plus the arm that reads each. It is a DISPATCH and deliberately
//! not a shared body: `<script>`-block hosts, MDX and Astro share a return type and a wiring seam, not
//! an algorithm, and the tests that pin them apart (`prescan/tests.rs`'s
//! `one_byte_sequence_three_extensions_three_answers`) are exactly the ones a folded body has to fail.
//!
//! ## Why a pre-scan exists at all
//! Every host here dispatches to `None` (`zzop_engine::dispatch` has no arm for any of them), so a `.ts`
//! symbol imported and used ONLY from one of these files has zero visible fan-in through the normal
//! fused pipeline and false-fires `unimported-export`/`dead-candidates` — and, because the resolved
//! targets also seed `find_unreachable`'s entry set, a whole island behind such a target reads as dead.
//! The engine calls this at ASSEMBLE time, uncached, off disk (`analyze::assemble::prescan`), so the win
//! adds no cached FIELD and moves no `CACHE_SCHEMA_VERSION`. That is a statement about the cache's
//! SHAPE, not its invalidation: `crates/engine/build.rs` hashes this crate's sources into
//! `FP_TYPESCRIPT`, so editing this file wipes the TypeScript lane in every tree.
//!
//! **This module reads NOTHING but the `(rel, text)` it is handed** — no data file, no env var, no
//! build-time fetch. An input outside `FP_TYPESCRIPT`'s closure would make the cache key stop tracking
//! behaviour.

use zzop_core::ImportMap;

use crate::imports::parse_imports;
use crate::sfc_imports::{extract_sfc_script_imports, strip_fenced_blocks};

/// WHERE a filetype keeps its `import` statements — the axis this table is sorted on, and the reason it
/// is a pair table rather than a set. A one-list roster answering "is this file pre-scanned?" would have
/// to answer "with which reader?" by a second hand-kept list; that is the drift
/// `dispatch::non_source::NON_SOURCE_EXTENSIONS` adopted a pair table to close, and this follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrescanMode {
    /// Imports sit inside `<script ...>` blocks — a framework compiles the file to a Vue SFC.
    /// Read by [`crate::extract_sfc_script_imports`], whose doc owns that arm's reach and limits.
    ScriptBlocks,
    /// Imports are BARE top-level ESM, interleaved with prose. Needs a statement slicer (prose can
    /// start with the word `import` too) and a markdown fence strip (a fenced example is not an edge).
    BareEsm,
    /// Imports sit in a LEADING `---` frontmatter fence, which is real TypeScript by construction —
    /// no slicer, and deliberately no fence strip.
    AstroFence,
}

/// The filetypes an import pre-scan runs over, each paired with the arm that reads it. Membership is
/// bounded by whether the dialect has a place this crate can find its imports in, not by taste.
///
/// A THIRD extension axis, deliberately not derivable from either half of `zzop_engine::dispatch`'s
/// classification — it cuts across them. `vue`/`svelte`/`astro` are absent from `NON_SOURCE_EXTENSIONS`
/// entirely (they are plausible parser-adapter targets and must keep warning), while `md` and `mdx` are
/// IN it as `NoFactsToLose` and are pre-scan hosts all the same. Neither of their answers over there
/// changes — that enum's doc owns why, and this list is what makes its claim true rather than
/// aspirational for both of them.
///
/// Measured incidence, in-repo dogfood corpus. Write `FIND='find corpus \( -name .git -o -name
/// node_modules -o -name .zzop \) -prune -o -type f'` for the walk every count below shares.
/// - **`.mdx`**: `$FIND -iname '*.mdx' -print | wc -l` -> **218** files, of which 175 carry a top-level
///   `^import` and exactly **1** contains a `<script` at all (`… -print0 | xargs -0 grep -l '<script' |
///   wc -l` -> 1, `astro/packages/integrations/mdx/test/fixtures/mdx-basics/src/pages/
///   script-style-raw.mdx`, a raw-passthrough fixture; control: the same pipe over `-iname '*.md'` ->
///   19, so the counter is not stuck). An SFC roster row would have bought one file's worth of edges
///   at most, which is why `mdx` is a second ARM rather than a sixth name on the `<script>` list.
/// - **`.astro`**: `$FIND -path '*/astro/*' -iname '*.astro' -print | wc -l` -> **1539** (control: the
///   same command with `-iname '*.ts'` -> 2129), all under `corpus/frameworks/astro`. Of those, 1224
///   open with a `---` line (`… -print0 | xargs -0 awk 'FNR==1 && /^---\r?$/{print FILENAME}' |
///   wc -l`), and inside those fences sit **1438** import statements across **827** files (the same
///   `xargs -0 awk` with the fence state machine, printing `FILENAME":"FNR` and counting lines, then
///   unique first fields). The number that makes "frontmatter only" the right scope rather than a
///   shortcut is the control: **31** import lines sit BELOW a closing fence, i.e. under 3% of the total
///   — and 135 files carry a `<script` tag at all.
///
/// This roster is a POLICY-SHAPED constant carrying axis `fact` in `scripts/policy-census.txt`: which
/// filetype keeps its imports where is fixed by the frameworks that define the dialects, not named by
/// any project, exactly like `NUXT_CONFIG_FILES` two entries away.
///
/// The consumer is the engine's `analyze::assemble::helpers::is_prescan_ext`, the gate that fills
/// `Collected::prescan_rels` and therefore decides which files ever reach [`extract_prescan_imports`].
/// It must delegate to [`prescan_mode`]; a second extension list spelled there is the drift this const
/// exists to prevent, and while one exists the roster below is only half-true on the wire.
pub const PRESCAN_IMPORT_HOSTS: &[(&str, PrescanMode)] = &[
    ("vue", PrescanMode::ScriptBlocks),
    ("svelte", PrescanMode::ScriptBlocks),
    ("md", PrescanMode::ScriptBlocks),
    ("mdx", PrescanMode::BareEsm),
    ("astro", PrescanMode::AstroFence),
];

/// The [`PrescanMode`] for `ext` (no leading dot), or `None` when no pre-scan reads that filetype.
/// Case-insensitive, mirroring the engine's own extension normalization — a caller need not
/// pre-lowercase `ext`.
pub fn prescan_mode(ext: &str) -> Option<PrescanMode> {
    let lower = ext.to_ascii_lowercase();
    PRESCAN_IMPORT_HOSTS
        .iter()
        .find(|(e, _)| *e == lower)
        .map(|(_, mode)| *mode)
}

/// Extracts a pre-scan host's import bindings, dispatching on `rel`'s extension through
/// [`PRESCAN_IMPORT_HOSTS`]. A path whose extension is on no row (or that has none) reads nothing.
///
/// The EXTENSION gate is load-bearing for the Astro arm and not merely a router: `.md`/`.mdx` pages open
/// with `---` too, and theirs is YAML. Handing YAML to `parse_imports` is not a small error — swc fails
/// on the slab and the page's real imports go with it.
pub fn extract_prescan_imports(rel: &str, text: &str) -> ImportMap {
    let Some(ext) = std::path::Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
    else {
        return ImportMap::new();
    };
    match prescan_mode(ext) {
        Some(PrescanMode::ScriptBlocks) => extract_sfc_script_imports(rel, text),
        Some(PrescanMode::BareEsm) => bare_esm_imports(rel, text),
        Some(PrescanMode::AstroFence) => astro_frontmatter_imports(rel, text),
        None => ImportMap::new(),
    }
}

/// Maximum lines one bare-ESM statement may span before it is discarded as prose. Generous against the
/// measured shape (the two multi-line statements in the corpus span 4 lines each) and small enough that
/// a prose line starting with `import` cannot reach a real statement further down the page.
const BARE_ESM_MAX_STATEMENT_LINES: usize = 20;

/// The BARE-ESM arm — `.mdx`. Slices real `import` statements out of a prose page and hands the slab to
/// `parse_imports`.
///
/// **The whole text cannot go to `parse_imports`.** `parse::parse_uncached` enables `tsx` only for
/// `.tsx`/`.jsx`/`.js`/`.mjs`/`.cjs`, so an `.mdx` rel parses with `tsx: false` over YAML frontmatter,
/// prose and JSX — swc errors and the map comes back empty. Only the statements are handed over.
///
/// Two gates decide what a statement is, and both are measured rather than assumed:
/// - **Fences are stripped first** (`sfc_imports::strip_fenced_blocks`, the same one the `.md` arm uses
///   and the reason it is shared rather than reimplemented). Measured over the in-repo corpus: 357
///   `^import` statements sit outside a fence and 86 inside; 234 RELATIVE specifiers sit outside and
///   exactly ONE inside (`corpus/frameworks/typeorm/docs/docs/performance-optimization/
///   3-using-indexes.mdx:288`, `import { User } from "./User"` in a ```` ```typescript ```` fence). That
///   one would be a FABRICATED edge, and it would also seed `find_unreachable`'s forward closure — the
///   cost `extract_sfc_script_imports`'s doc priced for `.md`.
/// - **A statement starts at COLUMN 0 on a line matching `^import\b` and must TERMINATE.** The column
///   rule loses nothing: all 357 in-corpus statements start at column 0 and NONE is indented 1-3 spaces.
///   The terminator test is what a naive "every `^import` line" slicer lacks, and the counter-evidence
///   is in the sibling filetype: the corpus's `.md` files hold 4 outside-fence lines matching
///   `^import\b` and ALL FOUR are prose (`import-schema:`, `` import `EventSourceResponse` de
///   `fastapi.sse`: `` twice, and a Korean sentence). Concatenating one of those into the slab makes swc
///   fail and the WHOLE page's real imports are lost — which is why
///   `a_prose_line_starting_with_import_does_not_take_the_real_import_with_it` is an
///   anti-implementation test rather than a nicety.
///
/// Validated on the in-repo corpus with a line-for-line port of this rule: `.mdx` accepts 357 and
/// rejects 0; `.md` accepts 0 and rejects the 4 prose lines; and no accepted statement's span contains
/// another column-0 `^import` line, so no real statement is ever swallowed by a prose one.
///
/// **Residual, disclosed rather than modelled around, and stated by MECHANISM because its surface
/// form misleads.** The trigger is not "a prose line": it is ANY column-0 opener whose accumulated
/// text fails [`terminates_statement`], and a REAL import reaches that state whenever anything follows
/// its closing quote — `import Foo from './foo' // used below` is rejected by the trailing
/// `\s*;?\s*$`. Such an opener is DISCARDED, and the guard inside the walk bounds the damage to
/// exactly that one statement: the next column-0 opener ENDS the accumulation instead of being
/// swallowed by it, so a page whose first import carries a trailing comment loses that edge and keeps
/// every other one. `an_opener_that_never_terminates_loses_only_itself` pins both halves in one call.
/// Loss-only in direction — a dropped statement, never a fabricated edge.
///
/// The RADIUS is the point, because it used to be the PAGE: without that guard the walk ran past the
/// failed opener, closed on the NEXT statement's specifier, dragged whatever sat between them into the
/// slab, and swc rejected the slab whole. Measured on those exact bytes before the guard landed: an
/// EMPTY map where there is now one binding. A trailing comment on an import is not an exotic shape,
/// and losing a page is a different CLASS of error from the ordinary under-approximations this module
/// already makes — which is why the guard is here rather than the disclosure alone.
///
/// Neither version is visible to the corpus, stated rather than left implied: with the guard and
/// without it alike, `.mdx` accepts 357 and rejects 0, `.md` accepts 0 and rejects 4, and the swallow
/// counter (an accepted statement whose span holds another column-0 `^import`) is 0. It costs no
/// accuracy on anything we can see and bounds the radius on everything we cannot. The tighter
/// in-corpus bound is a separate count, since that total counts statements and not span width: exactly
/// 2 of the 357 span more than one line, both genuine multi-line `import { … } from "@site/x"` shapes.
fn bare_esm_imports(rel: &str, text: &str) -> ImportMap {
    let stripped = strip_fenced_blocks(text);
    let lines: Vec<&str> = stripped.split('\n').collect();
    let mut slab = String::new();
    let mut i = 0;
    while i < lines.len() {
        if !starts_bare_import(lines[i]) {
            i += 1;
            continue;
        }
        let mut statement = String::new();
        let mut end = None;
        for (n, line) in lines[i..]
            .iter()
            .enumerate()
            .take(BARE_ESM_MAX_STATEMENT_LINES)
        {
            // A new column-0 opener STARTS a statement, so the one being accumulated cannot still be
            // running: stop and DISCARD it rather than letting it swallow the new one. This is the
            // whole bound on the residual documented above — without it, one trailing comment on a
            // page's FIRST import costs every edge on that page instead of just its own.
            if n > 0 && starts_bare_import(line) {
                break;
            }
            if n > 0 {
                statement.push('\n');
            }
            statement.push_str(line);
            if terminates_statement(statement.trim_end()) {
                end = Some(i + n);
                break;
            }
        }
        match end {
            Some(last) => {
                slab.push_str(&statement);
                slab.push('\n');
                i = last + 1;
            }
            // Prose, not a statement — drop the line and resume from the very next one, so a real
            // statement below it is still found.
            None => i += 1,
        }
    }
    if slab.is_empty() {
        return ImportMap::new();
    }
    parse_imports(rel, &slab)
}

/// True when `line` opens a bare-ESM statement: the keyword `import` at COLUMN 0, followed by a word
/// boundary. Unicode-aware, matching Rust's regex defaults — a line like `import<hangul>...` (the
/// corpus's Korean prose line) has no boundary after the keyword and never opens a statement, which is
/// the same verdict the terminator test would have reached one step later.
fn starts_bare_import(line: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^import\b").expect("valid regex"))
        .is_match(line)
}

/// True when the accumulated statement text ENDS in a module specifier — either `from "…"` (the clause
/// forms) or a bare `import "…"` (the side-effect form) — with an optional semicolon. `\s` spans
/// newlines, so a multi-line `import {\n A,\n B,\n} from "@site/x"` matches on its last line and not
/// before.
///
/// The quote pairing is spelled as two alternatives per form rather than as a capture plus a
/// backreference (`(['"])[^'"]+\1`): the `regex` crate has no backreferences at all. The two spellings
/// accept the same language — the character class already excludes BOTH quote characters, so the only
/// thing the backreference added was matching the opener to the closer, which enumerating the two pairs
/// does exactly. Re-measured after the translation, unchanged: 357 accepted / 0 rejected on the
/// corpus's `.mdx`, 0 accepted / 4 rejected on its `.md`.
fn terminates_statement(statement: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?:from\s*(?:'[^'"]+'|"[^'"]+")|^import\s*(?:'[^'"]+'|"[^'"]+"))\s*;?\s*$"#,
        )
        .expect("valid regex")
    })
    .is_match(statement)
}

/// The FRONTMATTER arm — `.astro`. An Astro component opens with a `---` fence whose contents are a
/// real TypeScript module; everything below the closing `---` is template. The slab between the two
/// fences goes straight to `parse_imports` — no slicer is needed, because prose cannot appear there.
///
/// Three rules, each of which is a loss channel if got wrong:
/// - The opening fence must be the FIRST line (after an optional BOM), trailing whitespace/`\r`
///   trimmed. Astro requires it at the very top, so leading blank lines are NOT skipped: scanning
///   forward for a `---` instead would find a markdown thematic break or a YAML document separator.
/// - It closes on the next line that is exactly `---`. No closer means NOTHING is emitted — the
///   disclosed loss direction, the same shape as `sfc_imports`'s
///   `an_unclosed_fence_blanks_the_rest_of_the_page`.
/// - **No markdown fence strip.** Same reason `extract_sfc_script_imports` gives for `.vue`/`.svelte`:
///   a `~~~` run at line start is ordinary template-literal text in TypeScript, and blanking from there
///   would eat both the real script below it and the literal's closing backtick.
///
/// What this arm deliberately does NOT read is an Astro CLIENT `<script>` in the template, and the
/// measurement that makes that the right scope rather than a shortcut is in [`PRESCAN_IMPORT_HOSTS`]'s
/// doc: across the 1539 `.astro` files of `corpus/frameworks/astro`, 1438 import statements sit inside
/// the leading `---` fence and 31 import lines sit below a closing one. The frontmatter is where
/// essentially every local-path edge lives; the template is the residual this arm names rather than
/// covers.
fn astro_frontmatter_imports(rel: &str, text: &str) -> ImportMap {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = body.split('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return ImportMap::new();
    }
    let mut slab = String::new();
    for line in lines {
        if line.trim_end() == "---" {
            return parse_imports(rel, &slab);
        }
        slab.push_str(line);
        slab.push('\n');
    }
    // Unclosed fence: emit nothing rather than hand the template to `parse_imports`.
    ImportMap::new()
}

#[cfg(test)]
mod tests;
