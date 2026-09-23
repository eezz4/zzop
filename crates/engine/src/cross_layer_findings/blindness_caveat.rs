//! Extraction-blindness caveat: the shared sentence `cross_layer_findings`'s `unconsumed-endpoint`/
//! `unconsumed-mutation-endpoint` findings get appended when a sibling tree in the join contributed NO
//! joinable io at all.

use zzop_core::{Finding, SourceIo};

/// Builds the shared extraction-blindness caveat sentence for this join's `source_ios` — `None` when
/// every source contributed at least some joinable io, which is the common, healthy case: no caveat, no
/// per-finding cost. "No joinable io" = zero provides AND zero KEYED consumes, the same "no JOINABLE
/// contribution" definition `CoverageCensus::join_contribution_zero` uses (see that field's doc) — an
/// unresolved consume proves the extractor SAW a call site, it just could not resolve the target, so it
/// does not count as evidence of blindness here.
///
/// Appended (not prepended, not substituted) to an `unconsumed-endpoint`/`unconsumed-mutation-endpoint`
/// finding's own message via [`append`] below: those findings never get suppressed by this — a provide
/// with zero consumers anywhere in the run is still reported, just with the honest caveat that a
/// sibling tree's own extraction blindness could be the real explanation rather than a genuinely dead
/// endpoint (round dogfood: a fe-vue tree that failed to parse contributed 0 provides + 0 keyed
/// consumes, and 12 unconsumed-mutation-endpoint findings on the BE side read as dead endpoints while
/// the real cause sat only in far-away stderr warnings).
///
/// Known over-fire (accepted, hedged by "can be" in the text): a sibling tree that LEGITIMATELY has no
/// io (a shared-lib/UI package in a monorepo join, or a pathological zero-file tree — `SourceIo`
/// carries no file count to tell them apart) is also named. The caveat stays phrased as a possibility,
/// never an assertion; tightening this to "zero joinable io AND unparsed-extension evidence" needs
/// per-source coverage plumbed in — do that if field rounds show the hedge reads too strong.
///
/// Unlike `CoverageCensus::join_contribution_zero` (a strict exact-zero ASSERTION — see the divergence
/// pin in `framework_silence/tests.rs`), this caveat is a heuristic tripwire, so it may discount weak
/// evidence: a keyed consume whose http path is ENTIRELY `{}` wildcards (e.g. `GET /{}` from a
/// hand-rolled fetch wrapper's own internal call) proves the extractor saw *a* call but carries no
/// route identity, so it does not count as visibility evidence here (round dogfood: fe-svelte's single
/// `GET /{}` key was its only keyed consume while 20+ real call sites flowed unextracted through the
/// wrapper — 32 unconsumed findings fired with no caveat).
/// The sources this join saw NO joinable io from — zero provides AND zero route-identity-bearing keyed
/// consumes. Split out of [`build`] on 2026-09-07 so the SEVERITY path can read the same set the caveat
/// sentence names: the band and the sentence must not disagree about which trees the join is blind to.
///
/// 🔴 The bug that forced the split (review ledger V63). Severity for `unconsumed-mutation-endpoint`
/// keyed off `majority_unresolved_http_sources` alone, and that predicate is a RATIO over a floor
/// (`MIN_TOTAL_CONSUMES`) — so a tree with 5 consumes of which 3 were unresolved counted as blind and
/// dropped the band to `info`, while a tree that contributed NOTHING fell below the floor, counted as
/// NOT blind, and left the band at `warning`. Measured on `corpus/oss/fe-svelte` + `be-gin`: 16 write
/// endpoints reported at `warning` because the caller tree's `.svelte` files were never parsed. The band
/// moved OPPOSITE to the evidence, which is exactly what the 2026-08-25 severity rule forbids.
///
/// Zero contribution alone is NOT enough to move a band, and that is why this returns a set rather than
/// a verdict: a shared-lib or UI-only package in a monorepo join legitimately has no io, and silencing
/// a real attack-surface warning because an unrelated tree is io-less would be a worse trade than the
/// bug. The caller ANDs this with a positive blindness measurement — see `unconsumed_family`.
pub(super) fn zero_contribution_sources(source_ios: &[SourceIo]) -> Vec<&str> {
    source_ios
        .iter()
        .filter(|s| {
            s.io.provides.is_empty()
                && !s
                    .io
                    .consumes
                    .iter()
                    .any(|c| c.key.as_deref().is_some_and(evidences_visibility))
        })
        .map(|s| s.source.as_str())
        .collect()
}

