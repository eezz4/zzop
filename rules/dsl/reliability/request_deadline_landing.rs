//! §33/§37 LANDING for the two rules whose remedy is "put a deadline on this outbound call" —
//! `reliability/fetch-no-timeout` (TypeScript) and `reliability/reqwest-no-timeout` (Rust).
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). Both
//! messages end in a one-line edit that is easy to make and easy to make badly, and neither said
//! anything about either half of the cost. First, a deadline converts a class of slow SUCCESSES into
//! failures the caller must now handle: whatever number is picked is a claim about the longest this
//! particular call may legitimately take, and an upload, a report query or a cold start does not fit
//! the number an interactive call fits. Second, and this is the half a reader is least likely to
//! supply for themselves: aborting is local. `AbortSignal`/`ClientBuilder::timeout` stop THIS side
//! waiting; they do not reach the peer, so a call that times out may already have been received and
//! acted on, and this side cannot tell that from a request that never arrived. Retrying past a
//! timeout is therefore a SECOND attempt at work that may already be done, which is the failure the
//! remedy quietly introduces on any non-idempotent call.
//!
//! ONE CONSTANT FOR THE TWO, byte-identical, and the spellings stay out of it. Nothing in the
//! sentence is TypeScript or Rust: the deadline is on a request, the peer is on the other side of a
//! socket, and each rule keeps its own API spelling in its own imperative — `AbortSignal.timeout(...)`
//! there, `Client::builder().timeout(...)` here. That is what makes ONE spelling possible across two
//! languages, and `reliability/reliability.rs` already records that these two are one rule split by
//! `require_file` reach rather than two ideas.
//!
//! WHY `INCR_TTL_LOSS_LANDING` IS NOT REUSED — §37's most-dangerous-reuse test, and the nearest
//! sibling by vocabulary because both sentences are about an expiry that somebody has to choose. That
//! constant is about a Redis KEY's TTL: what it costs is a rate-limit window that stops resetting, and
//! its whole body is `INCR`/`EXPIRE`/`HEXPIRE` sequencing. Nothing in it is true of an HTTP client
//! deadline, and a reader who added `AbortSignal.timeout(5000)` would be handed a paragraph about
//! `MULTI`.
//!
//! NOT A DISQUALIFIER (§38's two axes). Neither finding becomes wrong because a deadline is hard to
//! size: the call still has no timeout, and a stalled peer still holds the task. Both rules keep the
//! axis-A pins they took on 2026-08-29 (`fetch-no-timeout`'s shared-client blind spot,
//! `reqwest-no-timeout`'s function-local scope block), each in its own `fn`, and this landing is
//! pinned here in `fn`s of its own so no block's helper set decides the other axis's verdict.
//!
//! POSITION, not presence. The invalidation probe for both is to move this constant to the tail of the
//! message: every token stays present and spelled exactly once, and each pin goes red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

const REQUEST_DEADLINE_LANDING: &str = "A DEADLINE TURNS A SLOW SUCCESS INTO A FAILURE, AND THE ABORT ONLY REACHES THIS SIDE OF THE WIRE: the peer never hears it, so a call that times out may already have been received and acted on — the charge made, the row written, the mail sent — and this side cannot tell that from a request that never arrived. Size the value against what this particular call does rather than a round number, since an upload, a report query or a cold start legitimately outlasts an interactive request; and where the call is not idempotent, give it an idempotency key or a reconciliation path in the same change, because a retry past a timeout is a second attempt at work that may already be done.";

/// `fetch-no-timeout`. Same delivered fixture the axis-A pin uses (a Workers `scheduled` module, which
/// is how this rule reaches a standalone backend repo with no server-ish path segment), in a separate
/// `fn` so the two verdicts stay independent.
#[test]
fn fetch_no_timeout_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/jobs/refresh.ts",
        "declare const ECB_URL: string;\n\nasync function fetchRates() {\n  return fetch(ECB_URL);\n}\n\nexport default {\n  async scheduled(controller: any, env: any, ctx: any) {\n    return fetchRates();\n  },\n};\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "fetch-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "fetch-no-timeout",
        &h[0].message,
        REQUEST_DEADLINE_LANDING,
        "Pass an explicit timeout",
    );
}

/// `reqwest-no-timeout`. This is the arm that fires nowhere on the dogfood corpus, so the delivered
/// fixture below is the only place the shipped bytes are ever read — which is exactly why the pin is
/// on a DELIVERED finding rather than on the template.
#[test]
fn reqwest_no_timeout_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-rel-rust");
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::new();\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "reqwest-no-timeout");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "reqwest-no-timeout",
        &h[0].message,
        REQUEST_DEADLINE_LANDING,
        "Set one: `Client::builder()",
    );
}

/// BYTE-IDENTITY across the language boundary, asserted rather than assumed. Two packs' worth of
/// discipline in one assertion: if either message ever paraphrases the sentence to sound more like its
/// own language, that rule silently stops being a carrier and its pin stops indexing.
#[test]
fn both_carriers_splice_the_same_bytes() {
    let dir = TempDir::new("zzop-rel-both");
    dir.write(
        "src/jobs/refresh.ts",
        "declare const ECB_URL: string;\n\nasync function fetchRates() {\n  return fetch(ECB_URL);\n}\n\nexport default {\n  async scheduled(controller: any, env: any, ctx: any) {\n    return fetchRates();\n  },\n};\n",
    );
    dir.write(
        "src/client.rs",
        "pub async fn fetch(url: &str) -> Result<String, reqwest::Error> {\n    let client = reqwest::Client::new();\n    client.get(url).send().await?.text().await\n}\n",
    );
    let out = scan(&dir);
    for rule in ["fetch-no-timeout", "reqwest-no-timeout"] {
        let h = hits(&out, rule);
        assert_eq!(h.len(), 1, "{rule}: {:?}", out.findings);
        assert_eq!(
            h[0].message.matches(REQUEST_DEADLINE_LANDING).count(),
            1,
            "{rule}: the shared landing must appear exactly once, byte-identical. In: {}",
            h[0].message
        );
    }
}
