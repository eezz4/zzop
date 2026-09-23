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
//!
//! # Why a count was not enough — `firstOmitted`
//! The count closed "the reply never says the band left". It did not close "the reader cannot see
//! WHAT left", and the residue is measurable at the exit code: `zzop analyze --config <cal.com>
//! --limit 1000 --fail-on critical` exits 3 naming `6 critical`, and no file, line or rule id of
//! those six appears anywhere in the reply that same command printed. A build breaks and the
//! artifact it broke on carries no evidence; the reader has to already know to re-run with
//! `--severity critical`. A disclosure only a reader who already suspects the problem can act on is
//! the shape this crate refuses everywhere else.
//!
//! So each silenced severity also carries its first few rows as ANCHORS — `ruleId`/`file`/`line`,
//! no prose, because prose is what the cap was defending against. Three properties are why this was
//! the cheap close rather than a change to the cut itself:
//!
//! * **Ordering is untouched.** `shown` is still exactly the first `limit` of the sorted list. The
//!   other candidate on the table — let the cap preserve the gating severity — makes the cap a sort
//!   key, and then `output-philosophy` §6 (one config, byte-identical output) has to be re-argued.
//! * **The gate stays count-driven.** `--fail-on` still reads `bySeverity`, so a view knob cannot
//!   narrow it (§19). This puts evidence in the reply; it does not let the reply move the verdict.
//! * **It answers for every severity, not the one someone happened to gate on.** No flag reaches
//!   this function and none should: a reply that says different things depending on whether a CI
//!   gate was armed is two replies.
//!
//! # Why a finding count was not enough either — `ruleCounts`
//! `counts` answers "how many rows left". It does not answer "how many RULES left", and those two
//! numbers come apart badly at the band boundary. Measured on `cases/trees/api-be` (2026-09-03,
//! after 27 rules moved `warning` -> `info`): the cut silenced 30 `info` rows, and those rows
//! belong to NINETEEN distinct rules. A reader is handed `30` and three anchors naming three rule
//! ids, and nothing tells them the other sixteen rules exist at all. Thirty rows reads as a few
//! noisy rules; nineteen rules is a different fact, and the rule is the unit a reader acts on --
//! `--rule X`, a `rules` entry, a suppression marker are all keyed by it.
//!
//! It is exact, not a sample, and it costs one integer per silenced severity off the SAME walk
//! that fills `counts` and `firstOmitted` -- so the three can never describe different sets.
//!
//! Bounded on purpose. This module exists to keep one reply bounded, and an unbounded anchor list
//! for `info` under `--limit 0` would hand the token bomb back through the disclosure door.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::MAX_LIMIT;

/// `{shown, totalMatching, hint}` — the fixed-cap lane's disclosure, hint supplied by the caller.
pub(super) fn plain(shown: usize, total_matching: usize, hint: &str) -> serde_json::Value {
    serde_json::json!({
        "shown": shown,
        "totalMatching": total_matching,
        "hint": hint,
    })
}

/// How many rows of a silenced severity ride along as anchors. Three, not one: one anchor from a
/// six-row band reads as "here is the finding" rather than "here is where the band starts", and a
/// reader who sees three sites in three different files learns something one site cannot say. Not
/// unbounded, for the reason in the module doc. `counts` beside it is always exact, so the list
/// being a sample is stated rather than implied.
const FIRST_OMITTED_ANCHORS: usize = 3;

/// The `ruleId`/`file`/`line` of one finding — enough to open the file, and nothing else. No
/// `message`: the message is the biggest field a finding carries and folding it was worth its own
/// module, so re-admitting it through the disclosure would undo the cap this object is explaining.
///
/// A key the finding does not carry is OMITTED rather than emitted as `null`, the same contract the
/// keys around it keep: `"line": null` says "this finding has no line", which is a different claim
/// from "this shape did not have one to give".
fn anchor(f: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for key in ["ruleId", "file", "line"] {
        if let Some(v) = f.get(key) {
            if !v.is_null() {
                out.insert(key.to_string(), v.clone());
            }
        }
    }
    serde_json::Value::Object(out)
}

/// `severity` as the sort and the census read it — one spelling of the fallback, so an anchor can
/// never be filed under a different key than the count that describes it.
fn severity_of(f: &serde_json::Value) -> &str {
    f.get("severity").and_then(|v| v.as_str()).unwrap_or("")
}

