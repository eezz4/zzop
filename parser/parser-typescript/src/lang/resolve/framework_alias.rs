//! Framework-RESERVED import aliases — the ones a framework defines for you and then writes into a
//! GENERATED config that a fresh checkout does not contain. They are collected here rather than left
//! beside the ordinary `@/` handling because they share one cause and one failure mode, and that shared
//! cause is the whole reason this module exists:
//!
//! - SvelteKit reserves `$lib` for `src/lib` and wires it through `.svelte-kit/tsconfig.json`, created
//!   by `svelte-kit sync` at BUILD time.
//! - Nuxt reserves `~`/`@` for its `srcDir` and `~~`/`@@` for its `rootDir`, wired through
//!   `.nuxt/tsconfig.json`, created by `nuxt prepare` at BUILD time.
//!
//! In both cases the mapping is REAL and non-configurable in normal use, and in both cases it is absent
//! from the repository — so the resolver meets a specifier it cannot place, every import through the
//! alias fails, and `dead-candidates`/`unimported-export` report the aliased tree as orphaned.
//!
//! Measured on nocodb (2026-08-21): `packages/nc-gui/tsconfig.json` is `{"extends":
//! "./.nuxt/tsconfig.json"}` and `.nuxt/` does not exist in a clone, while `nc-gui` carries 220 `~/`
//! import specifiers across 145 files — none of which could resolve. That tree's own config cannot be
//! read for the answer either: its `alias:` block is a Vite POLYFILL block (`querystring`, `util`,
//! `url`) and never mentions `~`.
//!
//! BOTH lookups are ANCHORED, not rooted: the alias means a path inside the app the IMPORTING file
//! belongs to, so `anchor_dir` walks up from `from_file` to the nearest ancestor directory holding that
//! framework's config. For nocodb that is `packages/nc-gui/`, not the analysis root, and resolving
//! against the root instead would silently mint edges to same-named files in a SIBLING app.
//!
//! `$lib` was rooted at `src/lib` when it was moved here and stayed that way for a day — the doc above
//! claimed anchoring while half the module did not do it. Measured on immich (2026-08-21), whose web app
//! lives in `web/`: `$lib/actions/shortcut` resolved against a top-level `src/lib` that does not exist,
//! so none of the 2,401 `$lib/` specifiers landed, `web/src` ended with 44 resolved in-edges across 286
//! keys (~2%), and `dead-candidates` called 235 of 236 live files dead. The lesson is the one that made
//! this module exist: a new mechanism has to be applied to the SIBLINGS already in the file, not only to
//! the case that motivated it.
//!
//! Deliberately NOT read: a `srcDir` set explicitly in `nuxt.config.*`. That means parsing a TypeScript
//! config file for one string, while the default (`srcDir` = the config's own directory) is what the
//! overwhelming majority of apps use. An app that moves its srcDir resolves nothing here and is no
//! worse off than before this module existed — the same under-approximation, never a wrong edge.

use std::collections::HashSet;

use super::specifier::try_ext;

/// Files whose presence marks a directory as a Nuxt app root (and therefore, by default, its srcDir).
///
/// THE ONE OWNER of that anchor, `pub` for that reason alone. `zzop_engine`'s Nuxt auto-import
/// assembly (`analyze/assemble/nuxt_auto_import/app_dir.rs`) and the `plugins/`+`middleware/`
/// convention beside it need the SAME four filenames — the app root is what the alias, the
/// conventions and that table are all relative to — and until 2026-08-31 they carried a
/// byte-identical hand copy across the crate boundary with nothing comparing the two. That was the
/// T2 shape (assert the relationship in a pin) sitting where T1 was available all along: the engine
/// already depends on this crate, so the copy is gone and the engine `use`s this slice.
pub const NUXT_CONFIG_FILES: &[&str] = &[
    "nuxt.config.ts",
    "nuxt.config.js",
    "nuxt.config.mjs",
    "nuxt.config.mts",
];

/// Files whose presence marks a directory as a SvelteKit app root (and therefore the parent of its
/// `src/lib`). A SvelteKit project cannot build without one — `svelte-kit sync` reads it to write the
/// generated tsconfig this module exists to stand in for — so requiring it costs nothing a real Kit app
/// has, and refusing to resolve without it is what keeps `$lib` from reaching a sibling app's file.
const SVELTE_CONFIG_FILES: &[&str] = &[
    "svelte.config.js",
    "svelte.config.ts",
    "svelte.config.mjs",
    "svelte.config.cjs",
];

