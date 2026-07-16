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
    pattern_rule: Vec<usize>,
}

impl LineScanPrefilter {
    /// Build the set from `pack`. A `LineScan` rule with no compilable pattern contributes nothing. `None`
    /// if no valid pattern exists, or `RegexSet::new` errors — callers fall back to unfiltered evaluation.
    pub(super) fn build(pack: &RulePackDef) -> Option<Self> {
        let mut patterns: Vec<String> = Vec::new();
        let mut pattern_rule: Vec<usize> = Vec::new();
        for (rule_idx, rule) in pack.rules.iter().enumerate() {
            let Matcher::LineScan(m) = &rule.matcher else {
                continue;
            };
            let rule_patterns: Vec<&str> = match (&m.any, &m.line_pattern) {
                (Some(alts), _) => {
                    let mut v = Vec::with_capacity(alts.len());
                    for lp in alts {
                        if regex::Regex::new(&lp.pattern).is_err() {
                            v.clear();
                            break; // one bad alt -> the whole rule contributes nothing (matches eval_line_scan)
                        }
                        v.push(lp.pattern.as_str());
                    }
                    v
                }
                (None, Some(p)) => {
                    if regex::Regex::new(p).is_ok() {
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
