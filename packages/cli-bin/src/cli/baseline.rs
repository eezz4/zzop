//! `--baseline <file>` — the RATCHET gate: accept what a repository already has, fail on what it adds.
//!
//! # The adoption problem this exists for, measured
//!
//! External review round 20 ran zzop over twelve real third-party repositories and read every
//! `critical` as that code's owner would. **101 of 101 were findings the owner would dismiss** — test
//! fixtures, test certificates, a cache's own documented `clear()`, one EC key repeated 72 times across
//! grafana's `test-data/receiver-exports/`. Three popular repositories (requests, gin, typeorm) exit
//! [`FAIL_ON_EXIT`] on `--fail-on critical` the first time anyone runs them.
//!
//! The only escapes were `rules: { "<id>": "off" }` and `exclude`, and both kill the real case with the
//! false one: turning off `private-key-committed` because a test cert tripped it is how a committed
//! production key ships unnoticed later. A team cannot adopt a gate whose only tuning is blindness.
//!
//! This is the shape every lint has for that: record what is there, gate on what is new.
//!
//! # Why the key is the RULE and not `(rule, file, line)`
//!
//! A baseline keyed on a line number churns on every edit above it and teaches people to regenerate
//! it, which is the same as not having one. `(rule, file)` survives line moves but needs the whole
//! findings list, and the reply's `shown` is capped (`DEFAULT_FINDINGS_LIMIT`) — building a baseline
//! from it would silently record only the first fifty.
//!
//! `findings.byRule` is a census over the WHOLE run, present in every reply, uncapped. So the unit is
//! one rule, and the recorded value is its count.
//!
//! **What that cannot see, said out loud**: a finding that moves from one file to another under the
//! same rule, and a fix-plus-regression that leaves the count unchanged. This is the classic ratchet
//! limit and it is not repaired by pretending otherwise — a count that goes UP is always reported, which
//! is the property CI actually needs.
//!
//! # Why `--baseline` and `--fail-on` are refused together
//!
//! They answer different questions — "is anything above this band present" versus "is anything new" —
//! and there is one exit code between them. Silently letting one win is the half-option this repo
//! forbids (`output-philosophy.md` §7), and the reply carries no per-rule severity for the combined
//! form to be built honestly out of: `byRule` holds counts, `bySeverity` holds bands, and nothing joins
//! them for the rules that did not make the capped `shown` window. So the pair is a usage error that
//! names both and asks which one was meant.

use std::collections::BTreeMap;
use std::path::Path;

use super::fail_on::FAIL_ON_EXIT;

// The on-disk shape is built and read with `serde_json` directly rather than a derive: this binary does
// not depend on `serde` itself, and `check-dep-closure.sh` polices that closure. Four fields do not earn
// a new dependency edge.
//
// Key order in the written file is ALPHABETICAL, not the order below: `serde_json` serializes an object
// through a BTreeMap unless the workspace turns on `preserve_order`, which is not a feature one CLI file
// gets to flip. So `byRule` leads and `meaning` sits third. That is fine and is said here rather than
// wished away — the file is pretty-printed, `meaning` is four lines down, and a reader who opens it finds
// the sentence without being told where to look.

const MEANING: &str = "A zzop findings baseline: what this tree already had when it was recorded. \
`zzop analyze <path> --baseline <this file>` compares a fresh run against `byRule` and exits 3 when \
any rule finds MORE than the count here — new problems fail, existing ones do not. Counts are per RULE \
over the whole run, not per file and not per line, so this file survives edits that move code around; \
what it cannot see is a finding moving between files under one rule, or a fix and a regression that \
cancel out. Delete the file and re-run to re-record. Commit it.";

/// Pulls `--baseline <path>` out of the argument list, mirroring `extract_fail_on`'s contract exactly:
/// a missing value and a repeated flag are both usage errors, because this flag decides an exit code.
pub fn extract_baseline(args: &[String], usage: &str) -> (Vec<String>, Option<String>) {
    let mut path: Option<String> = None;
    let mut rest: Vec<String> = args[..2.min(args.len())].to_vec();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--baseline" {
            let Some(value) = args.get(i + 1).filter(|v| !v.starts_with('-')) else {
                eprintln!("{usage} (--baseline needs a file path)");
                std::process::exit(2);
            };
            super::args::refuse_repeated_flag(path.is_some(), "--baseline", usage);
            path = Some(value.clone());
            i += 2;
            continue;
        }
        rest.push(args[i].clone());
        i += 1;
    }
    (rest, path)
}

/// The pair refusal. Called before either gate runs, so neither can half-apply.
pub fn refuse_baseline_with_fail_on(baseline: Option<&str>, fail_on: Option<&str>, usage: &str) {
    if baseline.is_some() && fail_on.is_some() {
        eprintln!(
            "{usage}\n  --baseline and --fail-on are two different gates and there is one exit code \
             between them: --fail-on asks \"is anything at or above this band present\", --baseline \
             asks \"is anything NEW\". Pick one. (A baselined run reports every rule that grew, in \
             every band — if what you want is \"fail on new criticals only\", that is not built.)"
        );
        std::process::exit(2);
    }
}

/// A UTC day stamp for the `recorded` field. Day granularity on purpose: this is provenance for a
/// human reading a committed file, not an ordering key, and a second-resolution timestamp would make
/// every re-record a noisier diff than the counts it is there to explain.
fn recorded_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    // Civil-from-days (Howard Hinnant's algorithm), so this needs no date crate for one field.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Reads `findings.byRule` and `findings.total` out of a shaped reply.
fn census(reply: &serde_json::Value) -> Option<(u64, BTreeMap<String, u64>)> {
    let total = reply.pointer("/findings/total")?.as_u64()?;
    let by_rule = reply.pointer("/findings/byRule")?.as_object()?;
    let mut map = BTreeMap::new();
    for (rule, count) in by_rule {
        map.insert(rule.clone(), count.as_u64()?);
    }
    Some((total, map))
}

