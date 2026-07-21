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
        // A cycle whose every member is a test file is test-infrastructure structure (e.g. a
        // `playwright/` page-object <-> decorator <-> fixture loop), not deployed runtime coupling — the
        // same "not deployed surface" reasoning `zzop_core::is_test_file` already applies to dead-code and
        // route analyses. A cycle touching even ONE non-test file still fires: that file's real coupling
        // is the thing worth reporting.
        .filter(|cycle| !cycle.iter().all(|f| zzop_core::is_test_file(f)))
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

    #[test]
    fn all_test_file_cycle_is_not_flagged_but_a_mixed_cycle_is() {
        // Every member under a test path -> test-infra structure, exempt (see the filter's doc).
        let test_only = vec![
            "playwright/page-objects/conduit.page-object.ts".to_string(),
            "playwright/utils/test-decorators.ts".to_string(),
        ];
        assert!(circular_findings(&[test_only]).is_empty());
        // A cycle with even one non-test file still fires — the real coupling is worth reporting.
        let mixed = vec![
            "src/article/article.entity.ts".to_string(),
            "playwright/utils/test-decorators.ts".to_string(),
        ];
        assert_eq!(circular_findings(&[mixed]).len(), 1);
    }

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
