//! What kind of http provides this run has — the fact that decides both whether
//! `cross-layer/all-consumes-unjoined` fires and what it names as the cause.
//!
//! ## The defect this exists for (2026-09-07, review ledger V97)
//!
//! The predicate used to be one boolean, `run_has_http_provides`, and a verb-unknown route satisfied it.
//! That is a real provide — the PATH is served — but it can never join, because the join key is
//! `"<VERB> <path>"` and its verb is the [`zzop_core::UNKNOWN_VERB`] sentinel `?`. So a run whose routes
//! are ALL verb-unknown (a Django URLconf, which is verb-unknown by construction; Go `HandleFunc` with no
//! method guard; a Next.js `pages/api` serve-all default export) passed the gate and was then told:
//!
//! > "This run does have routes to join against, so the likely cause is ONE unresolved base path"
//!
//! Measured on `fe-vue` + `be-django` before any declaration: the front-end and back-end paths were
//! `/api/articles/feed` on BOTH sides — identical, no prefix problem at all. The cause was the verb
//! sentinel, and the finding sent the reader to `clientBase`/`mountedAt`, which cannot fix it. That is
//! worse than silence: it is a confident wrong address.
//!
//! The engine already draws this distinction one layer up (`cross_layer_findings::partition`'s
//! `without_verb_unknown_provides`), but this rule reads the RAW `CrossLayerResult`, so it has to draw it
//! itself. Kept as a three-state shape rather than a second boolean because the MIXED case is real and
//! needs a third sentence: some routes joinable, some verb-unknown, both remedies worth naming.

use zzop_core::io::CrossLayerResult;

/// The http provides a run actually has, from the join's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HttpProvideShape {
    /// No http provide at all — a front end analyzed alone. Nothing COULD have joined, so the rule
    /// stays silent rather than blaming the user for the shape of their own invocation.
    None,
    /// Every http provide is a verb-unknown sentinel. The paths are served; not one of them can match a
    /// `"<VERB> <path>"` consume key.
    VerbUnknownOnly,
    /// At least one provide could have joined. `with_verb_unknown` says whether verb-unknown routes sit
    /// beside them, because then BOTH causes are live and the message must offer both remedies.
    Joinable { with_verb_unknown: bool },
}

/// Classify the run's http provides. A provide "could have joined" when an http edge landed, when an
/// unconsumed http provide carries a real verb, or when an ambiguous consume named http candidates —
/// the three places a joinable provide survives into `CrossLayerResult`.
pub(super) fn classify(cross_layer: &CrossLayerResult) -> HttpProvideShape {
    let verb_unknown = cross_layer.unconsumed_provides.iter().any(|p| {
        p.provide.kind == "http" && zzop_core::unknown_verb_route_path(&p.provide.key).is_some()
    });

    let joinable = cross_layer.edges.iter().any(|e| e.kind == "http")
        || cross_layer.unconsumed_provides.iter().any(|p| {
            p.provide.kind == "http" && zzop_core::unknown_verb_route_path(&p.provide.key).is_none()
        })
        || cross_layer
            .ambiguous_consumes
            .iter()
            .any(|a| a.consume.kind == "http" && !a.candidates.is_empty());

    match (joinable, verb_unknown) {
        (true, with_verb_unknown) => HttpProvideShape::Joinable { with_verb_unknown },
        (false, true) => HttpProvideShape::VerbUnknownOnly,
        (false, false) => HttpProvideShape::None,
    }
}

/// The sentence naming what most likely went wrong. `n` is the unjoined-consume count already stated.
pub(super) fn cause_sentence(shape: HttpProvideShape, n: usize) -> String {
    match shape {
        // The gate above never reaches here with `None`.
        HttpProvideShape::None | HttpProvideShape::VerbUnknownOnly => format!(
            "This run's routes are ALL verb-unknown — their paths are served, but every one of them keys \
             as `? <path>` because no method literal was witnessed at the registration (a Django URLconf, \
             which is verb-unknown by construction; a Go `HandleFunc` with no method guard; a Next.js \
             `pages/api` serve-all handler). A consume keys as `GET <path>`, so NOT ONE of these {n} calls \
             could match, no matter how the paths line up — and they may already line up exactly. The base \
             path is very likely NOT your problem here"
        ),
        HttpProvideShape::Joinable {
            with_verb_unknown: false,
        } => format!(
            "This run does have routes to join against, so the likely cause is ONE unresolved base path, \
             not {n} independent problems"
        ),
        HttpProvideShape::Joinable {
            with_verb_unknown: true,
        } => format!(
            "This run has routes to join against, so the likely cause is ONE unresolved base path rather \
             than {n} independent problems — but SOME of its routes are also verb-unknown (`? <path>`, no \
             method literal witnessed at the registration), and those can never match a `GET <path>` \
             consume however the paths line up. Both causes are live here"
        ),
    }
}

/// The `routes` remedy, or empty when this run has no verb-unknown route for it to fix.
///
/// Named FIRST among the repairs when it applies, matching what `cross-layer/unknown-verb-route` already
/// tells the reader about the same routes — until 2026-09-07 the two rules in one reply prescribed
/// different things for one defect, and this rule's four repairs did not mention `routes` at all.
pub(super) fn routes_repair(shape: HttpProvideShape) -> &'static str {
    match shape {
        HttpProvideShape::None
        | HttpProvideShape::Joinable {
            with_verb_unknown: false,
        } => "",
        _ => {
            "Pin the method on the verb-unknown routes: `trees[].routes` takes one entry per route \
             (`routes: [{ \"key\": \"GET /articles/feed\" }]`), and `cross-layer/unknown-verb-route` names \
             each such route with a paste-ready stub. Do that FIRST when the paths already match — it is \
             the only one of these repairs that can fix a verb mismatch. "
        }
    }
}

#[cfg(test)]
mod tests;
