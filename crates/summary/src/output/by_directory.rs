//! `findings.byDirectory` — where this tree's findings SIT, folded to the first segment of each
//! finding's path, as a distribution and nothing more.
//!
//! # The gap this closes
//! Findings concentrate, hard, and no channel of the reply said so. A reader who saw `total: 511` on
//! fastapi had no way to learn that 484 of them (94.7%) sat under one directory, so a reply whose
//! `bySeverity` and `byRule` both read as statements about the project was in fact a statement about
//! its documentation examples. The same reply shape answered the same question for four other trees
//! and never once carried the fold that would have shown it.
//!
//! # Why this channel refuses to say which directory is noise — a measurement, not modesty
//! Measured 2026-09-11 on the restored corpus with `zzop analyze <tree> --limit 1000`, folding
//! `findings.shown` (uncapped at that limit for every tree below, so the fold is over the full set):
//!
//! | tree | findings | top segment | share |
//! |---|---|---|---|
//! | `fastapi` | 511 | `docs_src/` | 94.7% (the shipped package `fastapi/` holds 2) |
//! | `express` | 56 | `examples/` | 96.4% |
//! | `typeorm` | 43 | `src/` | 58.1% |
//! | `astro` | 111 | `packages/` | 82.0% |
//!
//! The concentration is real in all four. THE MEANING OF THE TOP SEGMENT IS NOT: the first two are
//! documentation and examples, the last two are the product itself. A channel that labelled the top
//! row "probably noise" would be wrong on half of this table, and wrong in the direction that erases —
//! a reader told a directory is noise stops opening it. So the rows carry a name, a count and a share,
//! and the judgement stays with the reader. That is the 2026-09-11 user ruling this module implements,
//! and re-opening it means re-measuring the table above rather than arguing from one tree.
//!
//! # Why the remedy sentence names TWO config keys and insists they differ
//! The obvious next move — "exclude it" — has two spellings in this repo's config and they do
//! different things, which is exactly the kind of distinction a one-word hint would flatten. Top-level
//! `exclude` maps to `globalExcludes`, a REPORT-level filter: the file is still walked, still parsed
//! and still in the dependency graph, and only what is reported about it is dropped
//! (`crates/facade/src/request.rs`'s `global_excludes`). `vocabulary.skipDirs` lands on the WALKER's
//! skip list (`DispatchConfig::skip_dirs`), so a matching directory is never read, and it leaves the
//! dep graph as well as the findings — which is why `coverageGaps` next door can start reporting
//! differently after one of those edits and not after the other.
//!
//! # The CROSS lane, and the ambiguity that rides with it
//! `output::shape_findings` is one shaper with two shipped call sites, so this channel also lands on
//! `crossLayerFindings` (`crate::cross`). Measured on `zzop cross corpus/frameworks/express
//! corpus/frameworks/fastapi`: 207 findings, `docs_src/` 81.2% and `examples/` 18.8% — two rows that
//! belong to two DIFFERENT trees, because a cross-layer finding's path is tree-relative and carries no
//! tree qualifier. The concentration is as real there as it is per tree, so the channel stays; what is
//! NOT true there is that a row is a directory. Two trees with a `src/` each arrive as one row, and
//! that is disclosed in the note rather than left for a reader to discover, which is the same call
//! `cross.rs` already makes one key up when it keeps the path-shape half of the deployment-role axis
//! and drops the manifest half. Whether the cross lane should instead SUPPRESS this channel is a
//! product question this module does not answer on its own.
//!
//! # What is deliberately NOT here
//! No second grain (a two-segment fold doubles the rows to say the same thing), no threshold that
//! fires the channel only above some share (a threshold IS the guess this ruling refused), and no
//! per-directory severity or rule breakdown (`bySeverity`/`byRule` own those axes and a cross-product
//! of three axes is a table, not a disclosure).

use std::collections::BTreeMap;

use serde_json::{json, Value};

/// How many rows ride the wire. A SHORTLIST bar, the same device `coverageGaps.extensions` uses: the
/// channel exists to show concentration, and concentration is visible in the first few rows by
/// definition. `basis` names how many directories were crossed and what these rows cover, so the cut
/// never reads as "that is all of them". Five, because the measured tables above are decided inside
/// their first two rows and the third is already tail.
pub(crate) const MAX_ROWS: usize = 5;