/// Nuxt alias prefixes, longest first — `~~/` must be tested before `~/` or it would be read as the
/// srcDir alias applied to a path beginning `~/`.
const NUXT_PREFIXES: &[&str] = &["~~/", "@@/", "~/", "@/"];

/// Is this specifier one this module claims? The workspace resolver has to know BEFORE it falls through
/// to package matching, and the first version of this module was reached only through `@/` because that
/// caller carried its own hand-written list of alias spellings — so a `~/` import was classified as an
/// external package and every Nuxt fix here was dead code. Measured: nocodb's `dead-candidates` did not
/// move (346 before, 346 after) until this predicate existed. One owner for the list, asked rather than
/// re-spelled, is what stops that from recurring the next time a spelling is added.
pub(super) fn is_framework_alias(specifier: &str) -> bool {
    specifier == "$lib"
        || specifier.starts_with("$lib/")
        || NUXT_PREFIXES.iter().any(|p| specifier.starts_with(p))
}

/// Resolves a framework-reserved alias, or `None` when the specifier is not one (or names no file this
/// run knows). Never guesses: a Nuxt alias in a tree with no `nuxt.config.*` above the importing file
/// returns `None` rather than falling back to the analysis root.
pub(super) fn resolve_framework_alias(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<String>,
) -> Option<String> {
    if specifier == "$lib" || specifier.starts_with("$lib/") {
        let dir = anchor_dir(from_file, all_paths, SVELTE_CONFIG_FILES)?;
        let lib = join_dir(&dir, "src/lib");
        return match specifier.strip_prefix("$lib/") {
            Some(rest) => try_ext(&format!("{lib}/{rest}"), all_paths),
            None => try_ext(&lib, all_paths),
        };
    }
    // `~~`/`@@` name the ROOT dir and `~`/`@` the SRC dir. With no explicit `srcDir` — the default, and
    // the only case this module claims — the two are the SAME directory, so all four resolve alike here.
    // They are listed separately anyway because the distinction is real in Nuxt and a future `srcDir`
    // reader has to tell them apart; collapsing them now would hide that the difference was ever there.
    let rest = NUXT_PREFIXES
        .iter()
        .find_map(|p| specifier.strip_prefix(p))?;
    let dir = anchor_dir(from_file, all_paths, NUXT_CONFIG_FILES)?;
    try_ext(&join_dir(&dir, rest), all_paths)
}

/// Nearest ancestor directory of `from_file` holding one of `config_files` — the app that file belongs
/// to. `""` is the analysis root and is a legitimate answer (a single-app repo with the config at top
/// level), which is why the walk cannot use `Option` emptiness as its terminator.
///
/// Both frameworks share this walk because both aliases mean the same thing: a path relative to the app
/// the IMPORTING file belongs to, not to the analysis root. `$lib` had a hand-rolled root-relative
/// `src/lib` until 2026-08-21 — measured on immich, whose web app lives in `web/`, that resolved
/// `$lib/...` against a top-level `src/lib` that does not exist and matched nothing: 2,401 `$lib/`
/// specifiers, a `web/src` dep graph with 44 resolved in-edges across 286 keys, and `dead-candidates`
/// reporting 235 false positives out of 236. One walk for both is also what stops the next alias from
/// being added on the rooted side of that split.
fn anchor_dir(
    from_file: &str,
    all_paths: &HashSet<String>,
    config_files: &[&str],
) -> Option<String> {
    let mut dir = match from_file.rfind('/') {
        Some(i) => from_file[..i].to_string(),
        None => String::new(),
    };
    loop {
        if config_files
            .iter()
            .any(|f| all_paths.contains(&join_dir(&dir, f)))
        {
            return Some(dir);
        }
        if dir.is_empty() {
            return None;
        }
        match dir.rfind('/') {
            Some(i) => dir.truncate(i),
            None => dir.clear(),
        }
    }
}

fn join_dir(dir: &str, rest: &str) -> String {
    if dir.is_empty() {
        rest.to_string()
    } else {
        format!("{dir}/{rest}")
    }
}

#[cfg(test)]
mod tests;
