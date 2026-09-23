//! Nuxt AUTO-IMPORT resolution: which files under an auto-import directory are reached by a BARE
//! SYMBOL NAME, with no import statement anywhere for the dep graph to read.
//!
//! ## The defect this answers
//! Nuxt registers every module under `composables/` and `utils/` (and whatever else `imports.dirs`
//! adds) into a compile-time auto-import table, so a component writes `useBase()` with no import line
//! at all. The static graph sees no importer and `dead-candidates` concludes the file is dead —
//! measured on nocodb 3a5cbd5, 207 of the tree's 296 findings sat in those directories and a blind
//! auditor judged the `nc-gui` half **81% false**. This module resolves the reference instead of
//! exempting the directory, which is the difference between silencing 207 findings and silencing 189:
//! measured end to end (whole tree, `--rule dead-candidates`, 296 -> 107, +0 added), **18** of those
//! files are reached by nothing at all, and every one of them must keep being named.
//!
//! ## Two consumers, two granularities
//! The same resolution answers a FILE question (`dead-candidates`: is anything in this file reached?)
//! and a SYMBOL question (`unimported-export`: is THIS export reached?). Only the first was wired, and
//! the second rule went on reading the import graph alone — so on the same tree it reported **644**
//! exports under this app's auto-import directories, 78% of nocodb's findings, and a blind auditor
//! sampling 14 of them found 13 referenced elsewhere in the app. Every one of those references was
//! already resolved here; nothing consumed the answer. Hence [`NuxtAutoImportRefs::names_by_target`]
//! beside `by_target` — same scan, same scope wall, same evidence, one more reader.
//!
//! ## Reference side: raw identifier tokens, not a parse
//! A resolving reference lives in one of three places, classified over all 207 subjects: a bare
//! identifier in a `.ts`/`.js` file, a bare identifier inside a `.vue` `<script>` block (93), and a
//! bare identifier that only ever appears in a `<template>` (4). Zero rest on an explicit `import`.
//! So the `.ts` side reads the `used_names` roster the fused pass already produces, and the `.vue` side
//! takes a WORD-BOUNDARY IDENTIFIER-TOKEN SCAN of the whole file text — cheaper than a `<script>` parse
//! (the pre-scan already has those bytes in hand) and strictly better, because it covers the
//! template-only 4 with no `<template>`/`<script>` splitter and no swc parse of a `.vue`. The extra
//! permissiveness — a name inside a comment or a string counts as a reference — wrongly silenced
//! **0** of the 18 on that tree.
//!
//! `.vue` is the ONLY pre-scan host that vouches ([`referrers::NUXT_TOKEN_ROSTER_EXTS`]), even though
//! `zzop_parser_typescript::PRESCAN_IMPORT_HOSTS` hands this pass `.svelte`/`.md`/`.mdx`/`.astro` too:
//! a Svelte or Astro component inside a Nuxt app is not a Nuxt module, and a Nuxt Content `.md`/`.mdx`
//! page compiles only its `<script setup>` block, so its PROSE is not a call site and a whole-text scan
//! cannot tell the two apart. Measured on nocodb's 7 `.md` files: they vouch for 0 of the 189 resolved
//! subjects, so the tree's number is the same with or without them.
//!
//! The two sides are deliberately NOT the same reader, and the difference is visible in the result:
//! `used_names` is a PARSED identifier set, so a name that appears only inside a `//` comment in a
//! `.ts` file does not count. That is what leaves `nc-gui/utils/Queue.ts` reported — its seven export
//! names have exactly one in-tree mention outside itself, a comment in `composables/useLTARStore.ts`.
//! A comment is not a reference, so it stays a finding, and it is the 18th dead file.
//!
//! ## The scope wall is the design, not the roster
//! Both sides are confined to ONE NUXT APP DIRECTORY (the directory holding `nuxt.config.*`), and to
//! the SAME one: [`referrers::owning_app_dir`] pairs every candidate AND every referrer with the
//! deepest app dir containing it, and [`bump_for_referrer`] bumps nothing across that pair. Nuxt builds
//! each app with its own auto-import table, so a name written in `apps/web` never resolves anything in
//! `apps/admin` — a union over the app dirs would seed the admin file into `unreachable`'s
//! `extra_entries` on the strength of a table it is never compiled against. (Nuxt LAYERS do share
//! tables through `extends`. That is not modelled: this module neither reads `extends` nor needs to,
//! and a shared layer's files simply keep whatever fan-in the dep graph already sees.)
//!
//! That wall is not tidiness: a whole-repo bare-name scan wrongly silences 2 of the 18 —
//! `nc-gui/utils/mimeTypeUtils.ts` dies to a DIFFERENT `mimeIcons` declared in `packages/nocodb/src`,
//! and `nc-gui/utils/workflowUtils.ts` to the token `transformNode` inside a minified
//! `vue.2.6.14.min.js` (which sits in `packages/nocodb/src/public/js/`, outside the app). Scoped to the
//! app dir, 0 of the 18 go quiet. The wall is also what keeps every non-Nuxt tree at exactly its old
//! numbers: with no `nuxt.config.*` in the tree there are no app dirs, no candidates and no rosters, so
//! this module allocates nothing and answers nothing.
//!
//! The app dir alone does not cover a vendored bundle the app ships ITSELF, so one directory is
//! excluded on the referrer side by name: `<app>/public/` is served VERBATIM, the auto-import transform
//! never runs over it, and no token in it can be a call site. Inside nocodb's own app dir that is
//! `nc-gui/public/js/swagger-ui-bundle.min.js` — 1.06 MB, 8898 distinct tokens, two of them
//! (`convert`, `deepClone`) export names of files this module resolves. Both of those have real in-app
//! referrers as well, so excluding the bundle moves nocodb by 0.
//!
//! ## Two sources for "which directory", and why they stay apart (§24)
//! `composables/` and `utils/` are auto-imported because the FRAMEWORK says so — no file in the tree
//! states it, so reading them is a convention, and they cover 170 of nocodb's 207 subjects on their
//! own. `store/`, `helpers/` and `lib/` are auto-imported only because that app's own `nuxt.config.*`
//! declares them, and silencing those without reading the declaration would be a guess dressed as a
//! convention. So the declaration IS read ([`zzop_parser_typescript::parse_nuxt_imports_dirs`]), and
//! the two halves stay visibly separate: [`NUXT_DEFAULT_AUTO_IMPORT_DIRS`] is the convention,
//! [`declared_dirs`] is the config, and a tree that declares nothing gets exactly the convention.
//!
//! ## Collisions, measured
//! The 342 files under nocodb's declared auto-import dirs export 1159 distinct names; 272 of those
//! (23%) are ALSO declared locally somewhere in the app, so a local declaration could in principle
//! vouch for an unrelated auto-import file. Only 1 of the resolved subjects rests solely on such a
//! shadowed name, and that one (`utils/shortcutUtils.ts`) is genuinely live. Real false liveness among
//! them: **0**. Scope shadowing is therefore not modelled — a token is a token.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use crate::dead_exports::DeadExportNames;

