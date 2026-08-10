//! Pack evaluation entry points — `eval_pack` / `eval_pack_profiled` and the per-rule dispatch.

use crate::finding::Finding;

use super::call_scan::eval_call_scan;
use super::def::{Matcher, RulePackDef};
use super::ir_scan::eval_symbol_scan;
use super::line_scan::eval_line_scan;
use super::literal_scan::eval_literal_scan;
use super::method_scan::eval_method_scan;
use super::prefilter::LineScanPrefilter;
use super::source::{RuleContext, RuleTiming};

/// Evaluate a whole rule pack -> findings. DROPS the rule-skip diagnostics (see `dsl::diagnostics`): a
/// rule whose pattern does not compile is skipped here as silently as it always was.
///
/// This is what `zzop-engine` calls, deliberately. Its user-facing disclosure of dead rules is
/// LOAD-scoped, not run-scoped — `pack_regex_issues` over every loaded pack, surfaced by the engine's
/// `uncompilable_rule_warnings` — because threading an accumulator through the parallel per-file pass
/// buys nothing a load-scope census does not already say, and the census additionally names a rule no
/// file in the run happened to reach. [`eval_pack_into`] is the run-scoped alternative for an embedder
/// that wants exactly "what did THIS pass skip"; nothing in this workspace needs that today.
pub fn eval_pack(pack: &RulePackDef, ctx: &RuleContext) -> Vec<Finding> {
    eval_pack_into(pack, ctx, &mut Vec::new())
}

/// [`eval_pack`] with the rule-skip diagnostics channel attached: one line per rule this pass skipped
/// because a regex did not compile (or the rule is structurally malformed), naming the rule id, the DSL
/// field, and the regex crate's own error — see the `diagnostics` module doc. Findings are identical to
/// `eval_pack`'s; the sink only makes an already-happening skip visible. `diagnostics` is an accumulator
/// the caller may reuse across the whole run (messages dedupe against what it already holds), which is
/// what keeps a per-file pass from repeating one defect once per file.
pub fn eval_pack_into(
    pack: &RulePackDef,
    ctx: &RuleContext,
    diagnostics: &mut Vec<String>,
) -> Vec<Finding> {
    eval_pack_impl(pack, ctx, true, false, diagnostics).0
}

/// `eval_pack` with the `RegexSet` pre-filter forced off — the reference the differential test compares against.
#[cfg(test)]
pub(super) fn eval_pack_no_prefilter(pack: &RulePackDef, ctx: &RuleContext) -> Vec<Finding> {
    eval_pack_impl(pack, ctx, false, false, &mut Vec::new()).0
}

/// Same as `eval_pack`, plus a `RuleTiming` per rule (wall time via `std::time::Instant`). Findings are
/// byte-for-byte identical to `eval_pack`'s, since this only adds timing around each rule's dispatch.
/// Drops diagnostics exactly like `eval_pack` does — [`eval_pack_profiled_into`] is the reporting twin.
pub fn eval_pack_profiled(
    pack: &RulePackDef,
    ctx: &RuleContext,
) -> (Vec<Finding>, Vec<RuleTiming>) {
    eval_pack_profiled_into(pack, ctx, &mut Vec::new())
}

/// [`eval_pack_profiled`] with the rule-skip diagnostics channel attached — the profiled twin of
/// [`eval_pack_into`], so turning profiling on can never cost a caller its diagnostics.
pub fn eval_pack_profiled_into(
    pack: &RulePackDef,
    ctx: &RuleContext,
    diagnostics: &mut Vec<String>,
) -> (Vec<Finding>, Vec<RuleTiming>) {
    eval_pack_impl(pack, ctx, true, true, diagnostics)
}