/// The bucket for a finding whose path has no directory at all. Spelled as something that is
/// obviously NOT a directory name, because the column beside it holds directory names a reader may
/// paste into a config, and `""` or `"."` would both be paste-able and both be wrong.
const ROOT_BUCKET: &str = "(root)";

/// The FULL text, for the reply-legends contract document. Interpolates nothing: every key it names is
/// spelled the same on every reply, and its numbers are about the 2026-09-11 corpus measurement rather
/// than about this run, which is what makes it foldable (see [`super::legends`]).
pub(crate) const BY_DIRECTORY_MEANING: &str =
    "`byDirectory` folds every finding to the FIRST segment of its file path and counts them. It is a \
     DISTRIBUTION and it is nothing else: no row here says a directory is noise, and no row says it \
     is product code.\n\n\
     THAT REFUSAL IS A MEASUREMENT rather than modesty. Four public trees were folded this way on \
     2026-09-11 and the concentration is real in every one of them, but the top segment means the \
     OPPOSITE thing in different trees. In `fastapi` 94.7% of 511 findings sit under `docs_src/`, \
     which is documentation, while the shipped package `fastapi/` holds two of them. In `express` \
     96.4% of 56 sit under `examples/`. Against that, `typeorm` puts 58.1% of 43 under `src/` and \
     `astro` 82.0% of 111 under `packages/`, and both of those ARE the product. A channel that \
     guessed which kind it was looking at would be wrong on half of that table, and wrong in the \
     direction that erases, because a reader told a directory is noise stops opening it. So this one \
     reports the share and stops.\n\n\
     WHAT TO DO WITH A ROW YOU DECIDE IS NOISE, and why the two config keys are not \
     interchangeable. Top-level `exclude` is a REPORT filter: a matching path is still walked, still \
     parsed and still in the dependency graph, and only what is REPORTED about it is dropped. \
     `vocabulary.skipDirs` is the WALKER's skip list: a matching directory is never read at all, so \
     it leaves the graph as well as the findings, and the `coverage` and `coverageGaps` channels of \
     this same reply will answer differently afterwards. Reach for `exclude` when the directory holds \
     real code you do not want judged today. Reach for `skipDirs` when it holds vendored or generated \
     bytes nobody should be reading. Neither edit makes the findings it hides false.\n\n\
     THE COUNTS ARE OVER THE FULL SET, exactly like the `total` and `bySeverity` and `byRule` beside \
     them: never over the `shown` window, and never narrowed by a `severity` or `rule` or `limit` \
     argument. `sharePct` is a row's count against the findings this fold could place, to one \
     decimal. A finding whose path has no directory is counted under `(root)`, and one carrying no \
     path at all is outside the fold and is counted in `basis` instead. A row is keyed by the segment \
     NAME, so on a reply that straddles trees — `crossLayerFindings`, whose findings are joined across \
     two or more of them — two trees' `src/` arrive as ONE row, and a row there names a path shape \
     rather than a directory.\n\n\
     THE ROWS ARE A SHORTLIST — the largest few by count, ties broken by name so two runs over one \
     tree produce the same bytes. `basis` names how many directories were crossed and how much of \
     the tree these rows cover, so a short list is never a claim that the rest is empty, and an empty \
     one says the fold ran and had nothing to place rather than that nobody asked.";

/// The first path segment of `file`, with its trailing slash so the value can be pasted into an
/// `exclude` entry as-is, or [`ROOT_BUCKET`] when there is no directory to name.
fn bucket(file: &str) -> String {
    let rel = file.strip_prefix("./").unwrap_or(file);
    match rel.split_once('/') {
        Some((head, _)) if !head.is_empty() => format!("{head}/"),
        _ => ROOT_BUCKET.to_string(),
    }
}

/// One share, to one decimal, against a denominator the caller has already proven non-zero.
fn share_pct(count: usize, of: usize) -> f64 {
    if of == 0 {
        return 0.0;
    }
    (count as f64 * 1000.0 / of as f64).round() / 10.0
}