mod app_dir;
mod dirs;
mod referrers;

pub(super) use app_dir::{app_dirs, nuxt_app_dir};
use dirs::{auto_import_dirs, is_candidate, AutoImportDir};
use referrers::owning_app_dir;
pub(super) use referrers::{identifier_tokens, is_token_roster_host, referrer_app_dir};

/// Resolved auto-import references, keyed by the file that is REACHED, in the two granularities its two
/// consumers ask for.
///
/// `by_target` is how many DISTINCT other files in the same app dir mention one of that file's public
/// export names as a bare token — a fan-in count, which is all `super::dep_graph::fan_in` needs and all
/// `dead-candidates` (a FILE-level analysis) can use.
///
/// `names_by_target` is WHICH of that file's export names were the ones mentioned. `unimported-export`
/// is SYMBOL-level, so the count cannot answer its question: a file with one live auto-imported
/// composable and six dead ones has a fan-in of 1 and six exports that nothing writes. Collapsing the
/// two would force a choice between silencing the file whole (which kills the genuine orphans this rule
/// exists to find — nocodb's `utils/filterUtils.ts#snapshotFilter` is one, and it is the ONLY true
/// zero among 14 sampled by a blind auditor) and silencing nothing. Both maps have the same key set by
/// construction — a bump and a name are recorded together — and they are kept apart because the VALUES
/// answer different questions, not because either is optional.
#[derive(Debug, Default)]
pub(super) struct NuxtAutoImportRefs {
    pub(super) by_target: BTreeMap<String, u32>,
    pub(super) names_by_target: BTreeMap<String, BTreeSet<String>>,
}

