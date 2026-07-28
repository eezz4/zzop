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
        fn alt(xs: &[String]) -> String {
            xs.join("|")
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
