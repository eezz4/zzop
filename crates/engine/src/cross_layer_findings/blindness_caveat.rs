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
pub(super) fn build(source_ios: &[SourceIo]) -> Option<String> {
    let zero_sources: Vec<&str> = source_ios
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
        .collect();
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

/// Does this keyed consume evidence real extraction visibility? A verb-path http key whose path
/// segments are ALL `{}` placeholders carries no route identity — it cannot pick out any particular
/// endpoint, so it is not visibility evidence (a root `VERB /` key, zero segments, still counts:
/// that IS a real route identity). Non-http-shaped keys (`table:users`, env keys, topics — no
/// "VERB /path" shape) always count.
fn evidences_visibility(key: &str) -> bool {
    let Some((_verb, path)) = key.split_once(' ') else {
        return true;
    };
    let mut segments = path.split('/').filter(|s| !s.is_empty()).peekable();
    if segments.peek().is_none() {
        return true; // "VERB /" — a real root route.
    }
    segments.any(|s| s != "{}")
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