/// Builds `findings.byDirectory` — `{directories, basis, meaning}`, the shape `coverageGaps` set.
///
/// ALWAYS returned, never conditional, for `coverageGaps`' own reason: a reader who does not see a
/// distribution cannot tell "measured, the findings are spread" from "nobody folded them", and telling
/// those apart is the whole point. `basis` is what makes the empty case readable.
///
/// `findings` is the FULL set — the same population `total`/`bySeverity`/`byRule` are computed over,
/// never the filtered window — so the share a reader quotes does not move when they add a `--rule`.
/// `None` when there is NO DISTRIBUTION TO REPORT and nothing else to disclose.
///
/// # Why this is conditional, and why it is not the threshold this module refuses (ledger V243)
/// This channel shipped unconditionally from birth while its two siblings in the same `findings`
/// object — `testPaths` and `buildPaths` — do the opposite: present when they have something to say,
/// ABSENT otherwise (`deployment_role`'s own doc argues that). Three emission doctrines in one object,
/// and a release freezes whichever each one happens to have. 📏 On a one-directory tree the block was
/// 1,848 bytes of a 19,700-byte reply (9.5%) to say `[{dir: "src/", findings: 1, sharePct: 100}]`, of
/// which 1,595 were the note explaining how to read a distribution that has nothing to distribute.
///
/// 🔴 "What is deliberately NOT here" below refuses **a threshold that fires only above some SHARE**,
/// and that refusal stands: a share threshold decides for the reader which concentration is worth
/// seeing, which is the guess the 2026-09-11 ruling rejected. This gate reads no share at all. It is
/// structural — one row is not a distribution, it is the total restated — so a two-directory tree
/// where one holds 100% still reports, which is exactly the case a share threshold would have hidden.
///
/// ⚠ The gate spares the `placed == 0` case on purpose. A run where findings carry no file path emits
/// `basis`'s "a MEASURED empty, never an unasked question" sentence, and that is a disclosure rather
/// than a distribution — suppressing it would delete the one thing this channel says when it has no
/// rows. So the condition is "fewer than two directories AND nothing unplaced to report".
pub(crate) fn by_directory(findings: &[Value]) -> Option<Value> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut unplaced = 0usize;
    for finding in findings {
        match finding
            .get("file")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|f| !f.is_empty())
        {
            Some(file) => *counts.entry(bucket(file)).or_default() += 1,
            None => unplaced += 1,
        }
    }
    let placed: usize = counts.values().sum();
    // `counts` is keyed by directory, so its length is how many directories hold at least one finding.
    if counts.len() < 2 && unplaced == 0 {
        return None;
    }

    // Count descending, then name ascending — a total order, so two runs over one tree serialize to
    // the same bytes even when two directories tie.
    let mut ranked: Vec<(&str, usize)> = counts.iter().map(|(d, n)| (d.as_str(), *n)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    let rows: Vec<Value> = ranked
        .iter()
        .take(MAX_ROWS)
        .map(|(dir, n)| json!({ "dir": dir, "findings": n, "sharePct": share_pct(*n, placed) }))
        .collect();
    let covered: usize = ranked.iter().take(MAX_ROWS).map(|(_, n)| n).sum();

    let outside = if unplaced == 0 {
        String::new()
    } else {
        format!("; {unplaced} finding(s) carry no file path and are outside this fold")
    };
    let basis = if placed == 0 {
        format!(
            "no finding carried a file path this run, so this fold had nothing to place — a MEASURED \
             empty, never an unasked question{outside}"
        )
    } else {
        format!(
            "{placed} finding(s) folded to their first path segment across {dirs} top-level \
             director(ies); the {shown} row(s) below are the largest by count and hold {} of \
             them{outside}",
            format_args!("{}%", share_pct(covered, placed)),
            dirs = ranked.len(),
            shown = rows.len(),
        )
    };

    Some(json!({
        "directories": rows,
        "basis": basis,
        // FOLDED (see `super::legends`): the readings a reader must not get wrong stay on the wire,
        // and the corpus table behind them ships once from the reply-legends document. The rows and
        // `basis` above are THIS RUN's measurement and are untouched by the fold.
        "meaning": super::legends::folded_string("findings.byDirectory.meaning"),
    }))
}

#[cfg(test)]
mod tests;
