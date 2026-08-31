//! The FE→BE contract break that no rule reports, and the reason an agent reads it as an all-clear.
//!
//! Two independent reviewers, on different fixtures, fell into the same hole: a **GET** route the front
//! end calls and the back end does not serve produces NO finding at all. `GET /api/auth/whoami` and
//! `GET /api/ghost-endpoint` both landed correctly in `unprovidedConsumes`, and both left
//! `crossLayerFindings` empty.
//!
//! The gating is deliberate and stays. `cross-layer/unprovided-mutation-call` judges WRITE verbs only,
//! because a read call to a missing route is far more often a stale client, a feature flag, or a route
//! served by something outside the analyzed set — and the single-tree `unprovided-consume` gates on the
//! tree providing at least one HTTP route itself, which a pure front end never does, so it is silent
//! there by construction.
//!
//! What was wrong is that neither gate said so. The fifth reviewer put it exactly: *an agent that reads
//! `crossLayerFindings` and stops sees "1 info, all clear" while a broken FE→BE contract is sitting in
//! `distinctBucketKeys`.* The fact reaches two other channels — the bucket itself, and `check_endpoint`
//! answering `consumed-unprovided` — but both require the reader to already suspect something. A
//! findings list that is empty because nothing was found and one that is empty because nothing was
//! ELIGIBLE are the same bytes, which is this repo's first-ranked failure mode.
//!
//! So this says it: the count, a sample, and which gate produced the silence. It is a warning rather
//! than a finding on purpose — promoting read drift to a finding is the judgment the gates already
//! declined, and making it one here would be reversing that decision by the back door.

use zzop_core::{CrossLayerResult, IoConsume};

/// Path samples, this repo's standard for a disclosure list.
const SAMPLE: usize = 3;

/// Names the unprovided READ consumes no rule will report. `None` when there are none — which is both
/// the healthy case and the case where the write-verb rule already spoke.
pub(super) fn maybe_warn(cross_layer: &CrossLayerResult) -> Option<String> {
    let mut keys: Vec<&str> = cross_layer
        .unprovided_consumes
        .iter()
        .map(|t| &t.consume)
        .filter(|c| c.kind == "http")
        .filter(|c| is_read_verb(c))
        .filter_map(|c| c.key.as_deref())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.is_empty() {
        return None;
    }
    let sample = keys.iter().take(SAMPLE).copied().collect::<Vec<_>>();
    let more = keys.len().saturating_sub(sample.len());
    let more = if more > 0 {
        format!(", and {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "{} READ route(s) are called in this run but provided by no analyzed tree, and NO rule reports \
         them: {}{more}. This is a disclosure of a gate, not a new judgment — \
         `cross-layer/unprovided-mutation-call` judges write verbs only (a read call to a missing route \
         is more often a stale client, a flag, or a route served outside the analyzed set), and the \
         single-tree `unprovided-consume` needs its own tree to provide at least one HTTP route, which \
         a pure front end never does. Both gates are deliberate; what was missing is that an empty \
         `crossLayerFindings` looked identical whether nothing was found or nothing was ELIGIBLE. The \
         keys above are also in `crossLayer.unprovidedConsumes`, and `endpoint <key>` answers \
         `consumed-unprovided` for each. If one of them should be served here, the provider tree is \
         missing from this run or its routes were not extracted.",
        keys.len(),
        sample.join(", "),
    ))
}

/// A consume's verb, read from the normalized `"METHOD /path"` key with `method` as the fallback. READ
/// means "not a write verb": an unrecognized or absent verb counts as a read here, which is the
/// direction that keeps this disclosure from claiming a write rule stayed silent when it did not.
fn is_read_verb(consume: &IoConsume) -> bool {
    let verb = consume
        .key
        .as_deref()
        .and_then(|k| k.split_once(' '))
        .map(|(m, _)| m.to_string())
        .or_else(|| consume.method.clone())
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(verb.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
}

#[cfg(test)]
mod tests;
