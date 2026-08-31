//! What a cap CUT, said out loud — the truncation objects both shaping lanes emit.
//!
//! Split from `output/mod.rs` when the findings lane's disclosure grew past a `json!` literal. Two
//! shapes live here because two things are true at once:
//!
//! * [`plain`] is the fixed-cap lane ([`super::shape_list`]): `{shown, totalMatching, hint}`, the
//!   caller's own hint echoed verbatim.
//! * [`findings`] is the findings lane, which knows something the other cannot — the rows it cut
//!   carry a SEVERITY, and the cut can take a whole severity band away.
//!
//! # Why the findings lane needs more than a count
//! Ordering is `deployment role desc -> severity desc` (see [`super::deployment_role`]), so every
//! finding on test or build surface sorts behind every shipped finding no matter what severity
//! either carries. That is deliberate and stays. Its residue, filed by the commit that landed it:
//! on a tree over the cap the demoted rows leave the reply entirely. Measured on cal.com at
//! `--limit 1000`: `bySeverity` said `critical: 6`, `shown` carried 1000 rows and not one of them
//! was `critical`, and no field anywhere said the two facts were about the same run. A reader who
//! trusts the list reads "no criticals here"; a reader who trusts the count cannot find them.
//! Neither is wrong about what they read, which is the definition of a reply that misleads.
//!
//! # The population is the set the CAP was applied to
//! Not the unfiltered census. A caller who passed `severity: "warning"` removed `info` THEMSELVES;
//! reporting `info` as "not shown" would name a silence the caller created, and a disclosure that
//! names the wrong cause teaches its reader to distrust the true ones. Restricting the population
//! to the post-filter set makes that self-silencing with no special-casing: under `--rule X` only
//! severities `X` fires at can ever be named, and a severity the filter ADMITTED and the cap then
//! took is still named, because that one really is news. `bySeverity` sits right above and remains
//! the unfiltered census either way.
//!
//! # Fully removed, not merely thinned
//! A severity with one row of six hundred in `shown` is visible to the reader; a severity with zero
//! is not. Only the second is claimed. Counting the partially-cut ones would be a bigger number
//! standing on a smaller fact.

use std::collections::{BTreeMap, HashSet};

use super::MAX_LIMIT;

/// `{shown, totalMatching, hint}` — the fixed-cap lane's disclosure, hint supplied by the caller.
pub(super) fn plain(shown: usize, total_matching: usize, hint: &str) -> serde_json::Value {
    serde_json::json!({
        "shown": shown,
        "totalMatching": total_matching,
        "hint": hint,
    })
}

/// The findings lane's disclosure. `ordered_severities` is the severity of every MATCHING finding in
/// final sort order, so its first `limit` entries are exactly what `shown` carries and the rest are
/// exactly what the cut took — the caller cannot hand this function a list and a `shown` that
/// disagree, because it derives both halves from the one list.
///
/// Only called when the cut actually bit (`ordered_severities.len() > limit`), which is why
/// `severitiesNotShown` never has to distinguish "nothing cut" from "nothing silenced".
pub(super) fn findings(ordered_severities: &[&str], limit: usize) -> serde_json::Value {
    let in_shown: HashSet<&str> = ordered_severities.iter().take(limit).copied().collect();
    // Only the CUT tail is counted, which is sound precisely because a severity qualifies here only
    // when `shown` holds none of it: every row it has is in this tail. BTreeMap, so the key order is
    // a pure function of the severities present (§6 determinism), not of iteration luck.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for sev in ordered_severities.iter().skip(limit) {
        if !in_shown.contains(sev) {
            *counts.entry(sev).or_default() += 1;
        }
    }

    // ALWAYS present, `{}` and all. `truncated`'s own presence is the additive gate — a tree under
    // the cap pays nothing for this axis — but once the reply admits it cut something, a MISSING
    // sub-key would make "the cut silenced no severity" and "this build does not compute that" the
    // same bytes. That collapse is the failure mode `output-philosophy` §1 names first.
    let mut out = plain(limit, ordered_severities.len(), &hint(limit));
    out["severitiesNotShown"] = serde_json::json!({
        "counts": counts,
        "meaning": "severities with at least one finding in the set this cap was applied to \
                    (after any `severity`/`rule` filter) and zero rows in `shown` — the cut took \
                    every one of them, so this list is invisible in `shown` and present in the \
                    counts above. Exact over that set. `bySeverity` above is the unfiltered census \
                    and never shrinks with a filter, so the two answer different questions. An \
                    empty object means the cut silenced no severity entirely; a severity merely \
                    thinned by the cut is not listed here.",
    });
    out
}

/// The remedy sentence, and it has to be TRUE at the value it is emitted for. `limit` above
/// [`MAX_LIMIT`] is a named usage error (`filters::parse_limit`), so at the ceiling "raise the
/// limit" is advice that exits 2 — the same inert-remedy defect `shape_list` was built to stop
/// telling callers about fixed-cap lists, which had a second home right here. At the ceiling the
/// remedies that still work are `severity` and `rule`, and narrowing by severity is what actually
/// reaches a band the cut removed.
///
/// The ceiling is INTERPOLATED from [`MAX_LIMIT`] rather than typed into the sentence. A hint that
/// quotes a number the code no longer enforces is the same class of lie as the inert remedy it
/// replaces, and the two owners would be one `const` edit apart.
fn hint(limit: usize) -> String {
    if limit >= MAX_LIMIT {
        format!(
            "narrow by severity or rule — `limit` is already at its maximum of {MAX_LIMIT} and a \
             larger value is a usage error, so the cap cannot be moved further"
        )
    } else {
        format!("narrow by severity or rule, or raise the limit (`limit` tops out at {MAX_LIMIT})")
    }
}
