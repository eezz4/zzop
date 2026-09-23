//! §37 LANDING for `security/csp-weak-or-disabled` — tightening the policy SUCCEEDS, and what it costs
//! is every inline block on the page, enforced by the browser rather than by anything that runs in CI.
//!
//! WHY (`1.architecture/rules/rule-quality.md` §27 leg 3). Enabling Helmet's default policy, or dropping
//! `unsafe-inline`, stops every inline `<script>`/`<style>`, every `on*=` attribute handler, and every
//! widget or analytics snippet that injects one — third-party ones included. The affected page renders
//! BLANK with console errors; nothing fails a build and nothing fails a test, so a green pipeline is not
//! evidence the rollout is safe. That asymmetry is why the sentence also carries the order that avoids
//! it: report-only first, real traffic long enough to reach the routes the tests never open, nonces or
//! hashes wired into every inline block those reports name, and only then the enforcing header.
//!
//! WHY IT TAKES ITS OWN CONSTANT (§37's most-dangerous-reuse test). `CORS_ORIGIN_ALLOWLIST_LANDING` is
//! the closest sibling — same pack, same "the browser is the enforcer, not your server" shape — and it
//! is still a different noun: that one is about a completeness obligation over OTHER PEOPLE'S origins,
//! where the failure is a caller whose response read fails after the write already committed. This one
//! is about the reader's OWN page going blank, and its exit is a staged rollout rather than a list to
//! keep complete. A landing whose noun is an origin list would be shipped to a reader who is not
//! enumerating origins.
//!
//! POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
//! every token stays present and spelled exactly once, and the pin must go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The CSP-enforcement-rollout landing, spliced ahead of the "keep a restrictive policy" imperative.
const CSP_ENFORCEMENT_ROLLOUT_LANDING: &str = "TIGHTENING CSP IS ENFORCED BY THE BROWSER THE MOMENT IT SHIPS, AND IT BLOCKS BEFORE IT REPORTS: enabling Helmet's default policy, or dropping `unsafe-inline`, stops every inline `<script>`/`<style>`, every `on*=` attribute handler and every widget or analytics snippet that injects one — including third-party ones you do not control — and the affected page renders blank with console errors rather than failing a build or a test, so a green pipeline is not evidence the rollout is safe. THE ORDER THAT AVOIDS THAT: ship the intended policy on `Content-Security-Policy-Report-Only` FIRST, collect violations from real traffic long enough to cover the routes your tests never open, wire a per-response nonce (or hashes) into every inline block those reports name, and only then move the same policy onto the enforcing header.";

/// One constant, one carrier — asserted against the shipped pack so a sibling that grows a CSP remedy
/// and pastes this sentence fails here rather than shipping a second spelling.
#[test]
fn the_csp_enforcement_rollout_landing_is_carried_by_exactly_one_rule() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("security.json")).expect("security.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(CSP_ENFORCEMENT_ROLLOUT_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        ["csp-weak-or-disabled"],
        "the CSP-enforcement-rollout landing is carried by a different rule set than the one it was \
         written for"
    );
}

/// The position pin, on a DELIVERED finding. The `helmet-csp-false` arm is used because it is the one
/// this rule's own fixtures already prove fires exactly once on a three-line file.
#[test]
fn csp_weak_or_disabled_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/app.ts",
        "declare const helmet: any;\ndeclare const app: any;\napp.use(helmet({\n  contentSecurityPolicy: false,\n}));\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "csp-weak-or-disabled");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "csp-weak-or-disabled",
        &h[0].message,
        CSP_ENFORCEMENT_ROLLOUT_LANDING,
        "Keep a restrictive policy: enable Helmet's default CSP",
    );
}
