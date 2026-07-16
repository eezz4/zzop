//! Pack evaluation entry points — `eval_pack` / `eval_pack_profiled` and the per-rule dispatch.

use crate::finding::Finding;

use super::def::{Matcher, RulePackDef};
use super::ir_scan::{eval_io_scan, eval_symbol_scan};
use super::line_scan::eval_line_scan;
use super::method_scan::eval_method_scan;
use super::prefilter::LineScanPrefilter;
use super::source::{RuleContext, RuleTiming};

/// Evaluate a whole rule pack -> findings.
pub fn eval_pack(pack: &RulePackDef, ctx: &RuleContext) -> Vec<Finding> {
    eval_pack_impl(pack, ctx, true, false).0
}

/// `eval_pack` with the `RegexSet` pre-filter forced off — the reference the differential test compares against.
#[cfg(test)]
pub(super) fn eval_pack_no_prefilter(pack: &RulePackDef, ctx: &RuleContext) -> Vec<Finding> {
    eval_pack_impl(pack, ctx, false, false).0
}

/// Same as `eval_pack`, plus a `RuleTiming` per rule (wall time via `std::time::Instant`). Findings are
/// byte-for-byte identical to `eval_pack`'s, since this only adds timing around each rule's dispatch.
pub fn eval_pack_profiled(
    pack: &RulePackDef,
    ctx: &RuleContext,
) -> (Vec<Finding>, Vec<RuleTiming>) {
    eval_pack_impl(pack, ctx, true, true)
}

fn eval_pack_impl(
    pack: &RulePackDef,
    ctx: &RuleContext,
    use_prefilter: bool,
    profile: bool,
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
                eval_line_scan(&pack.id, rule, m, ctx, file_candidates, &mut out);
            }
            Matcher::MethodScan(m) => eval_method_scan(&pack.id, rule, m, ctx, &mut out),
            Matcher::SymbolScan(m) => eval_symbol_scan(&pack.id, rule, m, ctx, &mut out),
            Matcher::IoScan(m) => eval_io_scan(&pack.id, rule, m, ctx, &mut out),
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
