//! The window's ORDER — the five keys that decide which findings a reader meets first, and nothing
//! else. Split out of the parent on 2026-09-05 on the seam the parent already had: `mod.rs` assembles
//! the reply (counts, folds, disclosures, caps) and this decides one list's sequence.
//!
//! Every key here is SHAPING ONLY, in both directions: nothing is dropped, no count moves, and every
//! finding is still in the same document whatever this returns. Two of the five ALSO announce
//! themselves on the wire (`testPaths`/`buildPaths`, written by the parent) because they demote, and a
//! demotion nobody is told about is a silent filter. The three diversity keys demote nothing — they
//! interleave — so there is nothing for them to disclose.
//!
//! None of the five is emitted. An index is not a wire field, which is the line `output-philosophy` §12
//! draws: what is forbidden is a scalar a caller can threshold on (`confidence > 0.8`), not the act of
//! ordering. Computed but not published stays out.

use super::deployment_role::deployment_role;
use super::filters::severity_rank;
use super::FindingFilters;

/// The filtered findings in window order. Returned as plain references because the keys exist only to
/// produce this sequence — no caller has ever needed one, and handing them out would be the first step
/// toward one reaching the wire.
///
/// `min_rank` is the caller's already-resolved severity floor (the parent owns the argument parsing);
/// `build_script_paths` is the analyzed tree's own manifest declaration of its build surface.
pub(super) fn window_order<'a>(
    findings: &'a [serde_json::Value],
    filters: &FindingFilters,
    min_rank: u8,
    build_script_paths: &std::collections::HashSet<&str>,
) -> Vec<&'a serde_json::Value> {
    // Round-robin counters, keyed by the BAND a finding lands in (deployment role + severity rank) plus
    // its rule id, and filled in ENGINE ORDER by the `map` below — so a finding's occurrence index is the
    // number of same-rule findings that preceded it inside its own band, and the whole key stays a pure
    // function of the pre-sort list. The band belongs in the key because the two outer keys have already
    // partitioned the list by the time this one is consulted: a rule that fires in two severity bands has
    // two independent turns, and a shared counter would let one band's traffic reorder another's.
    let mut seen_per_rule: std::collections::HashMap<(u8, u8, &str), u32> = Default::default();
    // The SAME round-robin idea one level deeper, keyed on (rule, FILE). See the fourth sort key below
    // for the measurement that made it necessary.
    let mut seen_per_rule_file: std::collections::HashMap<(u8, u8, &str, &str), u32> =
        Default::default();
    // The same idea again at the finest grain the wire carries, keyed on the SITE and on no rule at all.
    // See the fifth sort key below.
    let mut seen_per_site: std::collections::HashMap<(u8, u8, &str, i64), u32> = Default::default();
    let mut matching: Vec<(usize, &serde_json::Value, u8, u8, u32, u32, u32)> = findings
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            if severity_rank(sev) < min_rank {
                return false;
            }
            match &filters.rule {
                Some(rule) => f.get("ruleId").and_then(|v| v.as_str()) == Some(rule.as_str()),
                None => true,
            }
        })
        .map(|(i, f)| {
            let sev = f.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            let rank = severity_rank(sev);
            let role = deployment_role(f, build_script_paths);
            let rule = f.get("ruleId").and_then(|v| v.as_str()).unwrap_or("");
            let counter = seen_per_rule.entry((role, rank, rule)).or_insert(0);
            let occurrence = *counter;
            *counter += 1;
            let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let file_counter = seen_per_rule_file
                .entry((role, rank, rule, file))
                .or_insert(0);
            let dup_in_file = *file_counter;
            *file_counter += 1;
            // A finding with no file has no site, and findings that share the ABSENCE of one do not share
            // a place. They all take 0 and none of them is counted, which leaves this key a no-op on that
            // population rather than a claim that they collide. A finding with a file and no line keys on
            // -1: two file-level findings on one file really are the same destination for a reader.
            let dup_at_site = if file.is_empty() {
                0
            } else {
                let line = f.get("line").and_then(|v| v.as_i64()).unwrap_or(-1);
                let site_counter = seen_per_site.entry((role, rank, file, line)).or_insert(0);
                let seen = *site_counter;
                *site_counter += 1;
                seen
            };
            (i, f, rank, role, occurrence, dup_in_file, dup_at_site)
        })
        .collect();
    // DEPLOYMENT ROLE descending, THEN severity-desc within a role, original engine order as the stable
    // tiebreak — deterministic. See [`deployment_role`] for what the three roles are, why, and why role
    // is the OUTER key: a reader stops when the list stops paying, and the place they stop is the end of
    // the `critical` band, so a `critical` band made of fixtures and build scripts is a first screen that
    // found nothing. Shaping-only in both directions: nothing is dropped, counts never move, and BOTH
    // demotions announce themselves (`testPaths`/`buildPaths` below) — a demotion nobody is told about is
    // a silent filter.
    //
    // Roles are computed ONCE above rather than inside the comparator: the outer key is consulted on every
    // comparison, and it is two regex matches plus a set lookup.
    //
    // The THIRD key is RULE ROUND-ROBIN (2026-08-26): every rule's 1st finding, then every rule's 2nd, and
    // so on, with the engine index still breaking ties inside a round. Without it the tiebreak inside a
    // band was the engine index, which for a merged registry is the full path in lexicographic order — so
    // the first screen was never "the worst 40", it was "the alphabetically-first 40", and a rule whose
    // findings all sit under a late-sorting directory could not reach it at ANY count. Measured on
    // cal.com: `db/pagination-no-orderby` first appeared at rank 8 and `schema/fk-no-index` at rank 239,
    // same role, same severity, 231 places decided entirely by the letters in a path — and `apps/` (44
    // findings) filled the whole window while `packages/` (459) started at rank 48.
    //
    // Diversity, not importance: this key ranks by NOTHING about a finding except how many siblings its
    // own rule already spent, which is exactly why it can be computed here. It is deliberately CHEAP and
    // deliberately a proxy — the shipping form of "worst first" is a result-size feature, and this is the
    // step that opens the window far enough to see the material for one.
    //
    // Never emitted. The index is not a wire field, and that is the line `output-philosophy` §12 draws:
    // what is forbidden is a scalar a caller can threshold on (`confidence > 0.8`), not the act of
    // ordering — zzop already ordered by two keys before this one. Computed but not published stays out.
    // The FOURTH key, and it sits ABOVE the rule round-robin: how many findings of THIS RULE in THIS
    // FILE already came before. Every (rule, file) pair spends its first slot before any pair spends a
    // second, and the rule round-robin still orders the inside of each round.
    //
    // Added 2026-09-05 off a measurement that opened all 120 first-screen rows of three real projects
    // and judged each against the source. The rule round-robin above had already fixed "the window is
    // alphabetical", but the window was still not 40 pieces of information: on one project 14 OF THE 40
    // ROWS were the second or third instance of a judgment an earlier row had already delivered — the
    // same three-key dict in one Python config file taking three slots, the same package name in a
    // lockfile taking two, the same controller shape taking three, one deliberate route mirror taking
    // three, one email-template feature taking three. The reader meets "I saw this already" fourteen
    // times in a forty-row budget.
    //
    // Why it could not be the rule key alone: the duplication is INSIDE a rule, so a round-robin over
    // rules cannot see it. Why not key on file alone: one rule spread across twenty files would then
    // take twenty round-one slots and starve every other rule, which is the failure the rule key exists
    // to prevent. Both keys, rule-in-file first.
    //
    // Shaping only, exactly like the three keys above it: nothing is dropped, no count moves, every
    // finding is still in the same document. This key reorders; it never filters.
    //
    // The FIFTH key is the same question at the finest grain the wire carries and it is asked of NO RULE:
    // how many findings — any rule's — already pointed at THIS file:line. It leads the rule-in-file key
    // because it is the stronger form of the same reader question. "Have I already been sent here?" is
    // answered by the site; "have I already been told this?" is answered by the rule in the file; a
    // reader who has read a row is standing at its line whichever rule wrote it.
    //
    // Same 2026-09-05 measurement, its second shape: on one project two DIFFERENT rules fired on one
    // line of one file and took two of the forty slots — and their verdicts were opposite (one defect
    // worth fixing, one deliberate), so the pair was not even a corroboration. One line of source, two
    // slots, and the reader has to read both to learn that the second disagrees with the first.
    //
    // This key does not merge them and must not: two rules on one line are two findings, both true, both
    // counted, both in the reply. It only says the second one waits until every OTHER place in the tree
    // has had its turn — which is what makes the difference between them worth the slot when it comes.
    matching.sort_by(|a, b| {
        b.3.cmp(&a.3)
            .then(b.2.cmp(&a.2))
            .then(a.6.cmp(&b.6))
            .then(a.5.cmp(&b.5))
            .then(a.4.cmp(&b.4))
            .then(a.0.cmp(&b.0))
    });
    matching
        .into_iter()
        .map(|(_, f, _, _, _, _, _)| f)
        .collect()
}