fn eval_pack_impl(
    pack: &RulePackDef,
    ctx: &RuleContext,
    use_prefilter: bool,
    profile: bool,
    diagnostics: &mut Vec<String>,
) -> (Vec<Finding>, Vec<RuleTiming>) {
    let mut out = Vec::new();
    let mut timings = Vec::new();
    // Built through the pack's own cache, not per call: `eval_pack_impl` runs once per FILE, and a
    // `RegexSet` compiles every line-scan pattern into one automaton — the most rule-count-proportional
    // work in the pass. See `RegexCache`'s `prefilter` field for the measurement.
    let prefilter = use_prefilter
        .then(|| {
            pack.regex_cache
                .prefilter_or_init(|| LineScanPrefilter::build(pack))
        })
        .flatten();
    let candidates = prefilter
        .as_ref()
        .map(|p| p.compute_candidates(pack.rules.len(), ctx.files));
    // Built once per pack rather than per rule: the answer is a property of the FILES, and 140 rules
    // asking it 140 times would be 140 identical walks. `None` = no file in this context declares a
    // test-only region, which is the case for every language but Rust today and therefore the path that
    // must cost nothing.
    let test_regions = TestRegions::build(ctx);
    for (rule_idx, rule) in pack.rules.iter().enumerate() {
        let start_len = out.len();
        let t0 = profile.then(std::time::Instant::now);
        match &rule.matcher {
            Matcher::LineScan(m) => {
                let file_candidates = candidates.as_ref().map(|c| c[rule_idx].as_slice());
                eval_line_scan(
                    &pack.id,
                    rule,
                    m,
                    ctx,
                    file_candidates,
                    &mut out,
                    diagnostics,
                    &pack.regex_cache,
                );
            }
            Matcher::MethodScan(m) => eval_method_scan(
                &pack.id,
                rule,
                m,
                ctx,
                &mut out,
                diagnostics,
                &pack.regex_cache,
            ),
            Matcher::SymbolScan(m) => eval_symbol_scan(
                &pack.id,
                rule,
                m,
                ctx,
                &mut out,
                diagnostics,
                &pack.regex_cache,
            ),
            // No `candidates` lookup, deliberately: the `RegexSet` pre-filter is built from LINE-TEXT
            // patterns and answers "could this rule's regex hit some line of this file". `callee_pattern`
            // is anchored on a PROJECTED callee string (`console.error`), not on a line, so a set built
            // from it would answer a different question — a rule whose callee pattern is `^console\.` has
            // no line beginning with `console.` when the call is `  console.error(x)`, and the pre-filter
            // would drop a file that genuinely matches. Since a pre-filter must be observationally
            // invisible, the conservative reading is the only correct one: every file is a candidate, and
            // `eval_call_scan`'s own `call_sites.is_empty()` pre-skip is what makes that cheap.
            Matcher::CallScan(m) => eval_call_scan(
                &pack.id,
                rule,
                m,
                ctx,
                &mut out,
                diagnostics,
                &pack.regex_cache,
            ),
            // Same conservative no-prefilter reading as `CallScan` directly above: the `RegexSet`
            // pre-filter is built from LINE-TEXT patterns, and a `name_pattern` is anchored on a
            // projected binding NAME, not on a line — `eval_literal_scan`'s own
            // `string_literals.is_empty()` pre-skip is what makes every-file-a-candidate cheap.
            Matcher::LiteralScan(m) => eval_literal_scan(
                &pack.id,
                rule,
                m,
                ctx,
                &mut out,
                diagnostics,
                &pack.regex_cache,
            ),
            // io-scan evaluates whole-tree via eval_pack_io_scan since the 2026 projection redesign — see
            // ir_scan.rs's module doc.
            Matcher::IoScan(_) => {}
        }
        // AFTER the matcher, BEFORE the timing record — so `RuleTiming::findings` reports what the rule
        // actually contributed rather than what it would have contributed on test code. Skipped entirely
        // for a rule that declared `scan_test_regions` (see `RuleDef`'s field doc): a credential-at-rest
        // rule judges the COMMIT, not the execution, so a test region is not a carve-out for it.
        if let Some(regions) = &test_regions {
            if !rule.scan_test_regions {
                regions.drop_test_only(&mut out, start_len);
            }
        }
        if let Some(t0) = t0 {
            timings.push(RuleTiming {
                rule_id: format!("{}/{}", pack.id, rule.id),
                nanos: t0.elapsed().as_nanos(),
                findings: out.len() - start_len,
            });
        }
    }
    (out, timings)
}

