//! The compiled-regex memo a loaded pack carries — see [`RegexCache`].

use std::collections::HashMap;
use std::sync::Mutex;

/// A PACK-SCOPED memo of compiled regexes, keyed by the pattern source.
///
/// ## Why it exists
///
/// The evaluator compiles a rule's regexes on every call, and `eval_pack_into` runs **once per file**.
/// So a 2,000-file tree recompiles every loaded rule's patterns 2,000 times — and it is worse than one
/// compile per pattern per file, because four independent places compile the SAME string:
/// [`super::diagnostics::RuleDiag::compile`] (the one that keeps the result), `super::prefilter` (twice,
/// discarding the result — it only asks `is_err()`), and `super::markers` (the derived suppress marker,
/// rebuilt per call).
///
/// Measured 2026-08-08 on `cargo test -p zzop-rule-packs --test pack_security` (305 tests, warm):
/// **34.2s → 17.5s** with all four sites routed through one memo. An earlier experiment that memoized
/// only `RuleDiag::compile` moved it by a quarter to a third and concluded regex compilation was "real
/// but not the dominant term" — that conclusion was an artifact of memoizing one of four sites. It is
/// about half.
///
/// ## Why PACK-scoped and not process-global
///
/// A process-global memo is unbounded and outlives every run; `zzop-mcp` is a long-lived process, so it
/// would hold every pattern any request ever loaded, forever. A pack is loaded per run and dropped with
/// it, which puts this memo on the same lifetime as the thing it describes — the same reasoning that
/// made the git-collection memo run-scoped rather than global.
///
/// ## Why a miss is cached too
///
/// `None` (did not compile) is stored like any hit, so the prefilter/marker probes that only ask
/// "does it compile" never re-attempt a known-bad pattern. One caller deliberately still does:
/// `RuleDiag::compile` re-runs the failing compile per file to get the ERROR TEXT (see
/// [`RegexCache::compile_err`]) — what is deduplicated there is the diagnostic line (`RuleDiag::push`),
/// not the compile attempt itself.
///
/// ## What else lives here
///
/// The line-scan pre-filter's `RegexSet` too — see [`Inner::prefilter`]. Same owner because it is the
/// same fact: compiled regex state derived from this pack's patterns, rebuilt per file until it wasn't.
#[derive(Default, Clone)]
pub struct RegexCache {
    inner: std::sync::Arc<Inner>,
}

/// Behind an `Arc` so a CLONE of the pack SHARES all of it. Cloning a pack is common (the engine hands
/// one to each tree, and every rule-pack test builds a fresh `EngineConfig`), and a clone that started
/// empty would throw the compiled state away exactly when it is most reusable. Sharing is safe for
/// `entries` because it is pattern-KEYED and every entry is a pure function of the pack's own pattern
/// text — compiling `\.ts$` cannot mean two things. The `prefilter` below is NOT covered by that
/// argument: its `pattern_rule` is POSITIONAL (indices into a specific rules vec), so a clone whose
/// `rules` was mutated after cloning must NOT keep sharing it — the two seams that mutate
/// (`gate_pack_rules`, `envelope_rule_pack`) swap in [`RegexCache::fork_for_mutated_rules`], which
/// keeps the pattern memo and resets the prefilter. See
/// `prefilter::LineScanPrefilter::pattern_rule`'s doc, the invariant's owner.
#[derive(Default)]
struct Inner {
    entries: Mutex<HashMap<String, Option<regex::Regex>>>,
    /// The line-scan `RegexSet` pre-filter, built at most once per pack.
    ///
    /// It belongs here for the same reason the map does, and it was found the same way: `eval_pack_into`
    /// runs once per FILE and rebuilt the whole set every time. A `RegexSet` compiles every line-scan
    /// pattern into ONE automaton, so that rebuild is the most rule-count-proportional work in the pass
    /// — measured 2026-08-08 on `pack_security`, memoizing it alone took **17.8s → 14.7s**.
    ///
    /// `Option` inside, because "this pack has no usable line-scan pattern" is a real answer and must be
    /// cached like any other; `OnceLock` rather than the `Mutex` map because there is exactly one of
    /// these per pack and no key to look it up by.
    prefilter: std::sync::OnceLock<Option<std::sync::Arc<super::prefilter::LineScanPrefilter>>>,
}

