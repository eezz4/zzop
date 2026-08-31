//! WHICH directories are Nuxt app roots — the anchor every other half of this module hangs off, in its
//! own file because the module head sits at the repo's 300-line ceiling.

use std::collections::{BTreeSet, HashSet};

/// Files marking a directory as a Nuxt app root — the same anchor
/// `super::rules::framework_entries` uses for the `plugins/`+`middleware/` convention, and the same one
/// `zzop_parser_typescript`'s framework-alias resolver uses, for the same reason: the app root is what
/// the alias, the conventions and this table are all relative to.
///
/// READ from that resolver, never re-spelled. It was a byte-identical hand copy until 2026-08-31, and
/// the doc above ASSERTED the equality while nothing enforced it — the definition of a T2 duplicate
/// standing where T1 was available, since this crate already depends on `zzop-parser-typescript`. A
/// fifth `nuxt.config.*` spelling added on one side would otherwise have left the alias resolving
/// against an app root this assembly does not see, or the reverse.
pub(in crate::analyze::assemble) use zzop_parser_typescript::NUXT_CONFIG_FILES;

/// `Some(app dir)` when this path IS a Nuxt config file. `""` for a config at the analysis root.
pub(in crate::analyze::assemble) fn nuxt_app_dir(rel: &str) -> Option<&str> {
    let (dir, base) = match rel.rfind('/') {
        Some(i) => (&rel[..i], &rel[i + 1..]),
        None => ("", rel),
    };
    NUXT_CONFIG_FILES.contains(&base).then_some(dir)
}

/// Every Nuxt app directory in the tree, deduplicated and sorted (determinism: this set drives which
/// files get read below). Empty on every tree with no `nuxt.config.*` — the whole mechanism's off
/// switch, and the reason a non-Nuxt corpus tree cannot move.
pub(in crate::analyze::assemble) fn app_dirs(ts_paths: &HashSet<String>) -> Vec<String> {
    let mut dirs: Vec<String> = ts_paths
        .iter()
        .filter_map(|p| nuxt_app_dir(p))
        .map(str::to_string)
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    dirs.dedup();
    dirs
}
