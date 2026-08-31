//! The RULE-granularity half of the applicability census — what "a rule ADMITTED a file" means, in
//! one place. [`super::compute_dsl_scope`] folds these helpers into its single pass; the result rides
//! the wire as `PackLoaded::zero_admission_rules` (`packsLoaded[].zeroAdmissionRules`).
//!
//! # The definition of "admitted", and why it stops at the path gates
//! A file is admitted by a rule when the rule's PATH gates pass, exactly as evaluation applies them:
//! its `file_pattern` matches the analyzed rel AND its `file_exclude_pattern` (when the matcher
//! carries one — every matcher but `SymbolScan` does) does not. Nothing else counts, for two
//! load-bearing reasons:
//!
//! * **`0` has to mean "green is vacuous".** The path gates are the gates a rule applies BEFORE
//!   reading any content: a rule that admitted zero files could not have read a single byte of this
//!   tree, so its zero findings are pure scope. The content gates (`require_file`,
//!   `require_file_all`, `require_file_absent`) are the opposite case — they are decided by reading
//!   the file's text, so a rule whose `require_file` probe rejected all N files it opened DID judge
//!   those N files ("none of them carries the precondition token" is a verdict, not silence).
//!   Admission counts scrutiny's reach, not its outcome.
//! * **The census must be byte-identical warm and cold.** On a warm run the per-file cache lane skips
//!   evaluation and replays findings, so anything derived from execution — or from file CONTENT the
//!   warm pass never re-reads through the rule path — would flap. Path admission is a pure function
//!   of `(loaded packs, walked rel list)`, both of which every run computes identically.
//!
//! Unlike the PACK-level `files_in_scope` (deliberately pattern-only — see
//! [`super::compute_dsl_scope`]'s doc for why folding excludes into a pack-level number would be
//! incoherent), the rule-level count MUST consult the rule's own `file_exclude_pattern`: a rule whose
//! pattern matches 30 files that its exclude then vetoes wholesale judged nothing, and an
//! exclude-blind count would report it as covered — hiding exactly the silence this census exists to
//! surface. Per rule there is only one exclude, so the pack-level objection does not apply.
//!
//! # The mode filter: a rule the mode never evaluates admits nothing
//! Admission counts scrutiny's REACH, and in envelope mode evaluation retains only
//! `SymbolScan`/`IoScan` rules (`envelope::resolve::rule_runs_in_envelope_mode` — the single
//! definition, shared with the evaluation filter). [`super::compute_dsl_scope_filtered`] therefore
//! forces every mode-dropped rule into the zero-admission list regardless of its path gates: such a
//! rule read nothing, so listing it is the same "green is vacuous" fact this census exists for.
//! Native mode passes an always-true filter and is unaffected.
//!
//! # Compile failures mirror evaluation, not charity
//! A `file_pattern` that fails to compile matches nothing (the `applies_to`/census convention), and a
//! `file_exclude_pattern` that fails to compile makes evaluation skip the rule entirely (see
//! `zzop_core`'s `RuleDiag` early-returns) — both therefore admit zero files, which is the honest
//! count: such a rule reads nothing. `uncompilable_rule_warnings` separately names WHY (the dead-rule
//! channel); this census only reports the reach, and the two saying "zero" together is agreement, not
//! duplication. A compile failure in a CONTENT gate also kills a rule at evaluation, but is NOT
//! folded in here — admission stays a path fact, and the dead-rule warning already owns that failure.

use zzop_core::Matcher;

/// The two path gates of a rule, straight off its matcher: `(file_pattern, file_exclude_pattern)`.
/// `SymbolScan` is the one matcher with no exclude field.
pub(super) fn path_gates(matcher: &Matcher) -> (&str, Option<&str>) {
    match matcher {
        Matcher::LineScan(m) => (&m.file_pattern, m.file_exclude_pattern.as_deref()),
        Matcher::MethodScan(m) => (&m.file_pattern, m.file_exclude_pattern.as_deref()),
        Matcher::SymbolScan(m) => (&m.file_pattern, None),
        Matcher::IoScan(m) => (&m.file_pattern, m.file_exclude_pattern.as_deref()),
        Matcher::CallScan(m) => (&m.file_pattern, m.file_exclude_pattern.as_deref()),
        Matcher::LiteralScan(m) => (&m.file_pattern, m.file_exclude_pattern.as_deref()),
    }
}

/// Memoized per-pattern match mask over `analyzed_rels` — `None` = the pattern failed to compile
/// (matches nothing). One compile + one scan per UNIQUE pattern string per analysis, shared between
/// the pack-level and rule-level counts so the two can never disagree about what a pattern matched.
pub(super) type MaskMemo<'a> = std::collections::HashMap<&'a str, Option<Vec<bool>>>;

/// Ensures `masks` holds an entry for `pattern` (computing it on first sight).
pub(super) fn ensure_mask<'a>(masks: &mut MaskMemo<'a>, pattern: &'a str, analyzed_rels: &[&str]) {
    masks.entry(pattern).or_insert_with(|| {
        regex::Regex::new(pattern)
            .ok()
            .map(|re| analyzed_rels.iter().map(|rel| re.is_match(rel)).collect())
    });
}

