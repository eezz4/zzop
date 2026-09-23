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
//! pipeline instead of replacing it), and stderr names the counts and the threshold — plus, when the
//! reply's own list carries none of the rows it fired on, WHERE those rows are
//! ([`name_the_evidence`]).
//!
//! # Why it reads the COUNTS
//! `findings.bySeverity` is over every finding this run produced, never the filtered `shown` list, so a
//! `--fail-on` gate and a `--limit`/`--severity` view can be asked in the same invocation without the
//! view narrowing the gate. That stays exactly as it is: [`name_the_evidence`] puts sites in the
//! message, never a term in the verdict.

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
            // A repeated `--fail-on` used to keep the LAST threshold silently, which on this flag
            // decides an exit code: `--fail-on critical --fail-on info` gated on `info` while the log
            // recorded a request to gate on `critical`. Refused for the same reason the missing value
            // above is — see `super::args::refuse_repeated_flag`.
            super::args::refuse_repeated_flag(threshold.is_some(), "--fail-on", usage);
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
/// # Why this derives NOTHING itself (2026-09-02)
/// It used to walk `packsLoaded` here — native-id carve-out, pack lookup, `ruleIds` membership, two
/// hand-written refusal sentences — and `zzop_summary::unmatchable_rule_filter` walked the identical
/// structure for the MCP host, which turns the same verdict into `isError`. Two owners of one judgment
/// behind two hosts' failure signals: the day one is corrected is the day they disagree, and the
/// disagreement is invisible from either side. This now asks the shared function and only decides what
/// a TERMINAL does with the answer, which is this file's actual job.
///
/// The two derivations were measured against each other before folding, on the three shapes plus a
/// positive control (`tests/cli.rs`'s `a_rule_filter_that_can_never_match_is_loud_on_stderr_and_exits_two`
/// pins all four): a bare id no run could report, a typo inside a pack that DID load, a real id from a pack
/// this build ships but does not load, and a real loaded id. Both said the same thing on all four. The
/// one structural difference was this side's extra `!packs.is_empty()` guard, which is unreachable —
/// `packsLoaded` is emitted by the same build that reads it here and always carries an entry per
/// bundled pack, including packs a config gated off (`didNotRun`, with their `ruleIds` intact).
///
/// # The second half: a typo INSIDE a loaded pack (fixed 2026-08-20)
/// The check tested only the pack prefix, so `--rule security/no-such-rule` exited 0 with an empty
/// stderr and `shown: 0` — the silent empty result `zzop analyze --help` promises can never happen,
/// and byte-identical in a CI log to "that rule is clean". It stayed one-sided because nothing in the
/// reply enumerated a loaded pack's rule ids: `packsLoaded[].rules` was a COUNT, and validating
/// against the catalog compiled into this binary instead would falsely refuse a rule from a user pack
/// loaded out of `<tree>/zzop/rules/` — the very bug fixed for native ids the same morning. The run now
/// publishes the ids it could report (`packsLoaded[].ruleIds`, whose engine-side doc weighs the size),
/// so the refusal is decided by the packs that ACTUALLY loaded and is right for both pack sources.
///
/// Still one-sided where it must be, and that stays the shared function's contract rather than a second
/// copy of it here: a pack entry with no `ruleIds` key (an older/edge shape) is left alone rather than
/// turned into a false refusal. A refusal without evidence is the same defect in the other direction.
fn refuse_unmatched_rule_filter(reply: &str, rule: &str) {
    let Some(reason) = zzop_summary::unmatchable_rule_filter(reply, rule) else {
        return;
    };
    // The shared sentence is written for a READER of either host and names both dialects of its
    // retrieval route; the only thing added here is that on this host the verdict is an exit code.
    eprintln!("zzop: {reason}");
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
        refuse_unmatched_rule_filter(text, rule);
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
    name_the_evidence(&reply, rank);
    std::process::exit(FAIL_ON_EXIT);
}