impl RegexCache {
    /// The compiled regex for `pattern`, compiling at most once per distinct pattern for this cache's
    /// lifetime. A poisoned lock degrades to an uncached compile rather than panicking — a memo must
    /// never be able to fail an analysis.
    pub fn compile(&self, pattern: &str) -> Option<regex::Regex> {
        let Ok(mut entries) = self.inner.entries.lock() else {
            return regex::Regex::new(pattern).ok();
        };
        if let Some(hit) = entries.get(pattern) {
            return hit.clone();
        }
        let fresh = regex::Regex::new(pattern).ok();
        entries.insert(pattern.to_string(), fresh.clone());
        fresh
    }

    /// The compile ERROR for `pattern`, for the one caller that reports it (`RuleDiag::compile`).
    /// Deliberately not cached, which means the failing compile RE-RUNS once per file that reaches
    /// the broken rule: [`RegexCache::compile`] stores the miss as `None` and carries no error, so
    /// each file's `RuleDiag::compile` call re-derives it here. Only the resulting diagnostic LINE
    /// is emitted once per run (`RuleDiag::push` de-duplicates the message), not this derivation.
    /// That price is confined to a rule that is already broken — a path `validate-rule-pack` exists
    /// to make rare — and caching a `regex::Error` would mean holding a second copy of every failure
    /// keyed the same way for exactly this one reader.
    pub fn compile_err(pattern: &str) -> Option<regex::Error> {
        regex::Regex::new(pattern).err()
    }

    /// The cache a MUTATED-rules clone of the owning pack must carry (`gate_pack_rules`,
    /// `envelope_rule_pack` — the two seams that `retain` on `rules` after cloning). The
    /// pattern-keyed `entries` are COPIED (cheap: `regex::Regex` is internally reference-counted, so
    /// this clones handles, not automata) because a compiled pattern stays valid under any rules
    /// shape; the positional `prefilter` deliberately starts EMPTY so the mutated pack builds its
    /// own against its own rules vec. Without this fork the mutated clone shared the `Arc`'d
    /// prefilter with its differently-shaped original, and whichever shape evaluated first poisoned
    /// the other's rule-index mapping — `LineScanPrefilter::pattern_rule`'s doc owns that invariant,
    /// and its `debug_assert` caught exactly this via the public `analyze_tree` API (one loaded pack
    /// cloned into two configs, the second with a rule disabled).
    pub fn fork_for_mutated_rules(&self) -> RegexCache {
        let fresh = RegexCache::default();
        if let (Ok(src), Ok(mut dst)) = (self.inner.entries.lock(), fresh.inner.entries.lock()) {
            dst.extend(src.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        fresh
    }

    /// The pack's line-scan pre-filter, built by `f` at most once for this cache's lifetime.
    ///
    /// The builder is passed in rather than called here because building it needs the whole
    /// `RulePackDef`, and a cache that reached back into its owner would be a cycle. The caller that
    /// has the pack supplies the closure; this type only owns WHEN it runs.
    pub(super) fn prefilter_or_init(
        &self,
        f: impl FnOnce() -> Option<super::prefilter::LineScanPrefilter>,
    ) -> Option<std::sync::Arc<super::prefilter::LineScanPrefilter>> {
        self.inner
            .prefilter
            .get_or_init(|| f().map(std::sync::Arc::new))
            .clone()
    }
}

/// Prints a CONSTANT — no count, no contents.
///
/// This is not cosmetic. `zzop_engine::cache::ruleset_fingerprint` hashes `format!("{pack:?}")` to decide
/// whether a cached finding is still valid, so anything a pack's `Debug` reveals about MUTABLE state
/// becomes part of the cache key. A count would change as patterns compile, so the fingerprint would
/// differ between two runs over an unchanged tree and every warm hit would turn into a miss — measured,
/// not hypothesized: the first version printed `RegexCache(N compiled)` and `analyze_cache`'s three
/// warm-rerun tests went red immediately.
///
/// The memo is derived state, never pack identity, so contributing nothing to the fingerprint is also
/// the semantically correct answer — the constant is not a workaround for the hashing, it is what a
/// memo should say about itself.
impl std::fmt::Debug for RegexCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RegexCache")
    }
}
