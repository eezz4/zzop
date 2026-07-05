//! Finding-shaping for the `"circular"` native analysis. The cycle-detection algorithm (`circular_from_dep`)
//! lives in `zzop_core::graph` as a shared graph primitive (also used by `compute_scores`/
//! `build_recommendations`) — this module only turns an already-computed cycle list into `Finding`s.

use zzop_core::{Finding, Severity};

/// One `Finding` per cycle (native analysis id `"circular"`, matching `register_native_analyses`).
/// `file`/message use the cycle's *sorted* member list rather than raw Tarjan discovery order, so the
/// finding is deterministic independent of that internal ordering.
pub fn circular_findings(cycles: &[Vec<String>]) -> Vec<Finding> {
    cycles
        .iter()
        .cloned()
        .map(|mut cycle| {
            cycle.sort();
            let representative = cycle[0].clone();
            Finding {
                rule_id: "circular".to_string(),
                severity: Severity::Warning,
                file: representative,
                line: 1,
                message: format!(
                    "circular dependency: {} — a change to any file in this cycle can ripple through \
                     every other member, making the group hard to reason about, test, or refactor in \
                     isolation. Break the cycle by extracting the shared pieces into a module both sides \
                     import, or invert one dependency direction (e.g. an interface/callback in place of a \
                     direct import). Disable via rule config `disabled_rules: [\"circular\"]` if this \
                     cycle is an intentional, reviewed pattern (e.g. mutually recursive types re-exported \
                     through a barrel).",
                    cycle.join(" -> ")
                ),
                data: Some(serde_json::json!({ "cycle": cycle })),
            }
        })
        .collect()
}
