//! The per-tree join-contribution view — see [`join_visibility`].

use serde_json::{json, Value};

/// The join-contribution view: the two counts the ratio is built from, the ratio itself, and the
/// sentence that says what the ratio does and does not prove.
///
/// Why this stopped being a bare sentence (2026-08-08). The old field answered "did this tree
/// contribute anything at all", which reads IDENTICALLY at one keyed consume and at four hundred. A
/// tree with a single visible call was told `visible` and then went on to accuse the other tree's
/// endpoints of being unconsumed — the accusation and the reassurance came from the same run.
/// Onboarding now asks the user to CONVERGE this number, and a gauge that reads full at 5% cannot be
/// converged toward anything.
///
/// Two things this deliberately does NOT do:
///   * It is not a join count, and `meaning` says so. A key is a PRECONDITION for joining, never
///     proof of one — a keyed consume whose prefix drifted still misses. The join is cross-tree and
///     this surface is per-tree, so the honest fact here is key resolution, and naming it anything
///     stronger would overclaim by exactly the prefix-drift gap.
///   * It ships NO derived rate. The first cut carried a `keyedRatio`, and this reply's standing
///     no-single-score ruling rejected it — correctly, and not on a technicality: `keyedRatio: 1.0`
///     over one extracted call is the same "reads full at 5%" failure the old sentence had, wearing
///     a number instead of a word, and a quotient is exactly the shape that gets quoted away from its
///     magnitude. Counts cannot. 0/0 needs no special case once there is no quotient to invert — the
///     zeros are simply visible.
pub(super) fn join_visibility(census: &Value, join_zero: bool) -> Value {
    let n = |k: &str| census.get(k).and_then(Value::as_u64).unwrap_or(0);

    let meaning = if join_zero {
        "INVISIBLE to the cross-layer join: this tree extracted zero joinable io (no provides, no \
         keyed consumes), so any join finding involving it is not meaningful — a framework/SDK the \
         extractor cannot see is the common cause; a Mode B adapter overlay that contributes io \
         facts (`io.provides`/`consumes`) restores visibility — an overlay carrying only imports \
         or attributes does not."
    } else {
        "This tree contributed joinable io. `consumesKeyed` of (`consumesKeyed` + \
         `consumesUnresolved`) extracted external calls carry a joinable key; a key is a \
         PRECONDITION for joining, not proof that a join happened — a keyed consume whose path \
         prefix drifted still matches no route, and the join itself is cross-tree while this view is \
         per-tree. The counts ship without a quotient on purpose: 1 of 1 and 400 of 440 are not the \
         same evidence, and a rate hides which one you are holding."
    };

    json!({
        "provides": n("ioProvides"),
        "consumesKeyed": n("ioConsumesKeyed"),
        "consumesUnresolved": n("ioConsumesUnresolved"),
        "meaning": meaning,
    })
}
