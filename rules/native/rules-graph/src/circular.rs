//! Finding-shaping for the `"circular"` native analysis. The cycle-detection algorithm (`circular_from_dep`)
//! lives in `zzop_core::graph` as a shared graph primitive (also used by `compute_scores`/
//! `build_recommendations`) — this module only turns an already-computed cycle list into `Finding`s.

use zzop_core::{disable_hint, Finding, Severity};

mod landing;

use landing::CYCLE_EXTRACTION_LANDING;

/// How many cycle members the MESSAGE prints before it stops and says how many it dropped.
///
/// 📏 2026-10-07, measured on `corpus/audit/nocodb`: one cycle of **510** members rendered a 32,522-char
/// chain inside a 34,158-char message, and that single `info` finding was **97,974 bytes — 38.8% of the
/// whole 252,508-byte reply**. A reader cannot act on a 510-step sentence, and the message's own last
/// clause already concedes it ("the import you want is in one of those files"), so the prose was paying
/// list prices for something it does not deliver as a list.
///
/// Nothing is dropped: `data.cycle` stays uncapped and the message says so. 25 is not a new number —
/// it is `unconsumed_endpoint::MAX_LISTED_PER_SOURCE`, the cap this repo already chose for "a listing
/// long enough to show the shape, short enough to read" (its siblings use 8, 5, 4 and 3 for narrower
/// lists). Taking the house value rather than inventing one is deliberate: a cap picked per rule is a
/// number nobody can re-derive.
///
/// 🔴 The second cost is the one that does not look like this rule's problem. `summary`'s template fold
/// is ALL-OR-NOTHING per rule group (`rule_prose::template::mine::MAX_TOKENS`), so the single over-cap
/// message ALSO un-folded its six innocent siblings — 📏 nocodb 7 findings / 0 folded / 13,626 chars
/// paid inline, against immich's 8 findings / 8 folded / 191 chars each, same rule and same binary.
const MAX_CHAIN_PRINTED: usize = 25;

