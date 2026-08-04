//! The Feature-Sliced Design half of the scores configuration — the layer-name vocabulary
//! ([`FsdConfig`], declared through `vocabulary.fsd.*`) and the precompiled matcher built from it
//! ([`FsdMatcher`]).
//!
//! Split out of `scores/config.rs` on 2026-07-27 purely to stay under the repo's per-file line cap, when
//! naming the four default lists as consts pushed that file past it. The seam is the natural one: the
//! parent keeps the numeric threshold knobs, this file keeps the FSD name vocabulary — which is also the
//! split the census now draws between `cap` and `convention`.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Feature-Sliced Design vocabulary — the per-repo directory conventions that drive `classify_path`/`module_of`.
/// A generic FSD repo needs no overrides; the derived `Default` impl's values apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsdConfig {
    /// L2 slice containers — first subdirectory is the slice (e.g. features/auth).
    pub slice_containers: Vec<String>,
    /// L1 entry layer prefixes.
    pub entry: Vec<String>,
    /// L3 shared layer prefixes.
    pub shared: Vec<String>,
    /// Foundation directory names — paths containing `/{dir}/` are classified as base modules (L4).
    pub base_dirs: Vec<String>,
}

/// FSD layer directory names this crate assumes when a project declares none — the values
/// `vocabulary.fsd.*` replaces, and the ONE place each list lives (`zzop_engine`'s
/// `VocabularyConfig::built_in` reads these symbols rather than re-spelling them).
///
/// Named on 2026-07-27, when they were found to be simultaneously (a) invisible to the policy-value
/// census, which reads `const` declarations and could not see a struct-field default, (b) inside a
/// directory the census excluded for a reason written about THRESHOLDS, and (c) unreachable from any
/// config. Three defences empty at one point. These are conventions in the plainest sense: a project
/// that spells its slice container `modules` rather than `features` is scored against a layout it never
/// adopted.
pub const DEFAULT_FSD_SLICE_CONTAINERS: &[&str] = &["features", "domains"];
/// FSD entry-layer directory names — see [`DEFAULT_FSD_SLICE_CONTAINERS`].
pub const DEFAULT_FSD_ENTRY: &[&str] = &["pages", "routes", "api"];
/// FSD shared-layer directory names — see [`DEFAULT_FSD_SLICE_CONTAINERS`].
pub const DEFAULT_FSD_SHARED: &[&str] = &[
    "core", "hooks", "render", "ui", "shared", "lib", "utils", "__test__",
];
/// FSD base-layer directory names — see [`DEFAULT_FSD_SLICE_CONTAINERS`].
pub const DEFAULT_FSD_BASE_DIRS: &[&str] = &["base"];

