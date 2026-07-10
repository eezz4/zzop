//! Finding-shaping for the `"circular"` native analysis. The cycle-detection algorithm (`circular_from_dep`)
//! lives in `zzop_core::graph` as a shared graph primitive (also used by `compute_scores`/
//! `build_recommendations`) — this module only turns an already-computed cycle list into `Finding`s.

use zzop_core::{disable_hint, Finding, Severity};

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
                     direct import). {} if this cycle is an intentional, reviewed pattern (e.g. mutually \
                     recursive types re-exported through a barrel).",
                    cycle.join(" -> "),
                    disable_hint("circular")
                ),
                data: Some(serde_json::json!({ "cycle": cycle })),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact rendered message — regression coverage for the `disable_hint` splice this message
    /// went through during the 2026-07-10 dialect-consolidation sweep.
    #[test]
    fn message_is_byte_identical_to_the_pre_sweep_text() {
        let out = circular_findings(&[vec!["b.ts".to_string(), "a.ts".to_string()]]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "circular");
        assert_eq!(
            out[0].message,
            "circular dependency: a.ts -> b.ts — a change to any file in this cycle can ripple through \
             every other member, making the group hard to reason about, test, or refactor in isolation. \
             Break the cycle by extracting the shared pieces into a module both sides import, or invert \
             one dependency direction (e.g. an interface/callback in place of a direct import). Disable \
             via config `rules: { \"circular\": \"off\" }` (embedders: `disabled_rules`) if this cycle is \
             an intentional, reviewed pattern (e.g. mutually recursive types re-exported through a \
             barrel)."
        );
    }
}
