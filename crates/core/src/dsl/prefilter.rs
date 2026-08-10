//! `RegexSet` multi-pattern pre-filter for `Matcher::LineScan` — a pure optimization; findings must be
//! observationally identical with it on or off.

use super::def::{Matcher, RulePackDef};
use super::source::SourceFile;

/// Multi-pattern pre-filter for `Matcher::LineScan` (pure optimization). One `regex::RegexSet` is built
/// from every `LineScan` rule's patterns, each tagged with its owning rule's index — scanning a file's
/// lines through the set once yields exactly the rules with *any* chance of matching it.
pub(super) struct LineScanPrefilter {
    set: regex::RegexSet,
    /// set-pattern-index -> owning rule's index in `pack.rules`.
    ///
    /// POSITIONAL — and therefore the one piece of `RegexCache`-held state that is NOT a pure
    /// function of pattern text (the `entries` memo is pattern-KEYED, safe under any sharing). The
    /// cache — this prefilter inside it — is `Arc`-shared across pack CLONES; an index built against
    /// one clone's rules vec read against another's silently misattributes candidates (falsely
    /// excluding files for shifted rules) or indexes out of bounds. The load-bearing invariant:
    /// **exactly one rules-vec shape may read a given `RegexCache`'s prefilter.** It is enforced at
    /// the two seams that mutate a clone's `rules` after cloning AND evaluate through the prefilter —
    /// `gate_pack_rules` and `envelope_rule_pack` both give their mutated clone
    /// `RegexCache::fork_for_mutated_rules` (pattern memo kept, positional prefilter reset) — and
    /// [`Self::compute_candidates`]'s `debug_assert` is the tripwire for a future mutation seam that
    /// forgets to. Not hypothetical: that assert caught the pre-fork sharing through the public
    /// `analyze_tree` API (one loaded pack, two runs, the second with one rule disabled). One MORE
    /// mutation seam exists and deliberately does not fork: the engine's profiled io-scan lane
    /// (`eval_pack_timed`) clears-and-pushes a clone's rules per rule — safe only because that lane
    /// evaluates exclusively through `eval_pack_io_scan`, which never touches this prefilter; if it
    /// ever grows a line-scan leg, it must fork like the other two.
    pattern_rule: Vec<usize>,
}

impl LineScanPrefilter {
    /// Build the set from `pack`. A `LineScan` rule with no compilable pattern contributes nothing. `None`
    /// if no valid pattern exists, or `RegexSet::new` errors — callers fall back to unfiltered evaluation.
    pub(super) fn build(pack: &RulePackDef) -> Option<Self> {
        let mut patterns: Vec<String> = Vec::new();
        let mut pattern_rule: Vec<usize> = Vec::new();
        for (rule_idx, rule) in pack.rules.iter().enumerate() {
            // Line-scan only, and `Matcher::CallScan` is the arm most likely to look like an oversight:
            // its `callee_pattern` is a regex too, but it matches a PROJECTED callee string rather than a
            // line of text, so feeding it to this set would test it against the wrong subject and reject
            // files that genuinely match (see `eval`'s `CallScan` arm for the worked example). A rule kind
            // that contributes no pattern here is simply always a candidate, which is the only safe
            // default for an optimization that must stay observationally invisible.
            //
            // `Matcher::MethodScan` is excluded for a DIFFERENT reason, and the distinction matters
            // because the objection above does NOT apply to it: its `patterns` match `f.text`, the same
            // haystack this set reads, so a companion set keyed on the `trigger` would be correct. It was
            // built, measured, and removed — the pack's 60 method-scan rules over the 17-tree corpus came
            // out ~4% SLOWER (min-of-4 interleaved, output byte-identical). It loses because
            // `eval_method_scan` already holds a strictly STRONGER whole-text condition — every pattern
            // must match, not just the trigger — and holds it BEHIND the cheap `file_pattern` path check,
            // so it never touches the text of a file the path already rejected. A set here cannot see
            // that gate: `compute_candidates` runs every trigger over every file's text unconditionally.
            // Net-new work guarding a check that was already cheaper and stricter. Do not re-add without
            // first moving the path gate somewhere the pre-filter can consult.
            let Matcher::LineScan(m) = &rule.matcher else {
                continue;
            };
            let rule_patterns: Vec<&str> = match (&m.any, &m.line_pattern) {
                (Some(alts), _) => {
                    let mut v = Vec::with_capacity(alts.len());
                    for lp in alts {
                        if pack.regex_cache.compile(&lp.pattern).is_none() {
                            v.clear();
                            break; // one bad alt -> the whole rule contributes nothing (matches eval_line_scan)
                        }
                        v.push(lp.pattern.as_str());
                    }
                    v
                }
                (None, Some(p)) => {
                    if pack.regex_cache.compile(p).is_some() {
                        vec![p.as_str()]
                    } else {
                        vec![]
                    }
                }
                (None, None) => vec![],
            };
            for p in rule_patterns {
                patterns.push(p.to_string());
                pattern_rule.push(rule_idx);
            }
        }
        if patterns.is_empty() {
            return None;
        }
        let set = regex::RegexSet::new(&patterns).ok()?;
        Some(Self { set, pattern_rule })
    }

    /// `[rule_idx][file_idx] -> bool`: whether that rule has at least one set-pattern hit in that file.
    pub(super) fn compute_candidates(
        &self,
        num_rules: usize,
        files: &[SourceFile],
    ) -> Vec<Vec<bool>> {
        // The `pattern_rule` positional invariant (see the field doc): every stored rule index must
        // be in range for the rules vec the CALLER is evaluating. Cheap relative to the scan below,
        // but it runs once per (pack, FILE) — both engine lanes call `eval_pack` with a one-file
        // slice — so don't grow it into anything heavier than this O(patterns) sweep.
        debug_assert!(
            self.pattern_rule.iter().all(|&i| i < num_rules),
            "LineScanPrefilter built against a different rules vec than it is being read with \
             (max index {:?} >= num_rules {num_rules}) — a mutated pack clone shared this cache",
            self.pattern_rule.iter().max()
        );
        let mut matrix = vec![vec![false; files.len()]; num_rules];
        for (file_idx, f) in files.iter().enumerate() {
            for line in f.text.lines() {
                for pat_idx in self.set.matches(line).iter() {
                    matrix[self.pattern_rule[pat_idx]][file_idx] = true;
                }
            }
        }
        matrix
    }
}
