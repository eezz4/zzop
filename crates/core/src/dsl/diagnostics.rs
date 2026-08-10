//! Rule-skip diagnostics — the DSL evaluator's "why did this rule never run" channel.
//!
//! Every matcher compiles its regexes ONCE, up front, and skips the whole rule when one fails to compile:
//! a malformed rule must never fail the run, analysis stays best-effort. Silent, that skip is the worst
//! outcome this engine can produce — the rule never fires, the run reports clean, and nothing says the
//! difference between "no violations" and "this rule was never evaluated". So every skip pushes one line
//! into a caller-owned `Vec<String>` sink, the same shape the engine's user-facing warnings channel
//! already uses.
//!
//! Kernel-generic on purpose: a message carries the pack-prefixed rule id, the DSL FIELD name, and the
//! regex crate's own error — never a per-rule branch, never rule vocabulary. `zzop-core` stays
//! rule-vocabulary-free.
//!
//! `pack_loader::pack_regex_issues` reports the same class of defect ahead of a run, for a pack a user
//! chose to validate; this channel reports what an ACTUAL evaluation actually skipped, including packs
//! nothing forced through that validator (inline `packDefs`, a `packsDir` pack, a bundled pack).
//!
//! Every message here points at the validator in BOTH host dialects — `` `zzop validate-rule-pack
//! <pack.json>` `` and the `validate_rule_pack` MCP tool — the same pairing
//! `zzop_engine::analyze::diagnostics::capability`'s load-scope twin uses. This sink is a PUBLIC
//! embedder channel (`dsl::eval_pack_into`), so the crate cannot know which host will render the line;
//! naming one spelling hands the other audience a command it cannot type. Contract 16
//! (`crates/engine/tests/rule_contracts/host_vocabulary.rs`) scans this file for exactly that.

use super::def::LabeledPattern;

/// One rule's view of the diagnostics sink: the pack-prefixed rule id plus the shared accumulator every
/// message lands in. Built per rule by each matcher, so no call site has to repeat the id.
pub(super) struct RuleDiag<'a> {
    rule_id: &'a str,
    sink: &'a mut Vec<String>,
    /// The owning pack's compiled-regex memo — see [`crate::dsl::RegexCache`]. It rides here rather than
    /// being threaded through each call because every regex a rule needs already passes through this
    /// type, so one field reaches all of them.
    cache: &'a crate::dsl::RegexCache,
}

impl<'a> RuleDiag<'a> {
    pub(super) fn new(
        rule_id: &'a str,
        sink: &'a mut Vec<String>,
        cache: &'a crate::dsl::RegexCache,
    ) -> Self {
        Self {
            rule_id,
            sink,
            cache,
        }
    }

    /// The owning pack's memo, for the two compile paths that do NOT go through `compile`/`compile_opt`:
    /// the prefilter's validity probe and the derived suppress marker.
    pub(super) fn cache(&self) -> &'a crate::dsl::RegexCache {
        self.cache
    }

    /// Appends `message` unless the sink already carries it verbatim. The per-file DSL pass calls
    /// `eval_pack` once PER FILE against one shared accumulator, so a single broken pattern would
    /// otherwise repeat the identical line once per scanned file — thousands of copies of one defect.
    /// The linear scan is over a vec that only ever holds one entry per broken field, never a hot path.
    fn push(&mut self, message: String) {
        if !self.sink.contains(&message) {
            self.sink.push(message);
        }
    }

    /// Compiles a REQUIRED regex field. `None` means it did not compile and the failure is already
    /// reported — the caller's only correct response is to return (skip the rule).
    pub(super) fn compile(&mut self, field: &str, pattern: &str) -> Option<regex::Regex> {
        // The memo answers the SUCCESS path (the hot one — the same pattern arrives once per scanned
        // file). The FAILURE path re-derives the error text once per file that reaches the broken
        // rule: the memo stores the miss as `None` with no error attached, so `compile_err` re-runs
        // the failing compile here each time. What `push` below de-duplicates is the resulting
        // MESSAGE — one line per broken field per run — not this derivation; see
        // `RegexCache::compile_err` for why the error itself stays uncached.
        match self
            .cache
            .compile(pattern)
            .ok_or_else(|| crate::dsl::RegexCache::compile_err(pattern))
        {
            Ok(re) => Some(re),
            Err(err) => {
                let err = err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "regex did not compile".to_string());
                let message = format!(
                    "rule \"{}\": `{field}` is not a valid regex — the rule was SKIPPED and can never fire; \
                     catch this before a scan with `zzop validate-rule-pack <pack.json>` (CLI binary) / \
                     the `validate_rule_pack` MCP tool. regex error: {err}",
                    self.rule_id
                );
                self.push(message);
                None
            }
        }
    }

    /// Compiles an OPTIONAL regex field. Two levels of `Option` because the two outcomes must not
    /// collapse: outer `None` = the pattern was present and failed to compile (already reported, caller
    /// returns), `Some(None)` = the field is simply absent, which is not a defect. Folding a failure into
    /// "absent" would silently DROP a filter and make the rule fire more, not less.
    pub(super) fn compile_opt(
        &mut self,
        field: &str,
        pattern: Option<&String>,
    ) -> Option<Option<regex::Regex>> {
        match pattern {
            Some(p) => self.compile(field, p).map(Some),
            None => Some(None),
        }
    }

    /// Compiles every element of a `Vec<String>` regex field (`require_file_all`/`require_file_absent`).
    /// Stops at the first failure — that one message already names the field, and the rule is dead either
    /// way.
    pub(super) fn compile_all(
        &mut self,
        field: &str,
        patterns: &[String],
    ) -> Option<Vec<regex::Regex>> {
        let mut out = Vec::with_capacity(patterns.len());
        for p in patterns {
            out.push(self.compile(field, p)?);
        }
        Some(out)
    }

    /// `compile_all` for a labeled-pattern field (`any`/`patterns`/`absent`), carrying each label through.
    pub(super) fn compile_labeled(
        &mut self,
        field: &str,
        patterns: &[LabeledPattern],
    ) -> Option<Vec<(regex::Regex, String)>> {
        let mut out = Vec::with_capacity(patterns.len());
        for lp in patterns {
            out.push((self.compile(field, &lp.pattern)?, lp.label.clone()));
        }
        Some(out)
    }

    /// A STRUCTURAL reason the rule cannot run (no pattern field at all, a `trigger` naming a label that
    /// is not in `patterns`, ...) — same silent-death class as a bad regex, same visibility. `why` is a
    /// lowercase clause completing "rule \"x\": <why> — the rule was SKIPPED ...".
    pub(super) fn malformed(&mut self, why: &str) {
        let message = format!(
            "rule \"{}\": {why} — the rule was SKIPPED and can never fire; catch this before a scan \
             with `zzop validate-rule-pack <pack.json>` (CLI binary) / the `validate_rule_pack` MCP tool.",
            self.rule_id
        );
        self.push(message);
    }
}
