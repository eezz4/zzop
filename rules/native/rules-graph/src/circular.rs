//! Finding-shaping for the `"circular"` native analysis. The cycle-detection algorithm (`circular_from_dep`)
//! lives in `zzop_core::graph` as a shared graph primitive (also used by `compute_scores`/
//! `build_recommendations`) — this module only turns an already-computed cycle list into `Finding`s.

use zzop_core::{disable_hint, Finding, Severity};

mod landing;

use landing::CYCLE_EXTRACTION_LANDING;

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
                // Ships `info`. The message below ends by naming a legitimate way to KEEP the cycle
                // ("if this cycle is an intentional, reviewed pattern"), and a rule that publishes an
                // "intended is fine" exit is reporting a SHAPE, not claiming a defect. A 2026-09-06
                // product decision moved that whole class out of the band a first screen is built from:
                // measured over three fixed corpus trees this rule held 12 of the 150 first-screen rows,
                // and all 12 were this one rule. Nothing is dropped -- findings, counts and the
                // dependency-graph surface are unchanged; they sit behind the rules that claim a defect.
                severity: Severity::Info,
                file: representative,
                line: 1,
                message: format!(
                    "circular dependency: {} — a change to any file in this cycle can ripple through \
                     every other member, making the group hard to reason about, test, or refactor in \
                     isolation. {CYCLE_EXTRACTION_LANDING} Break the cycle by extracting the shared \
                     pieces into a module both sides import, or invert one dependency direction (e.g. an \
                     interface/callback in place of a direct import). {} if this cycle is an \
                     intentional, reviewed pattern (e.g. mutually recursive types re-exported through a \
                     barrel).",
                    cycle.join(" -> "),
                    disable_hint("circular")
                ),
                // Every other member of the cycle — the message prints the whole chain.
                evidence_paths: cycle.iter().skip(1).cloned().collect(),
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
    /// went through during the 2026-07-10 dialect-consolidation sweep, and for the §27 landing spliced
    /// in front of the imperative on 2026-09-12.
    ///
    /// The landing is reached through the constant rather than written out again: a second copy of that
    /// sentence in this file would be the thing `landing.rs` exists to prevent, and the order pin below
    /// locates it with `find`, which means nothing against a needle that occurs twice.
    #[test]
    fn message_is_byte_identical_to_the_pre_sweep_text() {
        let out = circular_findings(&[vec!["b.ts".to_string(), "a.ts".to_string()]]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "circular");
        assert_eq!(
            out[0].message,
            format!(
                "circular dependency: a.ts -> b.ts — a change to any file in this cycle can ripple \
                 through every other member, making the group hard to reason about, test, or refactor \
                 in isolation. {CYCLE_EXTRACTION_LANDING} Break the cycle by extracting the shared \
                 pieces into a module both sides import, or invert one dependency direction (e.g. an \
                 interface/callback in place of a direct import). Disable via config `rules: {{ \
                 \"circular\": \"off\" }}` (embedders: `disabledRules`) if this cycle is an \
                 intentional, reviewed pattern (e.g. mutually recursive types re-exported through a \
                 barrel)."
            )
        );
    }

    /// The §27 ORDER pin: LANDING < IMPERATIVE. Position, not existence — a reader who acts on the first
    /// instruction never reaches a caveat placed behind it. INVALIDATION PROBE: move
    /// `{CYCLE_EXTRACTION_LANDING}` in the `format!` above to just before the `disable_hint` splice and
    /// leave every token present; this must go red on ORDER alone.
    ///
    /// This rule carries no DISQUALIFIER and the pin does not pretend otherwise (§38: whether the
    /// finding is TRUE is a separate question from what acting on it costs, and a cycle the graph
    /// reports is a cycle). Its trailing "intentional, reviewed pattern" clause is an EXIT — a reason to
    /// keep the cycle — not a condition under which the finding is wrong.
    #[test]
    fn the_cycle_extraction_landing_precedes_the_break_imperative() {
        let out = circular_findings(&[vec!["b.ts".to_string(), "a.ts".to_string()]]);
        let msg = &out[0].message;
        for (name, needle) in [
            ("the landing clause", CYCLE_EXTRACTION_LANDING),
            ("the imperative", "Break the cycle by extracting"),
        ] {
            assert_eq!(
                msg.matches(needle).count(),
                1,
                "circular: {name} must be spelled ONCE, or an index comparison means nothing: {msg}"
            );
        }
        let land = msg.find(CYCLE_EXTRACTION_LANDING).expect("landing missing");
        let verb = msg
            .find("Break the cycle by extracting")
            .expect("imperative missing");
        assert!(
            land < verb,
            "the landing is at {land} and the imperative at {verb} -- a reader who acts on the \
             instruction never reaches the caveat behind it: {msg}"
        );
        // The facts the clause exists to carry. Presence, unlike order, is what a reword loses, and each
        // of these is a claim the sentence has to be able to stand behind.
        for needle in [
            "READS IMPORTS RATHER THAN WHAT THEY EXECUTE",
            "type-only re-exports is erased before anything runs",
            "TDZ ReferenceError",
            "Nothing fails to build either way",
            "that caller is outside the cycle printed above",
        ] {
            assert!(
                CYCLE_EXTRACTION_LANDING.contains(needle),
                "CYCLE_EXTRACTION_LANDING lost {needle:?}"
            );
        }
    }
}
