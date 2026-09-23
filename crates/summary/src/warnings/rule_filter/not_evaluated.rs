//! The refusal for an id this run never EVALUATED — carved out beside `gated_id` on 2026-09-05, same
//! seam: the parent decides which reading applies, each arm owns the sentence its reading needs.
//!
//! This arm's subject is the reply itself. `nativeAnalyses` already partitions every registered
//! analysis into exactly the three reasons one can be missing from `findings`, and each partition
//! carries a DIFFERENT remedy. The filter never read them, so all three arrived as `shown: 0` with an
//! empty stderr — the same "an empty list reads as a clean bill of health" defect this whole module
//! exists for, one layer in: the id is real, it is registered, it is spelled right, and it still could
//! not have appeared.

/// `Some(warning)` when the reply itself says this run did not evaluate `rule` (or could not report it
/// on this channel), `None` otherwise.
///
/// Reads only `output_view`, so it stays inside this crate's layering: the run publishes the three
/// lists and this arm quotes them back. An id in none of them returns `None` and falls through to the
/// caller's registered-native check.
pub(super) fn not_evaluated_refusal(output_view: &serde_json::Value, rule: &str) -> Option<String> {
    let names = |key: &str| -> bool {
        output_view["nativeAnalyses"][key]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(rule)))
    };

    // Order is the order of the reader's own agency, widest first: what THEY switched off, then what
    // the BUILD switched off, then what nothing switched off at all. Two of these can never both hold
    // (the partition is disjoint by construction), so the order is for the reader, not for correctness.
    if names("disabled") {
        return Some(format!(
            "the `rule` filter names `{rule}`, which this run did NOT evaluate — your config disabled \
             it, so this reply's `shown: 0` is that choice rather than a clean result. An empty list \
             for a disabled analysis is not a measured zero: it was never asked. Stop \
             disabling it (drop its `\"off\"` entry, or name it with a severity such as \
             `rules: {{ \"{rule}\": \"info\" }}`) to get a verdict. This reply's \
             `nativeAnalyses.disabled` lists every id in this state."
        ));
    }
    if names("shippedOff") {
        return Some(format!(
            "the `rule` filter names `{rule}`, which is a real analysis this build SHIPS OFF and your \
             config did not turn on — so this reply's `shown: 0` is that, not a clean result. Same \
             non-evaluation as a disabled rule, different author, and so a different fix: there is no \
             `\"off\"` of yours to remove — name it with a severity to switch it on \
             (`rules: {{ \"{rule}\": \"info\" }}`). This reply's `nativeAnalyses.shippedOff` lists \
             every id in this state."
        ));
    }
    if names("reportedInCrossLayerFindings") {
        return Some(format!(
            "the `rule` filter names `{rule}`, which IS switched on and still cannot appear on this \
             channel: it judges the cross-tree join and reports into `crossLayerFindings`, which a \
             per-tree view does not carry. So this reply's `shown: 0` is the channel rather than a \
             clean result, and neither your config nor this build switched anything off. Run the \
             cross-layer join over the same config and filter there. This reply's \
             `reportedInCrossLayerFindings` list holds every id in this state."
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One population, three partitions, one assertion each — because the whole point of this arm is
    /// that the three are NOT interchangeable: each names a different party as the author of the
    /// silence, and a reader handed the wrong one edits the wrong file and sees nothing change.
    #[test]
    fn each_partition_names_its_own_author_and_its_own_fix() {
        let view = serde_json::json!({
            "nativeAnalyses": {
                "disabled": ["dead-candidates"],
                "shippedOff": ["unimported-export"],
                "reportedInCrossLayerFindings": ["cross-layer/unconsumed-endpoint"],
            }
        });

        let mine =
            not_evaluated_refusal(&view, "dead-candidates").expect("a disabled id must refuse");
        assert!(
            mine.contains("your config disabled it") && mine.contains("Stop disabling it"),
            "the reader has to be told THEY are the author, or they go looking in the build: {mine}"
        );

        let theirs = not_evaluated_refusal(&view, "unimported-export")
            .expect("a shipped-off id must refuse");
        assert!(
            theirs.contains("SHIPS OFF") && theirs.contains("no `\"off\"` of yours to remove"),
            "the shipped-off remedy is the opposite gesture from the disabled one, and saying so is \
             the only thing that stops the reader hunting for an `off` they never wrote: {theirs}"
        );

        let neither = not_evaluated_refusal(&view, "cross-layer/unconsumed-endpoint")
            .expect("a cross-layer-only id must refuse on a per-tree view");
        assert!(
            neither.contains("IS switched on") && neither.contains("crossLayerFindings"),
            "this one is not switched off at all — telling the reader to turn it on would send them \
             to edit a config that is already correct: {neither}"
        );

        assert_eq!(
            not_evaluated_refusal(&view, "circular"),
            None,
            "an id in none of the three lists was evaluated, and refusing it would turn a real \
             measured zero into a false alarm"
        );
    }

    /// A reply that carries no `nativeAnalyses` at all (an older or shaped view) is NO DATA, never a
    /// refusal — the false-positive direction this whole channel refuses.
    #[test]
    fn a_view_without_the_partitions_is_silent() {
        let view = serde_json::json!({ "packsLoaded": [] });
        assert_eq!(not_evaluated_refusal(&view, "dead-candidates"), None);
    }
}
