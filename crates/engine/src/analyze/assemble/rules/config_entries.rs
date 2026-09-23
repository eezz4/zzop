//! Entry files a TOOL CONFIG declares — the substrate `dead-candidates` unions into its `extra_entries`,
//! and nothing else in this engine consumes.
//!
//! ## The gap this closes
//! `dead-candidates` already EXEMPTS a config file itself (`zzop_rules_graph::is_tool_config_file`, via
//! `is_tool_entry_file`): a `vite.config.ts` is loaded by its tool, so zero importers on it proves
//! nothing. What it did not do is READ the file it had just recognized. A bundler config is the one
//! place in a tree that states which files the build actually starts from, so throwing that text away
//! left every one of those entry files looking like an orphan. Measured on koel 3f5213d4: 6 of the 7
//! `dead-candidates` findings were wrong and 3 of them were entry files named, in plain text, inside two
//! config files zzop had already opened its exemption for — `laravel({ input: [...] })` naming the two
//! application entries, and a SECOND config (`vite.config.sw.js`, reached only through the tool's
//! `--config` flag) naming the service worker through `build.lib.entry`.
//!
//! ## Why a lexical harvest rather than a key path
//! The obvious shape is to read the KEYS a bundler declares entries under — `build.lib.entry`,
//! `build.rollupOptions.input`, the `input` option of a framework plugin. That is a hand-written list of
//! proper nouns, and it is worse than it looks: those three spellings are an object key, a nested object
//! key, and a POSITIONAL ARGUMENT to a plugin call, and two of the three wrap the value in
//! `resolve(__dirname, …)`, so reading them means evaluating a TypeScript module rather than indexing
//! one. The property all of them agree on instead is small and checkable: **the entry is a quoted string
//! literal, carrying a source extension, that resolves to a file this tree actually has.** So that is
//! what is matched, and the sixth bundler's spelling is covered without a row being added for it.
//!
//! Requiring the EXTENSION is what keeps the harvest from becoming a blanket exemption, and it is not a
//! stylistic choice: without it, `vite.config.ts`'s own alias table (`'@': resolve(__dirname,
//! './resources/assets/js')`) would name the application's whole source directory and excuse every file
//! under it. With it, a directory string resolves to nothing.
//!
//! ## Why this needs no `unreachable` seed, unlike the other two "loaded by a mechanism the graph
//! cannot see" passes
//! `merge_prescan_fan_in` and `merge_asset_ref_fan_in` both have to hand their targets to `unreachable` as
//! entries, because they BUMP `fan_in`: a file they touch stops being a `dead-candidates` subject
//! (`fan_in == 0`) and becomes an `unreachable` subject (`fan_in > 0`) in the same move, so without the
//! seed it would flip from one false finding to the other. This pass touches no counter at all — it
//! only adds paths to one rule's exemption set — so a file it excuses keeps `fan_in == 0` and stays an
//! IMPLICIT entry inside `find_unreachable`, which treats every zero-fan-in file as one. The flip is
//! structurally impossible here rather than merely unobserved.
//!
//! ## What it costs
//! Not reading the key is what buys the sixth bundler's spelling for free, and the same choice is
//! what makes the harvest POLARITY-BLIND. Both halves of that trade are named here, and both are
//! pinned in `tests.rs` — a cost nothing asserts is a cost the next edit can change by accident:
//!
//! - **A path in an EXCLUSION list is exempted exactly like an entry.** `coverage.exclude`,
//!   eslint's `ignores`, knip's `ignore` — a config saying "this tool skips this file" reads
//!   identically to one saying "this build starts here". This is not hypothetical: across the dogfood
//!   corpus the harvest resolves **15 such paths, in 5 config files, across 3 trees**, most plainly at
//!   `corpus/frameworks/nest/vitest.config.coverage.mts`, whose `coverage.exclude` array spells out
//!   eight concrete `packages/**/*.ts` paths, all eight of which exist on disk.
//!
//!   None of the 15 is a live suppression today, and the REASON matters more than the count, because
//!   the obvious reason is wrong: it is not that they all have importers. `corpus/frameworks/grafana/
//!   packages/mapbox-jsonlint-lines-primitives/lib/jsonlint.js` has ZERO in-repo importers — it is
//!   exempt through its package's `"main"` field, i.e. the `package.json` entry scan, a different
//!   exemption entirely. `packages/get-document/index.js` is carried by `entry_patterns`, and nest's
//!   `socket-module.ts` is reached only by a package-specifier dynamic import. So the honest statement
//!   is **"every one is already exempt or already imported"**, and it was confirmed by counterfactual
//!   rather than inferred: deleting `vitest.config.coverage.mts` from a copy of that tree and
//!   re-running leaves the `dead-candidates` count unchanged at 54, with none of the eight appearing
//!   in either run.
//!
//!   The channel is open all the same, and it runs in the ERASING direction (a file added to
//!   `coverage.exclude` and later orphaned is never reported again, with no trace). Worth stating
//!   plainly, because a disclosed cost and a closed one are not the same thing: this exemption rests
//!   on an INFERENCE — "a config file names this path, therefore a tool loads it" — and not on
//!   anything the user declared. `coverage.exclude` declares that coverage skips a file; it never
//!   declares that the file is an entry, and it never declares that a finding should be dropped. Under
//!   `rule-quality.md` §24 that is erasure standing on inference, which the disclosure below makes
//!   visible to a maintainer reading this file and does NOT make visible to the user, since a deleted
//!   finding leaves nothing behind. Documented, not neutralized.
//! - **An entry written WITHOUT its extension is not exempted at all.** `input: 'src/app'` is legal
//!   rollup/vite and resolves fine for the bundler; here the extension requirement above rejects it
//!   before `try_ext` ever runs. Loosening this is not free — the alias table in the paragraph above
//!   is the shape that would come through with it — which is why the miss is disclosed rather than
//!   patched.
//!
//! What bounds the whole pass is "a config file spells this exact path", and nothing else — it cannot
//! reach a file no config mentions, which is the property the true-positive pin asserts in the same
//! call as the false-positive one. The two costs above are the reason the user-facing message and the
//! catalog row describe the PREDICATE (a quoted path carrying a source extension) instead of the
//! intent (a bundler's entry list): a message that names the intent asserts a relation this matcher
//! does not hold, which is `rule-quality.md` §25's first failure shape.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use zzop_core::posix_path::join_and_normalize;

