//! The per-extension CENSUS this cell judges, and the two share predicates that judge it — bucketing
//! the tree's walked files by extension, and deciding which of those buckets is a PRINCIPAL filetype.
//!
//! Split out of the parent on 2026-08-20 for the repo's per-file line cap; the code moved unchanged
//! (the parent's own history records what changed that day, and it was not this move). The seam is
//! the per-file pass versus the reply: everything that opens `ir.loc` and classifies a path is here,
//! and nothing here knows what a row looks like or what the reply says about one. The parent still
//! sums these buckets — those totals ARE the denominators it feeds back to [`is_principal`], so they
//! belong at the call site that owns the question rather than in the pass that filled the buckets.

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
    pub(super) lines: u64,
    pub(super) structural: usize,
    pub(super) in_dep_graph: usize,
    /// Lines held by this extension's SINGLE largest file — the only field here that is not a tally,
    /// and the one [`is_principal_population`] subtracts. See that function for the measurement.
    pub(super) largest_file_lines: u64,
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
    for (rel, lines) in loc {
        let entry = by_ext.entry(ext_of(rel)).or_default();
        let this_file = lines.as_u64().unwrap_or(0);
        entry.files += 1;
        entry.lines += this_file;
        entry.largest_file_lines = entry.largest_file_lines.max(this_file);
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
pub(super) fn is_principal(part: u64, whole: u64) -> bool {
    whole > 0 && part * 100 >= whole * PRINCIPAL_SHARE_PCT as u64
}

/// The LINE leg of the principal-filetype test.
///
/// # A largest-file exclusion was tried here on 2026-08-20 and REVERTED the same day
/// The two-share test answers "is this a filetype the tree is MADE OF", and a share ONE file can buy
/// single-handedly is a statement about that file rather than about a filetype. That reasoning is
/// sound and the noise it targeted was real: `corpus/oss/be-express` `.json` is 13 files / 10,229
/// lines of which `package-lock.json` alone is 9,898 (**96.8%**), and `be-nest` `.json` is 6 files of
/// which the lock is **99.0%** — both pure tooling config with nothing to lose. Subtracting the
/// single largest file removed exactly those two rows and left every other row on 19 trees standing.
///
/// It was reverted because the adversarial pass measured what it ALSO removes. Two trees, identical
/// unread payload — 10 MyBatis mappers, ~2,990 XML lines of `${}`-interpolated SQL, 20 `.java` files
/// beside them — differing only in whether the XML lines sit in one file:
///   one 2,600-line mapper + 9 small  -> `coverageGaps.extensions: []`
///   ten 296-line mappers            -> `[{"ext":"xml","files":10,"kind":"data-config",...}]`
/// Same blind spot, opposite answers, and `zzop coverage`'s `unreadExtensions` still reported the
/// concentrated tree — so the two surfaces disagreed about a whole row rather than at the margin.
/// The defence was also one-shot: a second lock-shaped file restores the row.
///
/// The deciding argument is DIRECTION, not the count. This leg ERASES a row, and it erased on an
/// INFERENCE (line concentration implies "not a filetype the tree is made of"). A finding removed by
/// an inference leaves no trace for the reader, which is the asymmetry this repo requires a
/// DECLARATION for. The noise it bought back is disclosed rather than silent: every such row now
/// carries `kind: "data-config"`, and this cell's `MEANING` states that such a row is not a verdict
/// and that the cost depends on what the files hold. A labelled row a reader can dismiss in seconds
/// beats an erased row they cannot see is missing.
///
/// Reopening it needs a signal that separates a 12-file `tsconfig` population from a 10-file MyBatis
/// one, which line concentration provably does not. Two candidates are already measured dead: a
/// filename axis (`*-lock.json`, `tsconfig*`) is a hand list the next ecosystem falls outside of, and
/// "is this extension in any loaded rule's scope" deletes real rows too, since `.vue`/`.svelte` sit
/// in bundled patterns while `.xml` sits in none — and it would make this suppression's correctness
/// depend on an unrelated regex. What is left is reading a little of the CONTENT, which is the only
/// candidate that asks the question the row actually poses.
pub(super) fn is_principal_population(counts: &ExtCounts, total_lines: u64) -> bool {
    is_principal(counts.lines, total_lines)
}
