//! `--fail-on <severity>` — the CI GATE, and the one thing this binary's exit code could not say.
//!
//! Until 2026-08-17 the exit code answered exactly one question: did zzop RUN. A tree with critical
//! findings exited 0, so every project wiring zzop into CI wrote its own gate — an external reviewer
//! did, and measured three traps in the process, two of which this flag removes outright: the exit code
//! not moving with severity, and a naive `shown`-list diff silently reading 50 of 382 findings because
//! the default `--limit` caps the LIST while the counts stay whole. (The third, a `--limit` ceiling of
//! 1000 that makes full enumeration impossible on a large repo, is untouched here — count-based gating
//! is exactly what this flag does, so it does not need the list.)
//!
//! # Why a THIRD exit code rather than 1
//! `1` already means "zzop could not answer" and `2` means "you called it wrong". Reusing `1` for
//! "zzop answered, and the answer is bad" would make a broken config and a real critical finding
//! indistinguishable in a CI log — the same conflation this whole product refuses elsewhere. So a
//! threshold hit exits [`FAIL_ON_EXIT`], stdout still carries the whole reply (the gate composes with a
//! pipeline instead of replacing it), and stderr carries one line naming the counts and the threshold.
//!
//! # Why it reads the COUNTS
//! `findings.bySeverity` is over every finding this run produced, never the filtered `shown` list, so a
//! `--fail-on` gate and a `--limit`/`--severity` view can be asked in the same invocation without the
//! view narrowing the gate.

/// Exit code for "the run succeeded and the findings met the declared threshold". Distinct from `1`
/// (zzop failed to answer) and `2` (usage) on purpose — see the module doc.
pub const FAIL_ON_EXIT: i32 = 3;

/// Lifts `--fail-on <severity>` out of argv. Returns `(argv with the flag and its value removed,
/// threshold)`. A missing or unrecognized value exits 2 with `usage` — never a silently ignored gate,
/// which would report success on every run and be discovered only by the bug it failed to catch.
pub fn extract_fail_on(args: &[String], usage: &str) -> (Vec<String>, Option<String>) {
    let mut threshold = None;
    let mut rest: Vec<String> = args[..2.min(args.len())].to_vec();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--fail-on" {
            let Some(value) = args.get(i + 1).filter(|v| !v.starts_with('-')) else {
                eprintln!("{usage} (--fail-on needs a severity: critical, warning or info)");
                std::process::exit(2);
            };
            // Ranked by the SHARED function the findings view filters with — never a second ordering
            // in this binary, which would answer "is warning above info" twice and drift the day a
            // severity is added. Rank 0 is that function's "not a severity I can name".
            if zzop_summary::severity_rank(value) == 0 {
                eprintln!("{usage} (--fail-on takes critical, warning or info; got {value:?})");
                std::process::exit(2);
            }
            threshold = Some(value.clone());
            i += 2;
            continue;
        }
        rest.push(args[i].clone());
        i += 1;
    }
    (rest, threshold)
}

/// Refuses `--fail-on` on a lane whose reply carries no per-severity census of the findings being
/// gated, naming what is missing. REFUSED rather than ignored, the same stance `graph --fold` takes on
/// the domains it cannot mean anything for: a gate that accepts its flag and never fires is worse than
/// no gate, because a green build then proves nothing and says it proved something.
///
/// `cross` is that lane today. Its reply carries `crossLayerFindings.bySeverity` (the JOIN's own
/// findings) and, per tree, a bare `findingCount` with no severity breakdown — so a gate here would
/// silently cover the cross-layer half alone. Gate the per-tree half with `zzop analyze --fail-on` on
/// each tree, which reads the census that actually exists.
pub fn refuse_fail_on(lane: &str, per_lane_reason: &str) -> ! {
    eprintln!(
        "zzop: `{lane} --fail-on` is refused rather than ignored: {per_lane_reason} A gate that \
         accepts the flag and can never fire would make a green build mean nothing while looking like \
         it meant something."
    );
    std::process::exit(2);
}

