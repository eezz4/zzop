//! The replacement half of `cross-layer/all-consumes-unjoined`: dropping the per-consume findings a fired
//! per-tree fold now stands in for. Split from the rule body to keep that file under the line-count
//! ratchet; the two belong to one contract and the parent module's doc holds it.

use std::collections::BTreeSet;

use zzop_core::Finding;

/// The rule ids a fired fold REPLACES. Both are per-call-site verdicts that go dark with the join: an
/// `ambiguous-consume` restates "several trees provide this" once per call, and an
/// `unprovided-mutation-call` asserts "no provider anywhere", which is simply FALSE when the provider is in
/// the run behind an unresolved base. Nothing else is listed on purpose — an AGGREGATE (`prefix-drift`,
/// `unresolved-consume-ratio`) is one-per-cause already and, when it fires, is the better message.
const REPLACED: &[&str] = &[
    "cross-layer/ambiguous-consume",
    "cross-layer/unprovided-mutation-call",
];

/// Drops the per-consume findings replaced by a fired aggregate. Matches on the finding's own
/// `data.consumeSource` — the key both replaced rules already emit — so no positional coupling to the rule
/// module is introduced. A finding without that key is KEPT (defensive: an unrecognized shape must never
/// vanish, which is the exact failure this whole rule exists to remove), and only [`REPLACED`] ids are
/// considered at all.
pub fn retain_non_subsumed_sources(
    findings: Vec<Finding>,
    subsumed_sources: &BTreeSet<String>,
) -> Vec<Finding> {
    findings
        .into_iter()
        .filter(|f| {
            if !REPLACED.contains(&f.rule_id.as_str()) {
                return true;
            }
            let Some(source) = f
                .data
                .as_ref()
                .and_then(|d| d.get("consumeSource"))
                .and_then(|v| v.as_str())
            else {
                return true;
            };
            !subsumed_sources.contains(source)
        })
        .collect()
}
