//! THE DISCLOSURE FOLD (2026-07-29) — the one shaper every analyze-shaped reply runs the facade's
//! run-global blindness-class registry through.
//!
//! # What it does, and why it is allowed to
//! The facade emits the whole registry on every output: 17 classes of prose, ~10.6KB, BYTE-IDENTICAL on
//! every run (`zzop_engine::BLINDNESS_REGISTRY` is a `const`, and the facade's view function takes no
//! run argument). Measured on zzop's own tree it was 10.6 of 16.6KB of the `analyze` reply, and ~70% of
//! a small tree's — a ~2,500-token fixed tax on every tool call for this output's primary reader, an AI
//! agent, 100% repeated from the second call onward. It was also the ONE list this crate forwarded
//! uncapped while capping even `degraded`.
//!
//! Decision 1c ("a disclosure an agent has to notice is not a disclosure") requires the RUN to report
//! what class was checked and what was not detected. Folding stays inside that because the fact that
//! gaps exist — and their MAGNITUDE — stays in the reply, unasked: `classes` and the per-status counts.
//! What leaves is the invariant prose, which is now one lookup away on both surfaces
//! (`zzop contract disclosure-classes`, MCP `resources/read zzop://contract/disclosure-classes`) —
//! the same treatment `rule-catalog`, likewise an invariant table, already gets.
//!
//! # The one property that keeps it honest
//! The counts are COUNTED off the registry the reply was handed — the same registry the contract
//! document is rendered from — never a hand-maintained tally. A class added to the engine's registry
//! moves these numbers with no edit here. `crates/summary/tests/disclosure_fold.rs` seals both halves
//! against `zzop_engine::blindness_registry()` directly, so a fold that stopped tracking growth fails
//! the build rather than shipping a smaller number than the truth.
//!
//! # What is NOT folded
//! The RUN-VARYING disclosure channels, which never rode this registry: `coverage.joinContributionZero`
//! (asserted per tree), the rest of the `coverage` census, and the `warnings` self-reports. They are why
//! folding the static registry does not remove THIS run's disclosure. The uncapped `zzop facts` lane
//! also keeps the registry verbatim — that surface exists to hand a rule author everything, and its own
//! doc says so.

use serde_json::{json, Value};

use crate::contracts::{DISCLOSURE_CONTRACT_NAME, URI_PREFIX};

/// Folds the facade's full `disclosure` registry array into the ~10-line summary a reply carries:
/// `{classes, asserted, partial, notYetDetected, note, resource, command}`.
///
/// Degradation: a `disclosure` that is not an array (an older/edge facade output) folds to zero counts
/// with the pointer intact rather than to a missing channel — the pointer is the half that always
/// resolves, and a reply with no `disclosure` key at all would be the one shape a reader could mistake
/// for "nothing to disclose".
pub(crate) fn fold(disclosure: &Value) -> Value {
    let classes = disclosure.as_array().map(Vec::len).unwrap_or(0);
    let count = |token: &str| {
        disclosure
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| entry["status"] == json!(token))
                    .count()
            })
            .unwrap_or(0)
    };
    // The three wire status tokens, in the order the fold reports them — the same closed set the
    // engine's own status pin (`crates/engine/src/disclosure/tests.rs`) holds. A fourth token would
    // land in `classes` and in none of the three, which is exactly what the fold's derivation test
    // fails on rather than quietly under-reporting.
    let [asserted, partial, not_yet] = ["asserted", "partial", "notYetDetected"].map(count);
    // The magnitude of the GAP, spelled out: the counts already carry it, but the number an agent
    // actually needs to act on is "how many of these am I not covered for", and making it read that
    // subtraction itself is the kind of noticing decision 1c says a disclosure must not require.
    let gaps = partial + not_yet;
    json!({
        "classes": classes,
        "asserted": asserted,
        "partial": partial,
        "notYetDetected": not_yet,
        "note": format!(
            "{classes} known classes of silent failure in zzop's own output; {gaps} are only partially \
             detected or not detected at all. Identical every run, so the full text ships once, not per \
             call — read it with the command/resource below. Run-VARYING disclosure is already in this \
             reply: `coverage` (incl. `joinContributionZero`) and `warnings`."
        ),
        "resource": format!("{URI_PREFIX}{DISCLOSURE_CONTRACT_NAME}"),
        "command": format!("zzop contract {DISCLOSURE_CONTRACT_NAME}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(id: &str, status: &str) -> Value {
        json!({ "id": id, "group": "extraction-blind", "summary": "prose", "status": status })
    }

    #[test]
    fn counts_every_status_and_leaves_the_prose_behind() {
        let registry = json!([
            class("a", "asserted"),
            class("b", "partial"),
            class("c", "partial"),
            class("d", "notYetDetected"),
        ]);
        let folded = fold(&registry);
        assert_eq!(folded["classes"], 4);
        assert_eq!(folded["asserted"], 1);
        assert_eq!(folded["partial"], 2);
        assert_eq!(folded["notYetDetected"], 1);
        assert!(!folded.to_string().contains("prose"));
        assert_eq!(
            folded["resource"],
            json!("zzop://contract/disclosure-classes")
        );
    }

    /// The fold reports the GAP count in words, and it must be the sum of the two non-asserted
    /// statuses — the number a reader would otherwise have to compute to know what it is exposed to.
    #[test]
    fn the_note_states_the_gap_as_the_non_asserted_total() {
        let registry = json!([
            class("a", "asserted"),
            class("b", "partial"),
            class("c", "notYetDetected"),
        ]);
        let note = fold(&registry)["note"].as_str().unwrap().to_string();
        assert!(note.contains("3 known classes"), "{note}");
        assert!(note.contains("2 are only partially detected"), "{note}");
    }

    /// A missing/!array channel keeps the pointer — the half that is always true — instead of dropping
    /// the key and reading as "nothing to disclose".
    #[test]
    fn a_missing_registry_still_points_at_the_full_text() {
        let folded = fold(&Value::Null);
        assert_eq!(folded["classes"], 0);
        assert_eq!(
            folded["resource"],
            json!("zzop://contract/disclosure-classes")
        );
    }
}