/// Refuses a `--rule` filter this run can be PROVEN not to have been able to match, on stderr and with
/// exit 2. Silent otherwise, including whenever it cannot be certain.
///
/// # The defect
/// `zzop analyze <tree> --rule totally/bogus-rule` exited 0 with an EMPTY stderr and `shown: 0` — in a
/// CI log, byte-indistinguishable from "nothing to report". The reply's `warnings` array did carry an
/// explanation (`zzop_summary`'s own unknown-rule-filter channel), as one entry among fifteen in a JSON
/// document nobody opens on a green build. A disclosure only a reader who already suspects the problem
/// will find is not a disclosure; the exit code is what CI reads.
///
/// # Why the test is `packsLoaded` and not the warnings text
/// Sniffing the shared crate's prose for a substring would make this binary's exit code depend on a
/// sentence's wording. The FACT behind that sentence is in the reply as structured data: a qualified id
/// whose pack is absent from `packsLoaded` could not have produced a finding this run, whatever the tree
/// contains. Bare ids never reach here — `super::args::resolve_rule_filter` has already expanded a bare
/// DSL id to its full form or refused it at argv time.
///
/// # The second half: a typo INSIDE a loaded pack (fixed 2026-08-20)
/// This check tested only the pack prefix, so `--rule security/no-such-rule` exited 0 with an empty
/// stderr and `shown: 0` — the silent empty result `zzop analyze --help` promises can never happen,
/// and byte-identical in a CI log to "that rule is clean". It stayed one-sided because nothing in the
/// reply enumerated a loaded pack's rule ids: `packsLoaded[].rules` was a COUNT, and validating
/// against the catalog compiled into this binary instead would falsely refuse a rule from a user pack
/// loaded out of `<tree>/zzop/rules/` — the very bug fixed for native ids the same morning. The run now
/// publishes the ids it could report (`packsLoaded[].ruleIds`, whose engine-side doc weighs the size),
/// so the refusal is decided by the packs that ACTUALLY loaded and is right for both pack sources.
///
/// Still one-sided where it must be: a reply carrying no `packsLoaded` at all, or a pack entry with no
/// `ruleIds` key (an older/edge shape), is left alone rather than turned into a false refusal. A
/// refusal without evidence is the same defect in the other direction.
fn refuse_unmatched_rule_filter(reply: &serde_json::Value, rule: &str) {
    let Some((pack, name)) = rule.split_once('/') else {
        return;
    };
    // The `<namespace>/<name>` shape belongs to TWO id spaces, and `packsLoaded` enumerates only one.
    // The NATIVE analyses are namespaced the same way (`schema/god-model`, `cross-layer/route-near-miss`)
    // and are compiled in rather than loaded from a pack, so they can never appear there — reading the
    // prefix as a pack id alone refused a filter that had just matched, on a run whose own `shown` held
    // the finding. Consulting the native registry is what tells "this id is not from a pack" apart from
    // "this id is from a pack that did not load".
    if zzop_summary::native_analysis_ids()
        .iter()
        .any(|id| id == rule)
    {
        return;
    }
    let Some(loaded) = reply["packsLoaded"].as_array().filter(|p| !p.is_empty()) else {
        return;
    };
    if let Some(entry) = loaded.iter().find(|p| p["id"].as_str() == Some(pack)) {
        // The pack loaded. Whether the RULE exists inside it is answerable only from the ids that pack
        // published; a missing `ruleIds` is "no data", never "no such rule".
        let Some(ids) = entry["ruleIds"].as_array() else {
            return;
        };
        if ids.iter().any(|id| id.as_str() == Some(name)) {
            return;
        }
        eprintln!(
            "zzop: --rule {rule:?} matched nothing because pack {pack:?} loaded in this run and \
             carries no rule {name:?} — this reply's `shown` is the filter, not a clean result. The \
             pack's own `ruleIds` in the reply on stdout lists every id it could have reported; \
             `zzop explain <id>` prints one rule's data."
        );
        std::process::exit(2);
    }
    eprintln!(
        "zzop: --rule {rule:?} matched nothing because no pack {pack:?} was loaded in this run — this \
         reply's `shown` is the filter, not a clean result. Two readings needing different fixes: the id \
         may be misspelled, or its pack may be one this build SHIPS BUT DOES NOT LOAD (an exported pack \
         is a real id, not a typo — retrieve it with `zzop contract example-pack-<stem>` and save it \
         under <tree>/zzop/rules/). `packsLoaded` in the reply on stdout names the packs that DID load."
    );
    std::process::exit(2);
}

/// Applies the threshold to a shaped analyze-style reply: exits [`FAIL_ON_EXIT`] when any finding sits
/// at or above `threshold`, after printing the reply. `None` threshold is the pre-existing behavior,
/// byte for byte.
///
/// Reads `findings.bySeverity` — the FULL census, never `shown` (see the module doc). A reply with no
/// such object cannot be gated, and that is a refusal rather than a pass: silently exiting 0 there is
/// the "green build proves nothing" failure this flag exists to end.
pub fn gate_or_exit(text: &str, threshold: Option<&str>, rule: Option<&str>) -> ! {
    println!("{text}");
    if threshold.is_none() && rule.is_none() {
        std::process::exit(0);
    }
    let reply: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("zzop: could not read this run's own reply as JSON ({e}) — refusing to report a pass it did not verify.");
            std::process::exit(1);
        }
    };
    // BEFORE the gate, on purpose. A `--rule` id nothing could have matched makes the printed view a
    // lie, and a threshold applied to a lie is worse than no threshold: `2` (you called it wrong)
    // therefore outranks `FAIL_ON_EXIT`. See `refuse_unmatched_rule_filter`.
    if let Some(rule) = rule {
        refuse_unmatched_rule_filter(&reply, rule);
    }
    let Some(threshold) = threshold else {
        std::process::exit(0);
    };
    let Some(by_severity) = reply
        .pointer("/findings/bySeverity")
        .and_then(|v| v.as_object())
    else {
        eprintln!("zzop: --fail-on found no `findings.bySeverity` census in this reply — refusing to report a pass it did not verify.");
        std::process::exit(1);
    };
    let rank = zzop_summary::severity_rank(threshold);
    let hits: u64 = by_severity
        .iter()
        .filter(|(sev, _)| zzop_summary::severity_rank(sev) >= rank)
        .filter_map(|(_, n)| n.as_u64())
        .sum();
    if hits == 0 {
        std::process::exit(0);
    }
    // The counts by name, not just the total: a gate that says "3 findings" sends the reader back to
    // the reply to find out which three mattered, and a CI log is usually all they have.
    let breakdown: Vec<String> = by_severity
        .iter()
        .filter(|(sev, _)| zzop_summary::severity_rank(sev) >= rank)
        .map(|(sev, n)| format!("{n} {sev}"))
        .collect();
    eprintln!(
        "zzop: --fail-on {threshold} matched {hits} finding(s) ({}). The full reply is on stdout; \
         counts there cover every finding, while `shown` is a capped view.",
        breakdown.join(", ")
    );
    std::process::exit(FAIL_ON_EXIT);
}