/// Nuxt's own default auto-import directories — registered with no declaration anywhere, which is what
/// makes reading them a framework CONVENTION rather than a config the tree has to state (§24).
/// Directories that are auto-imported only because `nuxt.config.*` says so (nocodb's `store`,
/// `helpers`, `lib`) are deliberately NOT here: they arrive through [`declared_dirs`], which reads that
/// declaration, because silencing them without it would be a guess dressed as a convention. Measured
/// split on nocodb: these two cover 170 of the 207 subjects, and the declared line buys the other 37.
const NUXT_DEFAULT_AUTO_IMPORT_DIRS: &[&str] = &["composables", "utils"];

/// Extensions Nuxt's auto-import scanner reads under those directories.
const AUTO_IMPORT_EXTS: &[&str] = &["ts", "js", "mjs", "mts"];

/// Resolves every auto-import reference in the tree. `ts_used_names` is the fused pass's per-file
/// identifier roster (`unimported-export`'s own substrate, reused rather than re-read);
/// `prescan_token_rosters` is the whole-text token roster the import pre-scan bundled out of its own
/// disk read, already confined to the app dirs. Returns an empty result — reading nothing off disk —
/// when the tree has no `nuxt.config.*`.
pub(super) fn scan(
    root: &Path,
    ts_paths: &HashSet<String>,
    app_dirs: &[String],
    ts_used_names: &HashMap<String, DeadExportNames>,
    prescan_token_rosters: &[(String, BTreeSet<String>)],
) -> NuxtAutoImportRefs {
    if app_dirs.is_empty() {
        return NuxtAutoImportRefs::default();
    }
    // Per app: the framework's two default directories plus whatever that app's own `nuxt.config.*`
    // declares. One config read per app, and none at all on a tree with no Nuxt app in it.
    let per_app: Vec<(&String, Vec<AutoImportDir>)> = app_dirs
        .iter()
        .map(|d| (d, auto_import_dirs(root, ts_paths, d)))
        .collect();
    // Candidate files, each PAIRED WITH ITS OWN APP, sorted: the read order below and the `by_name`
    // fan-out both ride on it, and the pairing is what keeps one app's referrer from vouching for
    // another app's file (each app is built with its own auto-import table).
    let mut candidates: Vec<(&String, &str)> = ts_paths
        .iter()
        .filter_map(|p| {
            let app = owning_app_dir(p, app_dirs)?;
            let (_, dirs) = per_app.iter().find(|(a, _)| a.as_str() == app)?;
            is_candidate(p, app, dirs).then_some((p, app))
        })
        .collect();
    candidates.sort();
    if candidates.is_empty() {
        return NuxtAutoImportRefs::default();
    }

    // Public export name -> the candidates publishing it. One parse per candidate, off disk: the
    // fused pass's `SourceSymbol::exported` cannot answer this — a pure barrel like
    // `lib/formBuilder.ts` declares nothing and republishes four imported names, and `zzop file`
    // reports `symbols.count = 0` for it.
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, (rel, _)) in candidates.iter().enumerate() {
        // `read_for_parse`, not `fs::read`: these candidates come from `ts_paths`, and
        // `pipeline::fresh::ts_slot` puts a file in there whether or not the gate refused it — so a
        // refused file reached `parse_exported_names` below and took the process down (ledger V127).
        let Some(text) = crate::analyze::read_for_parse(root, rel.as_str()) else {
            continue; // unreadable, or past a recursion cap — no names, so no liveness claimed
        };
        for name in zzop_parser_typescript::parse_exported_names(rel, &text) {
            if name == "default" {
                continue; // not a bare identifier any auto-import call site can write
            }
            by_name.entry(name).or_default().push(idx);
        }
    }

    // One pass per referencing file, with a per-file `seen` set so two names of the same target count
    // once — the same dedup convention `merge_prescan_fan_in`/`merge_asset_ref_fan_in` use.
    // (Sorted, though every bump is a commutative `+= 1`: hash order would still be a hash order in a
    // diff, and this pass is cheap next to the reads above.)
    let mut hits: Vec<u32> = vec![0; candidates.len()];
    let mut names_hit: Vec<BTreeSet<String>> = vec![BTreeSet::new(); candidates.len()];
    let mut referrers: Vec<(&String, &str)> = ts_used_names
        .keys()
        .filter_map(|r| referrer_app_dir(r, app_dirs).map(|app| (r, app)))
        .collect();
    referrers.sort();
    for (rel, app) in referrers {
        let names = &ts_used_names[rel].used;
        bump_for_referrer(
            rel,
            app,
            names.iter(),
            &by_name,
            &candidates,
            &mut hits,
            &mut names_hit,
        );
    }
    for (rel, tokens) in prescan_token_rosters {
        if let Some(app) = referrer_app_dir(rel, app_dirs) {
            bump_for_referrer(
                rel,
                app,
                tokens.iter(),
                &by_name,
                &candidates,
                &mut hits,
                &mut names_hit,
            );
        }
    }

    let mut refs = NuxtAutoImportRefs::default();
    for (((rel, _), n), names) in candidates.iter().zip(hits).zip(names_hit) {
        if n > 0 {
            refs.by_target.insert((*rel).clone(), n);
        }
        if !names.is_empty() {
            refs.names_by_target.insert((*rel).clone(), names);
        }
    }
    refs
}

