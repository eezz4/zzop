//! The per-extension CENSUS this cell judges, and the share predicate that judges it — bucketing
//! the tree's walked files by extension, and deciding which of those buckets is a PRINCIPAL filetype.
//!
//! Split out of the parent on 2026-08-20 for the repo's per-file line cap; the code moved unchanged
//! (the parent's own history records what changed that day, and it was not this move). The seam is
//! the per-file pass versus the reply: everything that opens `ir.loc` and classifies a path is here,
//! and nothing here knows what a row looks like or what the reply says about one. The parent still
//! sums these buckets — those totals ARE the denominators it feeds back to [`is_principal`], so they
//! belong at the call site that owns the question rather than in the pass that filled the buckets.
//!
//! The bucket carries no LINE tally, and that is a deletion rather than an omission: a second share
//! test over `ir.loc`'s line counts gated this cell until 2026-09-01. The parent's module doc owns
//! why it went and what still holds its half of the argument.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

/// Share of the tree an extension must hold to be a PRINCIPAL filetype — the ENGINE's constant, reached
/// through `zzop-facade`'s re-export rather than copied.
///
/// This was a local literal for one review cycle, justified by the layering rule that no shipped code in
/// this crate reaches below `zzop-facade`. The rule is real; the conclusion was not. `zzop-facade` IS a
/// shipped dependency here and already re-exports engine items for exactly this case, so the copy bought
/// nothing and cost the drift the crate's own convention exists to prevent: change the engine's floor and
/// the two reach reports, `unreadExtensions` and `zeroExtraction` all move while this cell silently keeps
/// 10, so one reply calls an extension principal on one surface and not on another with no sentence
/// explaining it, and the whole workspace stays green. Sealing a copy with a test was the alternative;
/// deleting the copy is strictly better, because there is then nothing to seal.
const PRINCIPAL_SHARE_PCT: usize = zzop_facade::MIN_UNCOVERED_EXTENSION_SHARE_PCT;

/// Per-extension tallies, in the same vocabulary `zzop coverage`'s table publishes.
#[derive(Default)]
pub(super) struct ExtCounts {
    pub(super) files: usize,
    pub(super) structural: usize,
    pub(super) in_dep_graph: usize,
}

/// Lowercased tail after the last `.` of the last path segment; the whole name (lowercased) when there
/// is no dot, so `Makefile` groups as `makefile` rather than vanishing into an empty key.
///
/// A THIRD copy of a rule whose other two (`zzop_facade::query_coverage` and the engine census's own)
/// must already agree byte-for-byte — and it exists because neither is reachable from this crate, which
/// ships nothing below `zzop-facade`. So the relation is sealed the way this crate seals every other
/// cross-crate literal: `coverage_gaps_tests.rs` runs the coverage query over the same tree and
/// requires every row emitted here to match that table's own `files`/`structural` cells, over a fixture
/// carrying the three names that break a naive split (no dot, upper case, double extension).
fn ext_of(rel: &str) -> String {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    match base.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => base.to_ascii_lowercase(),
    }
}

/// Buckets the tree's walked files by extension. Bucketing rule is `query_coverage::tree_view`'s,
/// arm for arm: a DEGRADED file is neither structural nor lexical-only (a parser tried and bailed), and
/// `in_dep_graph` counts a NON-EMPTY dep source entry, never mere key presence — the engine gives every
/// parsed file a dep entry, so counting keys would read "parsed" as "resolved" and hide exactly the
/// sparsity this field exists to show.
pub(super) fn by_extension(ir: &Value, degraded: &HashSet<&str>) -> BTreeMap<String, ExtCounts> {
    let mut structural: HashSet<&str> = ir
        .get("symbols")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("file").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let dep = ir.get("dep").and_then(Value::as_object);
    if let Some(dep) = dep {
        structural.extend(dep.keys().map(String::as_str));
    }
    // The third channel, folded in here for the same reason and on the same day as in the coverage
    // query this bucketing mirrors: a frontend can project io facts and nothing else (the SQL one
    // does, by design), so symbols+dep alone calls a parsed file unparsed. The seal test crosses this
    // module's rows against that table, so the two must fold the same three channels or the seal — not
    // a user — is what discovers the drift.
    if let Some(io) = ir.get("io").and_then(Value::as_object) {
        for side in ["provides", "consumes"] {
            let Some(rows) = io.get(side).and_then(Value::as_array) else {
                continue;
            };
            structural.extend(
                rows.iter()
                    .filter_map(|r| r.get("file").and_then(Value::as_str)),
            );
        }
    }
    let mut by_ext: BTreeMap<String, ExtCounts> = BTreeMap::new();
    let Some(loc) = ir.get("loc").and_then(Value::as_object) else {
        return by_ext;
    };
    for rel in loc.keys() {
        let entry = by_ext.entry(ext_of(rel)).or_default();
        entry.files += 1;
        if !degraded.contains(rel.as_str()) && structural.contains(rel.as_str()) {
            entry.structural += 1;
        }
        if dep
            .and_then(|d| d.get(rel))
            .and_then(Value::as_array)
            .is_some_and(|targets| !targets.is_empty())
        {
            entry.in_dep_graph += 1;
        }
    }
    by_ext
}

/// `true` when `part` is at least [`PRINCIPAL_SHARE_PCT`] of `whole`. An empty `whole` is never a
/// principal share of anything (and never a division by zero).
///
/// This is now the ONLY share predicate this module has, and the parent calls it once per row rather
/// than twice — the file-count leg for the no-parser kind, the structural-population leg for the
/// parsed one. What is gone is a THIRD call over line counts; the parent's module doc owns why.
pub(super) fn is_principal(part: u64, whole: u64) -> bool {
    whole > 0 && part * 100 >= whole * PRINCIPAL_SHARE_PCT as u64
}

/// The extensions the share bar held back from the parent's row list — the same predicate those rows
/// use, minus its share leg, sorted largest first.
///
/// # Why this is disclosed rather than simply excluded
/// The row list is a SHORTLIST and earns its bar: without it a twelve-file tree with one `.jsonc`
/// reports a coverage gap, which is the locale noise the bar was built for and which
/// `coverage_gaps_tests` pins against. But the bar cannot tell signal from noise — one project's 114
/// `.xml` MyBatis mappers clear it at 15.8% and are named, while another app's mappers at 7.7%,
/// holding every SQL statement that app has, are deleted. Same filetype, same content, opposite
/// answer.
///
/// Both are true because they are about different channels, and only one of them was lying: `basis`
/// exists so an empty `extensions` cannot read as a verdict, and it said what was crossed while never
/// saying anything had been withheld. So the bar keeps picking rows and this list keeps `basis`
/// honest. Nothing is deleted from the reply; a shortlist stays a shortlist.
///
/// Largest first because the actionable one is always the biggest — a reader who stops after the first
/// name has stopped at the right one.
pub(super) fn withheld_by_share(
    by_ext: &BTreeMap<String, ExtCounts>,
    total_files: u64,
) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = by_ext
        .iter()
        .filter(|(ext, c)| {
            c.in_dep_graph == 0
                && c.structural == 0
                && zzop_facade::extraction_can_lose_facts(ext)
                && !is_principal(c.files as u64, total_files)
        })
        .map(|(ext, c)| (ext.clone(), c.files as u64))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}
