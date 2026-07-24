//! Pack evaluation entry points — `eval_pack` / `eval_pack_profiled` and the per-rule dispatch.

use crate::finding::Finding;

use super::def::{Matcher, RulePackDef};
use super::ir_scan::eval_symbol_scan;
use super::line_scan::eval_line_scan;
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
    let prefilter = use_prefilter
        .then(|| LineScanPrefilter::build(pack))
        .flatten();
    let candidates = prefilter
        .as_ref()
        .map(|p| p.compute_candidates(pack.rules.len(), ctx.files));
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
                );
            }
            Matcher::MethodScan(m) => {
                eval_method_scan(&pack.id, rule, m, ctx, &mut out, diagnostics)
            }
            Matcher::SymbolScan(m) => {
                eval_symbol_scan(&pack.id, rule, m, ctx, &mut out, diagnostics)
            }
            // io-scan evaluates whole-tree via eval_pack_io_scan since the 2026 projection redesign — see
            // ir_scan.rs's module doc.
            Matcher::IoScan(_) => {}
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