/// The line above says a build should break and how many findings say so. This one says WHICH, and
/// it exists because on a large tree the answer was nowhere at all.
///
/// # The measurement
/// `zzop analyze --config <cal.com> --limit 1000 --fail-on critical` exits 3 naming `6 critical`,
/// and the reply it had just printed carried 1000 rows of which NONE was critical: ordering is
/// deployment role first, so criticals sitting on fixtures and release scripts sort behind every
/// shipped finding and the cap takes the whole band. The counts were right and the list was right,
/// and between them the reader had a red pipeline and no site to open.
/// `truncated.severitiesNotShown.firstOmitted` now carries the first rows of any band the cut
/// removed outright, which is what makes this printable from the reply the gate already parsed — no
/// second run, no second ordering, and the gate still decides on the counts alone.
///
/// # Three states, and the third is why this is not one `if`
/// For every severity the gate counted, the reply either shows it, names it as cut, or does
/// neither — and the third is real: `--rule X --fail-on critical` gates on the WHOLE census while
/// the view holds one rule, so a critical from another rule is in neither place. Saying nothing
/// there would leave exactly the silence this function was added to end, one filter further along.
///
/// Silent when every gated severity is visible in `shown`: a reader who can already see a row does
/// not need to be told where it is.
fn name_the_evidence(reply: &serde_json::Value, rank: u8) {
    let shown_severities: std::collections::HashSet<&str> = reply
        .pointer("/findings/shown")
        .and_then(|v| v.as_array())
        .map(|rows| rows.iter().filter_map(|f| f["severity"].as_str()).collect())
        .unwrap_or_default();
    let cut = reply.pointer("/findings/truncated/severitiesNotShown/firstOmitted");
    let census = reply
        .pointer("/findings/bySeverity")
        .and_then(|v| v.as_object());

    let mut lines: Vec<String> = Vec::new();
    for (severity, count) in census.into_iter().flatten() {
        if zzop_summary::severity_rank(severity) < rank
            || count.as_u64().unwrap_or(0) == 0
            || shown_severities.contains(severity.as_str())
        {
            continue;
        }
        let sites: Vec<String> = cut
            .and_then(|c| c.get(severity))
            .and_then(|v| v.as_array())
            .map(|rows| rows.iter().map(site).collect())
            .unwrap_or_default();
        if sites.is_empty() {
            // In `shown`: no. Named as cut: no. So a `--rule`/`--severity` narrowed the VIEW while
            // the gate read the whole census. Naming the cause is the difference between "the tool
            // lost my findings" and "my filter did".
            lines.push(format!(
                "  {severity}: no row of this severity is in this reply's view — a `--rule`/`--severity` \
                 filter kept it out while the gate read the whole census. Re-run with `--severity {severity}`."
            ));
        } else {
            // The cause belongs on the LINE, not in the header: the two branches have different
            // ones, and a header that explained the cap would be describing the wrong thing on
            // every run where a filter was what hid the rows.
            lines.push(format!(
                "  {severity}: {} — from `findings.truncated.severitiesNotShown.firstOmitted` \
                 (ordering puts test and build surface behind shipped code, so the cap took the \
                 whole band)",
                sites.join(", ")
            ));
        }
    }
    if lines.is_empty() {
        return;
    }
    eprintln!(
        "zzop: `shown` in that reply carries no row of the severity this fired on, so the artifact \
         you just captured does not show what broke. Where those rows are:\n{}",
        lines.join("\n")
    );
}

/// `<ruleId> <file>:<line>` from one anchor, degrading key by key — an anchor omits what the
/// finding did not carry, so a missing `line` has to shorten the text rather than print `:null`.
fn site(anchor: &serde_json::Value) -> String {
    let rule = anchor["ruleId"].as_str().unwrap_or("<unnamed rule>");
    match (anchor["file"].as_str(), anchor["line"].as_u64()) {
        (Some(file), Some(line)) => format!("{rule} {file}:{line}"),
        (Some(file), None) => format!("{rule} {file}"),
        (None, _) => rule.to_string(),
    }
}