/// ORs this rule's admitted files into `union`, marks the FILETYPES it reached in `reach`, and returns
/// how many files it admitted (see the module doc for the definition of admitted). Both patterns must
/// already be in `masks` (the caller's [`ensure_mask`] calls); `union` must be `analyzed_rels.len()`
/// long.
///
/// The count, the union and the filetype marks come out of ONE traversal on purpose: they are three
/// granularities of the same admission fact — the count feeds
/// [`super::DslScope::zero_admission_rules_by_pack`] (which rules read nothing), the union feeds
/// [`super::DslScope::rule_vetoed_rels`] (which FILES nothing read), and the marks feed
/// [`super::DslScope::ext_rule_reach`] (how many rules could reach each LANGUAGE). Computing them from
/// two encodings of "admitted" is how they would come to disagree, and a disagreement here means one of
/// these reports lies about the same tree.
///
/// The caller decides whether the marks count, via [`ExtReach::commit`] / [`ExtReach::discard`] — the
/// mode filter is the caller's (see [`super::compute_dsl_scope_filtered`]).
pub(super) fn fold_admitted(
    masks: &MaskMemo<'_>,
    pattern: &str,
    exclude: Option<&str>,
    union: &mut [bool],
    reach: &mut ExtReach,
) -> usize {
    let Some(Some(pat_mask)) = masks.get(pattern) else {
        return 0; // uncompilable file_pattern: matches nothing, admits nothing
    };
    let mut count = 0;
    match exclude {
        None => {
            for (idx, (slot, matched)) in union.iter_mut().zip(pat_mask.iter()).enumerate() {
                *slot |= matched;
                count += usize::from(*matched);
                if *matched {
                    reach.mark(idx);
                }
            }
        }
        // Uncompilable exclude: evaluation skips the whole rule, so it admits nothing.
        Some(ex) => {
            let Some(Some(ex_mask)) = masks.get(ex) else {
                return 0;
            };
            for (idx, ((slot, matched), vetoed)) in union
                .iter_mut()
                .zip(pat_mask.iter())
                .zip(ex_mask.iter())
                .enumerate()
            {
                let admitted = *matched && !*vetoed;
                *slot |= admitted;
                count += usize::from(admitted);
                if admitted {
                    reach.mark(idx);
                }
            }
        }
    }
    count
}

/// Per-filetype rule reach, accumulated one rule at a time: how many LOADED rules admitted at least one
/// file carrying each extension. The rule-axis complement of [`super::DslScope::ext_census`]'s file-axis
/// count — that one asks "did anything at all target this filetype", this one asks "how much of the rule
/// set could reach it", and the measured answer differs by an order of magnitude between languages
/// (`uncovered_extension`'s `thin_rule_reach_warning` carries the numbers).
///
/// Counts each rule ONCE per extension however many of its files matched: the subject is the rule set's
/// reach, not a file tally, and a rule that admitted 400 `.ts` files is one rule.
pub(super) struct ExtReach {
    /// Per analyzed file, its extension's index into `names` — `None` for a file no native frontend
    /// claims, which is dropped for the same reason [`super::DslScope::ext_census`] drops it.
    ids: Vec<Option<usize>>,
    names: Vec<String>,
    /// Which extensions THIS rule has reached so far; cleared by `commit`/`discard`.
    hit: Vec<bool>,
    counts: Vec<usize>,
}

impl ExtReach {
    /// Builds the per-file extension index from the same whole-`dispatch` claim test `ext_census` uses
    /// (`glob_overrides` included), so the two halves of the language axis see one set of files.
    pub(super) fn new(analyzed_rels: &[&str], dispatch: &crate::DispatchConfig) -> Self {
        let mut names: Vec<String> = Vec::new();
        let ids = analyzed_rels
            .iter()
            .map(|rel| {
                crate::dispatch::dispatch(rel, dispatch)?;
                let ext = std::path::Path::new(rel)
                    .extension()
                    .and_then(|e| e.to_str())?
                    .to_ascii_lowercase();
                Some(match names.iter().position(|n| *n == ext) {
                    Some(idx) => idx,
                    None => {
                        names.push(ext);
                        names.len() - 1
                    }
                })
            })
            .collect();
        let hit = vec![false; names.len()];
        let counts = vec![0; names.len()];
        ExtReach {
            ids,
            names,
            hit,
            counts,
        }
    }

    fn mark(&mut self, file_idx: usize) {
        if let Some(Some(ext_idx)) = self.ids.get(file_idx) {
            self.hit[*ext_idx] = true;
        }
    }

    /// Counts the current rule against every extension it reached, then resets for the next rule.
    pub(super) fn commit(&mut self) {
        for (count, hit) in self.counts.iter_mut().zip(self.hit.iter_mut()) {
            *count += usize::from(*hit);
            *hit = false;
        }
    }

    /// Drops the current rule's marks — the caller's mode filter says this rule never runs, so its
    /// reach is zero however its path gates read (`compute_dsl_scope_filtered`'s doc owns why).
    pub(super) fn discard(&mut self) {
        self.hit.iter_mut().for_each(|hit| *hit = false);
    }

    /// Both halves of the language axis, from the ONE per-file extension classification this struct
    /// already holds: [`super::DslScope::ext_census`] (`ext -> (analyzed, of which in scope)`, read off
    /// `in_scope_mask`) and [`super::DslScope::ext_rule_reach`] (`ext -> rules that reached it`).
    ///
    /// They are returned together because they must be built from one classification. Each was
    /// previously free to decide for itself which files count as which language, and the census's own
    /// doc already records what that costs: keyed on the extension MAP rather than the whole `dispatch`,
    /// a `parsers.globOverrides` path fell between two reports and was named by neither.
    pub(super) fn into_census(
        self,
        in_scope_mask: &[bool],
    ) -> (
        std::collections::BTreeMap<String, (usize, usize)>,
        std::collections::BTreeMap<String, usize>,
    ) {
        let mut census: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
        for (id, matched) in self.ids.iter().zip(in_scope_mask.iter()) {
            let Some(idx) = id else { continue };
            let entry = census.entry(self.names[*idx].clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += usize::from(*matched);
        }
        (census, self.names.into_iter().zip(self.counts).collect())
    }
}