pub(super) fn build(source_ios: &[SourceIo]) -> Option<String> {
    let zero_sources = zero_contribution_sources(source_ios);
    if zero_sources.is_empty() {
        return None;
    }
    let (noun, pronoun) = if zero_sources.len() == 1 {
        ("tree", "its")
    } else {
        ("trees", "their")
    };
    let names = zero_sources
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        " NOTE: {noun} {names} contributed no joinable io facts ({pronoun} sources may be unparsed — see \
warnings) — this can be extraction blindness rather than a dead endpoint."
    ))
}

/// Does this keyed consume evidence real extraction visibility? Exactly the kernel's route-identity
/// gate: a verb-path http key whose path segments are ALL `{}` placeholders picks out no particular
/// endpoint, so it is not visibility evidence; a root `VERB /` key and any non-"VERB /path" key
/// (`table:users`, a tRPC procedure key) do count. A local re-implementation used to live here — it now
/// CALLS [`zzop_core::key_carries_route_identity`], the same predicate the linker's
/// unprovided-vs-unresolved bucketing and the single-tree `http/unprovided-consume` veto use, so the
/// three surfaces can never disagree about which keys are junk.
fn evidences_visibility(key: &str) -> bool {
    zzop_core::key_carries_route_identity(key)
}

/// Appends `caveat` (when `Some`) to every finding's message, in place — the shared tail
/// `compute_cross_layer_findings` applies to both `unconsumed-endpoint` and
/// `unconsumed-mutation-endpoint` finding sets identically.
pub(super) fn append(findings: &mut [Finding], caveat: &Option<String>) {
    let Some(caveat) = caveat else { return };
    for f in findings {
        f.message.push_str(caveat);
    }
}

#[cfg(test)]
mod tests {
    use super::{build, evidences_visibility};
    use zzop_core::{IoConsume, IoFacts, SourceIo};

    fn keyed_consume(key: &str) -> IoConsume {
        IoConsume {
            kind: "http".to_string(),
            key: Some(key.to_string()),
            file: "src/lib/api.js".to_string(),
            line: 4,
            raw: None,
            method: None,
            retry_configured: None,
            body: None,
            client: None,
        }
    }

    fn source(name: &str, consumes: Vec<IoConsume>) -> SourceIo {
        SourceIo {
            source: name.to_string(),
            io: IoFacts {
                provides: Vec::new(),
                consumes,
            },
        }
    }

    #[test]
    fn wildcard_only_keys_are_not_visibility_evidence_but_real_keys_are() {
        assert!(!evidences_visibility("GET /{}"));
        assert!(!evidences_visibility("POST /{}/{}"));
        assert!(evidences_visibility("GET /")); // root route IS a real identity
        assert!(evidences_visibility("GET /api/{}"));
        assert!(evidences_visibility("table:users")); // non-verb-path kinds always count
    }

    #[test]
    fn a_tree_whose_only_keyed_consume_is_fully_wildcarded_is_named_blind() {
        // The fe-svelte round-10 shape: one junk `GET /{}` key from a fetch wrapper's internal call,
        // 20+ real call sites unextracted — the caveat must fire despite the nonzero keyed count.
        let sources = vec![source("fe-svelte", vec![keyed_consume("GET /{}")])];
        let caveat = build(&sources).expect("caveat should fire");
        assert!(caveat.contains("'fe-svelte'"), "got: {caveat}");
    }

    #[test]
    fn a_tree_with_a_real_route_key_is_not_named() {
        let sources = vec![source("fe", vec![keyed_consume("GET /api/articles")])];
        assert!(build(&sources).is_none());
    }
}