/// The findings lane's disclosure. `ordered` is every MATCHING finding in final sort order, so its
/// first `limit` entries are exactly what `shown` carries and the rest are exactly what the cut
/// took — the caller cannot hand this function a list and a `shown` that disagree, because it
/// derives every half from the one list.
///
/// Only called when the cut actually bit (`ordered.len() > limit`), which is why
/// `severitiesNotShown` never has to distinguish "nothing cut" from "nothing silenced".
pub(super) fn findings(ordered: &[&serde_json::Value], limit: usize) -> serde_json::Value {
    let in_shown: HashSet<&str> = ordered.iter().take(limit).map(|f| severity_of(f)).collect();
    // Only the CUT tail is counted, which is sound precisely because a severity qualifies here only
    // when `shown` holds none of it: every row it has is in this tail. BTreeMap, so the key order is
    // a pure function of the severities present (§6 determinism), not of iteration luck. The anchors
    // are filled from the SAME walk in the same order, so `firstOmitted[s]` is the first rows of
    // exactly the set `counts[s]` counted — two views of one traversal, never two traversals.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    // Distinct rule ids per silenced severity -- see "Why a finding count was not enough either"
    // above. A BTreeSet so the answer is a pure function of the set, and filled in the same pass so
    // it can never count a different tail than `counts` did.
    let mut rule_ids: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut first_omitted: BTreeMap<&str, Vec<serde_json::Value>> = BTreeMap::new();
    for f in ordered.iter().skip(limit) {
        let sev = severity_of(f);
        if in_shown.contains(sev) {
            continue;
        }
        *counts.entry(sev).or_default() += 1;
        if let Some(id) = f.get("ruleId").and_then(|v| v.as_str()) {
            rule_ids.entry(sev).or_default().insert(id);
        }
        let anchors = first_omitted.entry(sev).or_default();
        if anchors.len() < FIRST_OMITTED_ANCHORS {
            anchors.push(anchor(f));
        }
    }

    // ALWAYS present, `{}` and all. `truncated`'s own presence is the additive gate — a tree under
    // the cap pays nothing for this axis — but once the reply admits it cut something, a MISSING
    // sub-key would make "the cut silenced no severity" and "this build does not compute that" the
    // same bytes. That collapse is the failure mode `output-philosophy` §1 names first.
    let mut out = plain(limit, ordered.len(), &hint(limit));
    out["severitiesNotShown"] = serde_json::json!({
        "counts": counts,
        // Emitted from the same `counts` key order, so a severity present in one is present in the
        // other. A finding carrying no `ruleId` contributes to `counts` and not here, which is why
        // `ruleCounts[s] <= counts[s]` is the only relation between them worth relying on.
        "ruleCounts": rule_ids.iter().map(|(k, v)| (*k, v.len())).collect::<BTreeMap<&str, usize>>(),
        "firstOmitted": first_omitted,
        // The `firstOmitted` cap is INTERPOLATED from the constant for the same reason `hint`
        // interpolates `MAX_LIMIT`: a sentence quoting a number the code no longer uses is the same
        // class of lie as an inert remedy, and the two owners would be one `const` edit apart.
        "meaning": format!(
            "severities with at least one finding in the set this cap was applied to (after any \
             `severity`/`rule` filter) and zero rows in `shown` — the cut took every one of them, \
             so this list is invisible in `shown` and present in the counts above. Exact over that \
             set. `bySeverity` above is the unfiltered census and never shrinks with a filter, so \
             the two answer different questions. An empty object means the cut silenced no \
             severity entirely; a severity merely thinned by the cut is not listed here. \
             `ruleCounts` gives, for each, how many DISTINCT RULES those rows came from -- exact, \
             and a different fact from the row count beside it: a band of thirty rows drawn from \
             nineteen rules is not a few noisy rules. \
             `firstOmitted` carries, for each of those severities, its first cut rows as \
             `ruleId`/`file`/`line` in the same order `shown` uses — so a run whose exit code \
             turned on a severity nothing in `shown` carries still names sites the reader can \
             open. It is a SAMPLE, at most {FIRST_OMITTED_ANCHORS} rows per severity; `counts` is \
             the exact number and narrowing by that severity reaches every one of them."
        ),
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