/// Every tree-relative source file a tool config in this tree names as a path. Resolution is relative
/// to each config's OWN directory (a monorepo's `packages/web/vite.config.ts` starts from
/// `packages/web/`), through the same `try_ext` the `package.json` manifest scan resolves its entry
/// fields with — so a config naming a compiled `./src/main.js` still lands on the `src/main.ts` that
/// produced it. A config that cannot be read contributes nothing rather than failing the run.
///
/// `ts_paths` is BOTH the population and the resolution universe on purpose: the files searched for
/// configs are the files this run dispatched to the TypeScript frontend, which is exactly the set a
/// resolved entry can land in. There is no second list to keep in step with the first.
pub(super) fn scan(root: &Path, ts_paths: &HashSet<String>) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    for rel in ts_paths
        .iter()
        .filter(|p| zzop_rules_graph::is_tool_config_file(p))
    {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let dir = match rel.rfind('/') {
            Some(i) => &rel[..i],
            None => "",
        };
        for cap in quoted_source_path().captures_iter(&text) {
            let token = &cap[1];
            if let Some(resolved) =
                zzop_parser_typescript::try_ext(&join_and_normalize(dir, token), ts_paths)
            {
                out.insert(resolved);
            }
        }
    }
    out
}

/// A quoted string literal whose content ends in a source extension — the one shape every bundler's
/// entry declaration agrees on (see the module doc). All three JS quote characters are accepted, and a
/// literal may not span a newline or contain another quote character, which is what keeps a template
/// literal's `${…}` interpolation from being read as a path (it resolves to nothing anyway; refusing it
/// here just costs less). The extension set is the TS-dispatch set `dead-candidates` uses for
/// eligibility, so the harvest cannot resolve to a file that rule would never have judged — and it is
/// case-INSENSITIVE for the same reason: `dead_candidates::is_ts_dispatch_extension` is `(?i)`, so a
/// case-sensitive harvest would have left `'./src/Main.TS'` eligible for the rule and invisible to the
/// exemption. (The two are hand copies across a crate boundary that is deliberate — `rules-graph` is
/// `zzop-core`-only — so this comment is the join; there is no shared const to point at.)
fn quoted_source_path() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"['"`]([^'"`\r\n]+\.(?i:ts|tsx|js|jsx|mjs|cjs|mts|cts))['"`]"#).unwrap()
    })
}

#[cfg(test)]
mod tests;