/// The cycle as prose, capped. Reads as a path while it fits and says what it withheld once it does not.
/// The cycle's members as prose, capped — joined with COMMAS rather than arrows.
///
/// # Why the separator is the fix and the sentence was not
/// This rendered `a -> b -> c` until 2026-10-09, and those arrows were never import edges. The
/// members are sorted (`cycle.sort()` below, for determinism), so the sequence is LEXICOGRAPHIC and
/// adjacent entries usually do not import each other at all. 📏 Reproduced by an outside auditor on
/// `corpus/audit/immich` and re-checked here: the first arrow of one finding read
/// `server/src/cores/storage.core.ts -> server/src/middleware/auth.guard.ts`, and
/// `grep -c 'auth.guard' server/src/cores/storage.core.ts` is **0**. The auditor went looking for the
/// named import, could not find it, and changed no code.
///
/// 🔴 The prose tried twice to carry this and failed both times. It first said the chain "names every
/// edge" — simply false. That was replaced (2026-10-07) with "names the members in order", which is
/// worse in a subtler way: a reader takes "in order" to mean traversal order, and it means
/// `sort()`. Both attempts were a sentence apologising for a punctuation mark. Commas do not need the
/// apology: a comma-separated list claims membership and nothing about adjacency, which is exactly
/// what this data is.
fn chain_prose(cycle: &[String]) -> String {
    if cycle.len() <= MAX_CHAIN_PRINTED {
        return cycle.join(", ");
    }
    format!(
        "{}, ... (+{} more members; the full cycle is in `data.cycle`, uncapped)",
        cycle[..MAX_CHAIN_PRINTED].join(", "),
        cycle.len() - MAX_CHAIN_PRINTED
    )
}

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
                // 🔴 A PLACEHOLDER, not evidence (2026-09-24, review ledger V312). An uncontaminated
                // auditor opened this citation on immich and found `import { dirname, join, resolve }
                // from 'node:path'` — line 1 of the alphabetically-first member — while the import that
                // actually closes the cycle sat on line 22. The file opened, so the citation read as
                // evidence, and it was not.
                //
                // Anchoring at the edge is not available here and the price of making it available is
                // known: `DepGraph` is `HashMap<String, Vec<String>>` (`zzop_core::ir`), so no import
                // edge carries a line anywhere in the IR, and putting one there moves the type, all
                // eight parser front-ends that build it, and every consumer that walks it. That is a
                // whole-IR change to improve one anchor, so the honest move today is the message saying
                // what this number is — the line below does that, and the chain in the message names
                // every edge the reader actually needs.
                line: 1,
                message: format!(
                    "circular dependency: {} — a change to any file in this cycle can ripple through \
                     every other member, making the group hard to reason about, test, or refactor in \
                     isolation. {CYCLE_EXTRACTION_LANDING} Break the cycle by extracting the shared \
                     pieces into a module both sides import, or invert one dependency direction (e.g. an \
                     interface/callback in place of a direct import). {} if this cycle is an \
                     intentional, reviewed pattern (e.g. mutually recursive types re-exported through a \
                     barrel). WHERE THIS FINDING POINTS: at the FILE, not at a line. The cycle is a \
                     property of the import graph, so `line: 1` here is a placeholder rather than \
                     evidence — do not read it as the offending import. The file named is the \
                     alphabetically first member, chosen for determinism. 🔴 AND THE LIST ABOVE IS A \
                     MEMBERSHIP SET, NOT A PATH: it is every file in the cycle, sorted, capped at 25 in \
                     prose, and the commas are NOT import edges -- two names next to each other usually \
                     do not import each other. `data.cycle` carries the same members, uncapped. So the \
                     import you want is in one of those files and this finding does not tell you which \
                     pair closes the loop; grep the names against each other if you need the edge.",
                    chain_prose(&cycle),
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
    /// Moved once more on 2026-09-24 the anchor disclosure (review ledger V312): an uncontaminated
    /// auditor read this finding's `file:line` as evidence and found an unrelated import there, so the
    /// message now says the anchor is the FILE. That clause is deliberately LAST — it tells a reader how
    /// to read the citation, not what to do, and §27's order pin below owns the part that must come
    /// first. Updating this expectation is the point of the pin, not a workaround for it: the pin exists
    /// to make a message change deliberate and visible, and this one is both.
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
                "circular dependency: a.ts, b.ts — a change to any file in this cycle can ripple \
                 through every other member, making the group hard to reason about, test, or refactor \
                 in isolation. {CYCLE_EXTRACTION_LANDING} Break the cycle by extracting the shared \
                 pieces into a module both sides import, or invert one dependency direction (e.g. an \
                 interface/callback in place of a direct import). Disable via config `rules: {{ \
                 \"circular\": \"off\" }}` (embedders: `disabledRules`) if this cycle is an \
                 intentional, reviewed pattern (e.g. mutually recursive types re-exported through a \
                 barrel). WHERE THIS FINDING POINTS: at the FILE, not at a line. The cycle is a \
                 property of the import graph, so `line: 1` here is a placeholder rather than \
                 evidence — do not read it as the offending import. The file named is the \
                 alphabetically first member, chosen for determinism. 🔴 AND THE LIST ABOVE IS A \
                 MEMBERSHIP SET, NOT A PATH: it is every file in the cycle, sorted, capped at 25 in \
                 prose, and the commas are NOT import edges -- two names next to each other usually \
                 do not import each other. `data.cycle` carries the same members, uncapped. So the \
                 import you want is in one of those files and this finding does not tell you which \
                 pair closes the loop; grep the names against each other if you need the edge."
            )
        );
    }

    /// 📏 The defect this cap exists for, as a test rather than a memory: on `corpus/audit/nocodb` one
    /// cycle had 510 members and rendered a 32,522-char chain, making that single `info` finding 38.8%
    /// of the whole reply. Below the cap nothing changes (the test above is the witness); at the cap the
    /// prose stops and says what it withheld, and `data.cycle` is NOT the thing capped.
    #[test]
    fn a_long_cycle_is_capped_in_prose_and_the_message_says_where_the_rest_is() {
        let cycle: Vec<String> = (0..60).map(|i| format!("f{i:02}.ts")).collect();
        let out = circular_findings(&[cycle]);
        let m = &out[0].message;

        // 25 printed, 35 withheld, and the count is stated rather than implied.
        // 🔴 NO ARROWS, and this is the half that had nothing holding it. The separator used to be
        // ` -> `, which an outside auditor read as an import path and went looking for an edge that
        // does not exist (2026-10-09; `cycle.sort()` means adjacent names usually do not import each
        // other). Asserting a SPECIFIC adjacency rather than counting ", ", because the prose around
        // the list has its own commas and a count would pass on the wrong ones.
        assert!(
            !m.contains(" -> "),
            "no arrow may come back as a separator: {m}"
        );
        assert!(
            m.contains("f00.ts, f01.ts"),
            "members are comma-joined: {m}"
        );
        assert!(
            m.contains("MEMBERSHIP SET, NOT A PATH"),
            "the message must say the list is not a path, since the punctuation alone cannot: {m}"
        );
        assert!(
            m.contains("(+35 more members; the full cycle is in `data.cycle`, uncapped)"),
            "{m}"
        );
        assert!(m.contains("f24.ts"), "the 25th member is printed");
        assert!(!m.contains("f25.ts"), "the 26th is not");

        // The cap is on PROSE. The machine-readable cycle keeps every member.
        let data = out[0]
            .data
            .as_ref()
            .expect("circular always carries data.cycle");
        assert_eq!(data["cycle"].as_array().unwrap().len(), 60);
        assert_eq!(out[0].evidence_paths.len(), 59);
    }

    /// The boundary, because an off-by-one here is invisible: exactly at the cap nothing is withheld, so
    /// no reader is told about an elision that did not happen.
    #[test]
    fn a_cycle_exactly_at_the_cap_prints_whole_with_no_elision_note() {
        let cycle: Vec<String> = (0..25).map(|i| format!("f{i:02}.ts")).collect();
        let out = circular_findings(&[cycle]);
        assert!(
            !out[0].message.contains("more members"),
            "{}",
            out[0].message
        );
        assert!(out[0].message.contains("f24.ts"));
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