/// Counts one referencing file's contribution: at most +1 per target, however many of that target's
/// names it mentions — while recording EVERY name it mentioned, because the two consumers count
/// different things (see [`NuxtAutoImportRefs`]). The per-file `seen` set therefore gates the fan-in
/// bump only: a referrer writing three of a target's names is one file for `by_target` and three names
/// for `names_by_target`, and dropping the second and third names would leave two live exports
/// reported dead by a rule that reads names.
///
/// Two clauses gate every record. A target in a DIFFERENT app is never reached —
/// `referrer_app` is the app that compiles the referrer, and a bare name resolves only against that
/// app's own table. And a candidate never vouches for ITSELF — `used_names` drops declaration names
/// but keeps the identifier in `export default useFoo`, and a `.vue`'s raw token scan keeps
/// everything, so without the self-check a candidate could be its own only referrer.
#[allow(clippy::too_many_arguments)]
fn bump_for_referrer<'a>(
    referrer: &str,
    referrer_app: &str,
    tokens: impl Iterator<Item = &'a String>,
    by_name: &HashMap<String, Vec<usize>>,
    candidates: &[(&String, &str)],
    hits: &mut [u32],
    names_hit: &mut [BTreeSet<String>],
) {
    let mut seen: HashSet<usize> = HashSet::new();
    for token in tokens {
        let Some(targets) = by_name.get(token.as_str()) else {
            continue;
        };
        for &idx in targets {
            let (target, target_app) = candidates[idx];
            if target_app != referrer_app || target.as_str() == referrer {
                continue;
            }
            names_hit[idx].insert(token.clone());
            if seen.insert(idx) {
                hits[idx] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests;