/// Writes the baseline, or gates against it. Exits; never returns.
///
/// Absent file = RECORD. That is the whole first-run experience: one command, no editing, and the file
/// says what it is. An absent file is deliberately not an error — requiring a separate `--write-baseline`
/// verb would make the common path two commands and the second one forgettable.
pub fn gate_against_baseline(text: &str, path: &str) -> ! {
    println!("{text}");
    let reply: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "zzop: could not read this run's own reply as JSON ({e}) — refusing to record or \
                 gate against a baseline it did not verify."
            );
            std::process::exit(1);
        }
    };
    let Some((total, by_rule)) = census(&reply) else {
        eprintln!(
            "zzop: --baseline found no `findings.byRule` census in this reply — refusing to report a \
             pass it did not verify."
        );
        std::process::exit(1);
    };

    if !Path::new(path).exists() {
        let mut rules = serde_json::Map::new();
        for (rule, count) in &by_rule {
            rules.insert(rule.clone(), serde_json::json!(count));
        }
        let baseline = serde_json::json!({
            "meaning": MEANING,
            "recorded": recorded_stamp(),
            "zzopVersion": zzop_summary::version(),
            "total": total,
            "byRule": serde_json::Value::Object(rules),
        });
        let body = match serde_json::to_string_pretty(&baseline) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("zzop: could not serialize the baseline ({e}).");
                std::process::exit(1);
            }
        };
        if let Err(e) = std::fs::write(path, format!("{body}\n")) {
            eprintln!("zzop: could not write the baseline to {path}: {e}");
            std::process::exit(1);
        }
        eprintln!(
            "zzop: recorded a baseline at {path} — {total} finding(s) across {} rule(s). This run \
             passes by definition. Commit that file; the next run fails only on what it adds.",
            by_rule.len()
        );
        std::process::exit(0);
    }

    let parsed: serde_json::Value = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_str(&b).map_err(|e| e.to_string()))
    {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "zzop: could not read the baseline at {path} ({e}) — refusing to report a pass it \
                 did not verify. Delete the file to re-record."
            );
            std::process::exit(1);
        }
    };
    // A file that parses as JSON but carries no `byRule` is not a baseline, and treating its absent
    // map as "everything was zero" would fail the run on every rule while looking like a real verdict.
    let Some(recorded_rules) = parsed.get("byRule").and_then(|v| v.as_object()) else {
        eprintln!(
            "zzop: {path} has no `byRule` map — that is not a zzop baseline. Delete it and re-run to \
             record one."
        );
        std::process::exit(1);
    };
    let mut recorded: BTreeMap<String, u64> = BTreeMap::new();
    for (rule, count) in recorded_rules {
        recorded.insert(rule.clone(), count.as_u64().unwrap_or(0));
    }

    // Every rule that grew, plus every rule that is new since the recording (absent == 0).
    let mut grew: Vec<(String, u64, u64)> = Vec::new();
    for (rule, now) in &by_rule {
        let was = recorded.get(rule).copied().unwrap_or(0);
        if *now > was {
            grew.push((rule.clone(), was, *now));
        }
    }
    if grew.is_empty() {
        // The other direction, reported the SAME WAY as growth — `rule: was -> now`, one line each.
        //
        // 🔴 It used to print a bare COUNT ("1 rule(s) now find FEWER"), and that asymmetry is the
        // defect (review ledger V233). Slack is the only thing a reader can act on here, and acting
        // means knowing WHICH rule got cheaper: on a tree with forty recorded rules, "3 rule(s) now
        // find fewer" is a number with no next step, while the growth half two screens down has always
        // named its rules. The direction that asks the reader to DO something was the terser one.
        //
        // Why this matters more than it looks: the slack is only visible while it stands. Measured on a
        // synthetic tree — record 3 secrets, fix one (this line fires), then add one back, and the run
        // exits 0 with an EMPTY stderr, because the count is 3 again and the baseline records 3. The
        // tree got worse and the gate is silent, correctly: a baseline holds one number per rule, not a
        // low-water mark, so a reading that has been re-consumed no longer exists to report. Recording
        // the best-ever count instead would mean this gate writes to a committed file on runs that
        // pass, which is a side effect a CI step must not have. So the only honest lever is to make the
        // ONE moment the slack is visible worth acting on.
        let mut shrank: Vec<(&String, u64, u64)> = Vec::new();
        for (rule, was) in &recorded {
            let now = by_rule.get(rule).copied().unwrap_or(0);
            if now < *was {
                shrank.push((rule, *was, now));
            }
        }
        if !shrank.is_empty() {
            eprintln!(
                "zzop: no rule grew. {} rule(s) now find FEWER than the baseline at {path} records:",
                shrank.len()
            );
            for (rule, was, now) in &shrank {
                eprintln!("  {rule}: {was} -> {now}");
            }
            eprintln!(
                "  That is standing slack: the gate would not fail until each of these climbs back to \
                 its recorded number. Delete {path} and re-run to tighten it to what the tree actually \
                 has now — nothing reminds you again once the counts climb back."
            );
        }
        std::process::exit(0);
    }

    eprintln!(
        "zzop: {} rule(s) find more than the baseline at {path} records:",
        grew.len()
    );
    for (rule, was, now) in &grew {
        eprintln!("  {rule}: {was} -> {now}");
    }
    eprintln!(
        "  Read `findings.shown` above for the sites. If these are wanted, delete {path} and re-run \
         to re-record."
    );
    std::process::exit(FAIL_ON_EXIT);
}