/// The per-pass index behind the ONE place a DSL finding is dropped for sitting in test-only code.
///
/// ## Why it is the DEFAULT rather than something every rule opts into
/// Almost every pack rule already declares this intent — `"file_exclude_pattern": "${test-paths-stories}"`
/// appears on rule after rule — and it declares it as a PATH regex, because in every language but Rust
/// the path is where test-ness lives. Rust's dominant convention (`#[cfg(test)] mod tests` inside the
/// shipping file) is invisible to that regex, so the same intent needs a second kind of evidence rather
/// than a second policy. Requiring every Rust-facing rule ever written to remember to ask for what
/// almost all of them already want is the wiring nobody connects.
///
/// ## Why it is nonetheless not UNCONDITIONAL — [`RuleDef::scan_test_regions`]
/// "Almost every" is not "every", and the exception is not cosmetic. Seven credential rules in
/// `rules/dsl/security/security.json` carry NO `file_exclude_pattern` on purpose, three of them
/// `critical`, and `private-key-committed`'s shipped message says so outright: *"an actual PEM header
/// sitting in a test fixture is still a committed key, so this rule scans test paths too, not just
/// application code."* For that class the COMMIT is the leak — a `-----BEGIN RSA PRIVATE KEY-----` or a
/// `postgres://user:pw@host` inside `#[cfg(test)] mod tests` is in git history, in every fork and every
/// clone, and has to be rotated whether or not the surrounding code ships. An unconditional gate deleted
/// exactly those findings and reported the run clean, which is this repo's cardinal failure. So the gate
/// is a default with a declared opt-out, and the opt-out's consistency with what the rule PUBLISHES is
/// itself guarded (`crates/facade/src/test_region_promise_tests.rs`), because a hand list is what rots.
///
/// ## What it deliberately does not cover
/// - `Matcher::IoScan`, which is whole-tree (`ir_scan::eval_pack_io_scan`) and never sees a
///   `SourceFile`. It needs no cover here: an IO fact is gated where it is EXTRACTED — every adapter
///   under `zzop_parser_rust::adapters` skips test-gated subtrees, so the channels they feed are clean
///   for EVERY consumer (the cross-layer join, the coverage census, the endpoint verdict), not
///   just rule packs. That is a per-adapter DISCIPLINE, not a structural guarantee: nothing forces a
///   new adapter — here or in another parser crate — to carry the gate, and each of the Rust ones was
///   found missing some part of it and repaired separately (2026-08-02). No adapter COUNT is written
///   here on purpose; the invariant worth checking is "every one of them", which `ls` answers and a
///   number silently stops answering. Gating io-scan here instead would clean the findings while leaving the same test
///   route in the join — a worse inconsistency than the one it closed. An external producer declaring
///   `test_spans` therefore also has to withhold the io facts it extracted from those spans; the
///   external-parser contract says so at the field (`docs/NORMALIZED_AST.md`,
///   `docs/adapters/envelope.schema.json`).
/// - Native (non-DSL) rules, which do not route through `eval_pack` at all.
///
/// ## Reach
/// Bounded by the FACT, not by a policy: `SourceFile::test_spans` is non-empty only where a parser
/// proved a region is compiled out of the shipping build. Among zzop's own parsers only Rust fills it
/// (`zzop_parser_rust::extract_test_spans`), so on a natively-parsed tree no rule outside `.rs` sees a
/// change. That is today's PARSER coverage and nothing stronger — the field is open on the
/// external-parser wire, where an adapter may declare spans for any language, so nothing here may be
/// written as if `.rs` were the permanent ceiling.
struct TestRegions<'a> {
    /// `rel -> that file`, built only for files that actually declare a region.
    by_rel: std::collections::HashMap<&'a str, &'a super::source::SourceFile>,
}

impl<'a> TestRegions<'a> {
    /// `None` when no file in this context declares a test-only region — the overwhelmingly common case,
    /// which then costs one pass over `ctx.files` per pack and nothing else.
    fn build(ctx: &'a RuleContext<'a>) -> Option<Self> {
        let by_rel: std::collections::HashMap<&str, &super::source::SourceFile> = ctx
            .files
            .iter()
            .filter(|f| !f.test_spans.is_empty())
            .map(|f| (f.rel.as_str(), f))
            .collect();
        (!by_rel.is_empty()).then_some(Self { by_rel })
    }

    /// Drops the findings `out[from..]` that anchor on a test-only line. Splits the tail off rather than
    /// removing in place so the cost is linear in the findings ONE rule produced, never quadratic.
    fn drop_test_only(&self, out: &mut Vec<Finding>, from: usize) {
        if from >= out.len() {
            return;
        }
        let tail = out.split_off(from);
        out.extend(tail.into_iter().filter(|f| {
            !self
                .by_rel
                .get(f.file.as_str())
                .is_some_and(|src| src.is_test_only_line(f.line))
        }));
    }
}