fn owned(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

impl Default for FsdConfig {
    fn default() -> Self {
        FsdConfig {
            slice_containers: owned(DEFAULT_FSD_SLICE_CONTAINERS),
            entry: owned(DEFAULT_FSD_ENTRY),
            shared: owned(DEFAULT_FSD_SHARED),
            base_dirs: owned(DEFAULT_FSD_BASE_DIRS),
        }
    }
}

/// Precompiled FSD regexes bundled with the config that produced them — an explicitly constructed value passed
/// at call sites rather than module-level mutable globals. Not `Serialize`/`Deserialize`: it holds compiled
/// `Regex` values, so it is treated as engine config, not analysis output (unlike `ScoreThresholds`).
#[derive(Debug, Clone)]
pub struct FsdMatcher {
    pub config: FsdConfig,
    pub entry_re: Regex,
    pub slice_re: Regex,
    pub shared_re: Regex,
    pub base_re: Regex,
}

impl FsdMatcher {
    /// Precompiles the four FSD regexes from `config`.
    ///
    /// An EMPTY layer list compiles to a regex that never matches, never to one built around an empty
    /// alternation. `["a","b"].join("|")` is fine, but `[].join("|")` is `""`, and interpolating that
    /// yields `^()/` — a pattern whose group matches emptiness and which therefore fires on shapes the
    /// author never named. Since 2026-07-27 an undeclared vocabulary means "this judgment is NOT made"
    /// (`zzop_engine::vocabulary`), so an empty list reaching here is the normal way to switch an FSD
    /// axis off, and turning it into an accidental matcher would make "I declared nothing" mean "match
    /// something" — the exact inversion the vocabulary contract forbids for empty guard patterns.
    pub fn new(config: FsdConfig) -> Self {
        /// A regex that cannot match any input: an empty character class is unsatisfiable.
        const NEVER: &str = "[^\\s\\S]";
        /// Every entry is a LITERAL directory name, so each is escaped before it joins the alternation.
        ///
        /// Without this the four `vocabulary.fsd.*` lists were raw regex source spliced into a pattern
        /// compiled with `.expect()`, and three defects fell out of that (all reproduced against this
        /// crate, 2026-08-04): a `.` in an entry was a WILDCARD, so `entry: ["ui.kit"]` classified an
        /// unrelated `uiXkit/` as the entry layer; a `|` SPLIT one declaration into two, so
        /// `entry: ["a|b"]` silently matched a directory the author never named; and an unbalanced
        /// bracket — `entry: ["foo("]` — **panicked the whole run**, on both hosts, from a legal config
        /// value that nothing validates on the way in (`zzop_facade::config::declared` hands
        /// `vocabulary.fsd.*` straight here).
        ///
        /// Escaping fixes all three at the source rather than adding a validator: there is no spelling
        /// of a directory name that this can reject, so no config becomes newly invalid, and the
        /// `.expect()`s below now hold for every possible entry (the residual is the `regex` crate's
        /// compiled-size limit, which needs a vocabulary far larger than a directory list can be).
        fn alt(xs: &[String]) -> String {
            xs.iter()
                .map(|x| regex::escape(x))
                .collect::<Vec<_>>()
                .join("|")
        }
        fn anchored(xs: &[String], shape: &str) -> String {
            if xs.is_empty() {
                NEVER.to_string()
            } else {
                shape.replace("{}", &alt(xs))
            }
        }
        let entry_re = Regex::new(&anchored(&config.entry, "^({})/")).expect("valid entry regex");
        let slice_re = Regex::new(&anchored(&config.slice_containers, "^({})/([^/]+)/"))
            .expect("valid slice regex");
        let shared_re =
            Regex::new(&anchored(&config.shared, "^({})/")).expect("valid shared regex");
        let base_re =
            Regex::new(&anchored(&config.base_dirs, "/({})/([^/]+)/")).expect("valid base regex");
        FsdMatcher {
            config,
            entry_re,
            slice_re,
            shared_re,
            base_re,
        }
    }
}

impl Default for FsdMatcher {
    fn default() -> Self {
        FsdMatcher::new(FsdConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{FsdConfig, FsdMatcher};
    use crate::scores::shared::classify_path;
    use crate::scores::ScoresConfig;

    /// A `ScoresConfig` whose FSD entry layer is exactly the given names.
    fn cfg_with_entry(entry: &[&str]) -> ScoresConfig {
        let mut cfg = ScoresConfig::default();
        cfg.fsd = FsdMatcher::new(FsdConfig {
            entry: entry.iter().map(|s| s.to_string()).collect(),
            ..cfg.fsd.config.clone()
        });
        cfg
    }

    /// RED before the escape: `.` was a regex wildcard, so a declared `ui.kit` claimed `uiXkit` too — a
    /// directory the author never named, silently scored as their entry layer.
    #[test]
    fn a_dot_in_a_declared_name_is_a_literal_not_a_wildcard() {
        let cfg = cfg_with_entry(&["ui.kit"]);
        assert_eq!(classify_path(&cfg, "ui.kit/x.ts").layer, 1);
        assert_ne!(
            classify_path(&cfg, "uiXkit/x.ts").layer,
            1,
            "`.` must not match an arbitrary character — the author declared one directory, not a pattern"
        );
    }

    /// RED before the escape: `|` split one declaration into two alternatives.
    #[test]
    fn a_pipe_in_a_declared_name_does_not_become_two_names() {
        let cfg = cfg_with_entry(&["a|b"]);
        assert_ne!(
            classify_path(&cfg, "b/x.ts").layer,
            1,
            "`a|b` is one directory name, not an alternation of two"
        );
    }

    /// RED before the escape: this PANICKED the process (`unclosed group`) from a legal config value,
    /// on both hosts, with no validator anywhere on the way in. A directory name is a literal; there is
    /// no spelling of one this constructor may refuse, and none it may crash on.
    #[test]
    fn a_regex_metacharacter_in_a_declared_name_does_not_panic() {
        for name in ["foo(", "a[", "x{2", "*", "\\"] {
            let cfg = cfg_with_entry(&[name]);
            // The point is that construction survived; classification of an unrelated path is incidental.
            let _ = classify_path(&cfg, "src/x.ts");
        }
    }
}
