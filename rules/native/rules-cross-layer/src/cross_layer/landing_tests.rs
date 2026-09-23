//! The §27 ORDER pins for `landing.rs`, and the structural guards that keep those pins meaningful.
//!
//! The assertion is about POSITION, not existence: a reader who acts on the first instruction never
//! reaches a caveat placed behind it. So the invalidation probe for every pin below is "move the landing
//! behind that arm's imperative and leave every token present" — it must go red on order alone.
//!
//! The fixtures are duplicated here rather than reached for in each rule's own test module for a
//! mechanical reason: four of those six modules are inline `#[cfg(test)] mod tests` blocks in files
//! already within a dozen lines of the 300-line source cap, and a pin that cannot be added without
//! splitting its subject is a pin that does not get added.

use super::landing::{
    CALLER_BREAKAGE_LANDING, HANDLER_SUBSTITUTION_LANDING, METHOD_CHANGE_LANDING,
    VENDOR_CONTRACT_LANDING,
};
use super::*;

use super::route_near_miss::PrefixNearMissRecord;
use zzop_core::io::{AmbiguousConsume, CrossLayerResult, IoConsume, TaggedConsume, TaggedProvide};
use zzop_core::IoProvide;

/// Every landing constant in `landing.rs`, paired with the name a failure should print. The structural
/// pins below walk this rather than naming four literals, so a fifth constant cannot join the module
/// without answering the two questions this file asks of the other four.
const REGISTERED_LANDINGS: [(&str, &str); 4] = [
    ("CALLER_BREAKAGE_LANDING", CALLER_BREAKAGE_LANDING),
    ("HANDLER_SUBSTITUTION_LANDING", HANDLER_SUBSTITUTION_LANDING),
    ("METHOD_CHANGE_LANDING", METHOD_CHANGE_LANDING),
    ("VENDOR_CONTRACT_LANDING", VENDOR_CONTRACT_LANDING),
];

/// The shared §27 order assertion — DISQUALIFIER, then LANDING, then IMPERATIVE.
///
/// All three must be spelled EXACTLY ONCE, because an index comparison against a needle that occurs
/// twice compares against whichever copy came first and means nothing. `who` names the arm so a failure
/// says which message moved.
fn assert_landing_order(who: &str, msg: &str, disqualifier: &str, landing: &str, imperative: &str) {
    for (name, needle) in [
        ("the disqualifier", disqualifier),
        ("the landing clause", landing),
        ("the imperative", imperative),
    ] {
        assert_eq!(
            msg.matches(needle).count(),
            1,
            "{who}: {name} must be spelled ONCE, or an index comparison means nothing: {msg}"
        );
    }
    let dq = msg.find(disqualifier).expect("disqualifier missing");
    let land = msg.find(landing).expect("landing missing");
    let verb = msg.find(imperative).expect("imperative missing");
    assert!(
        dq < land,
        "{who}: the disqualifier is at {dq} and the landing at {land} -- whether this finding is TRUE \
         is the question a reader answers first: {msg}"
    );
    assert!(
        land < verb,
        "{who}: the landing clause is at {land} and the imperative at {verb} -- a reader who acts on \
         the instruction never reaches the caveat behind it: {msg}"
    );
}

/// No landing may be a substring of another. Every pin here locates its clause with `find`, so one
/// constant contained in another would make the index it returns the wrong clause's index — and the
/// assertion would pass while pinning nothing.
#[test]
fn no_landing_constant_contains_another() {
    for (a_name, a) in REGISTERED_LANDINGS {
        assert!(
            !a.is_empty(),
            "{a_name} is empty -- an empty needle is found at index 0 and every order assertion passes"
        );
        for (b_name, b) in REGISTERED_LANDINGS {
            if a_name == b_name {
                continue;
            }
            assert!(
                !a.contains(b),
                "{a_name} contains {b_name}: an index comparison against {b_name} would silently \
                 return {a_name}'s position"
            );
        }
    }
}

/// Each landing carries facts a rewrite can lose without moving anything. Presence, unlike order, is
/// what a reword drops — and each of these is a claim the sentence has to be able to stand behind.
#[test]
fn every_landing_still_carries_the_facts_it_exists_for() {
    for needle in [
        "gets 404",
        "not type-checked against the route it names",
        "only the trees named in this run's config",
        "serve BOTH names for a release",
    ] {
        assert!(
            CALLER_BREAKAGE_LANDING.contains(needle),
            "CALLER_BREAKAGE_LANDING lost {needle:?}"
        );
    }
    for needle in [
        "moves no address",
        "a DIFFERENT handler answers it",
        "read them side by side",
        "not evidence the answer is the same",
    ] {
        assert!(
            HANDLER_SUBSTITUTION_LANDING.contains(needle),
            "HANDLER_SUBSTITUTION_LANDING lost {needle:?}"
        );
    }
    for needle in [
        "get 405",
        "carries its parameters in the URL",
        "stops being linkable",
        "fixing the call is the edit that costs nothing",
    ] {
        assert!(
            METHOD_CHANGE_LANDING.contains(needle),
            "METHOD_CHANGE_LANDING lost {needle:?}"
        );
    }
    for needle in [
        "NOT YOURS TO EDIT",
        "ONLY as a query parameter",
        "every call returns 401",
        "rotate the key",
    ] {
        assert!(
            VENDOR_CONTRACT_LANDING.contains(needle),
            "VENDOR_CONTRACT_LANDING lost {needle:?}"
        );
    }
}

// ===========================================================================================
// PER-RULE POSITION PINS. Each builds the rule's REAL message from the smallest fixture that
// makes it fire, then asserts DISQUALIFIER < LANDING < IMPERATIVE.
// ===========================================================================================

fn consume(key: &str, source: &str, file: &str, line: u32) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: IoConsume {
            client: None,
            body: None,
            kind: "http".to_string(),
            key: Some(key.to_string()),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
            retry_configured: None,
        },
    }
}

fn provide(key: &str, source: &str, file: &str, line: u32) -> HttpProvideSite {
    HttpProvideSite {
        source: source.to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
    }
}

fn tagged_provide(key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
    TaggedProvide {
        source: source.to_string(),
        provide: IoProvide {
            response: None,
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
            ..Default::default()
        },
    }
}

/// `route-near-miss` — the address-moving family's plainest member: one route, one dimension apart.
#[test]
fn route_near_miss_lands_caller_breakage_before_the_align_imperative() {
    let out = super::route_near_miss::route_near_miss_findings(
        &[consume("GET /api/Articles", "fe", "Api.tsx", 10)],
        &[provide(
            "GET /api/articles",
            "be",
            "articles.controller.ts",
            22,
        )],
    );
    assert_eq!(out.len(), 1);
    assert_landing_order(
        "route-near-miss",
        &out[0].message,
        "verify manually before treating this as drift",
        CALLER_BREAKAGE_LANDING,
        "IF IT IS DRIFT: align the call path with the served route",
    );
}

/// `prefix-drift` — the same noun at N-route magnitude. Magnitude is not a second cost: it belongs to
/// this rule's own imperative, which is where the count is said.
#[test]
fn prefix_drift_lands_caller_breakage_before_the_align_imperative() {
    let records: Vec<PrefixNearMissRecord> = ["articles", "comments", "users"]
        .iter()
        .enumerate()
        .map(|(i, name)| PrefixNearMissRecord {
            consume_source: "fe".to_string(),
            consume_key: format!("GET /{name}"),
            consume_file: "Api.tsx".to_string(),
            consume_line: (i as u32 + 1) * 10,
            provide_source: "be".to_string(),
            provide_key: format!("GET /api/{name}"),
            prefix: "/api".to_string(),
            consume_missing_prefix: true,
        })
        .collect();
    let out = super::prefix_drift::prefix_drift_findings(&records);
    assert_eq!(out.findings.len(), 1);
    assert_landing_order(
        "prefix-drift",
        &out.findings[0].message,
        "Verify manually — a helper/wrapper can make the runtime request differ from the call site",
        CALLER_BREAKAGE_LANDING,
        "IF ONE SIDE IS WRONG: align the base path once",
    );
}

/// `method-mismatch` — the one rule of the six whose repair leaves the address alone. It takes
/// `METHOD_CHANGE_LANDING` and NOT `CALLER_BREAKAGE_LANDING`, because a reader sent to look for a 404
/// on a route that still resolves finds nothing and reads that as safety.
#[test]
fn method_mismatch_lands_the_method_change_before_the_fix_imperative() {
    let out = super::method_mismatch::method_mismatch_findings(
        &[consume("POST /api/users", "fe", "Ctx.tsx", 10)],
        &[provide("GET /api/users", "be", "Api.java", 20)],
    );
    assert_eq!(out.len(), 1);
    assert_landing_order(
        "method-mismatch",
        &out[0].message,
        "verify the literal method manually before changing either side",
        METHOD_CHANGE_LANDING,
        "THEN FIX THE SIDE THAT IS WRONG",
    );
    assert!(
        !out[0].message.contains(CALLER_BREAKAGE_LANDING),
        "method-mismatch must NOT carry the address-moving landing: its repair keeps the path, and a \
         reader told to hunt for 404s finds none and calls the edit safe: {}",
        out[0].message
    );
}

/// `ambiguous-consume` — anchored at a CALL site while the repair it prescribes is on a service the
/// reader may not own, which is why the landing's caller-enumeration sentence is the right one.
#[test]
fn ambiguous_consume_lands_caller_breakage_before_the_disambiguate_imperative() {
    let entries = vec![AmbiguousConsume {
        source: "gateway".to_string(),
        consume: IoConsume {
            client: None,
            body: None,
            kind: "http".to_string(),
            key: Some("GET /health".to_string()),
            file: "gw.ts".to_string(),
            line: 1,
            raw: None,
            method: None,
            retry_configured: None,
        },
        candidates: vec![
            tagged_provide("GET /health", "svc-a", "svc-a/health.ts", 3),
            tagged_provide("GET /health", "svc-b", "svc-b/health.ts", 7),
        ],
    }];
    let out = super::ambiguous_consume::ambiguous_consume_findings(&entries);
    assert_eq!(out.len(), 1);
    assert_landing_order(
        "ambiguous-consume",
        &out[0].message,
        "there is nothing here to repair",
        CALLER_BREAKAGE_LANDING,
        "IF IT IS NOT DELIBERATE: disambiguate the route",
    );
}

/// `cross-layer/duplicate-route` — TWO landings, because its two ways out cost different things. The
/// second assertion is the one that made this module hold more than one sentence.
#[test]
fn duplicate_route_lands_both_costs_each_before_its_own_way_out() {
    let cl = CrossLayerResult {
        unconsumed_provides: vec![
            tagged_provide("DELETE /api/me", "svc-a", "a.ts", 3),
            tagged_provide("DELETE /api/me", "svc-b", "b.ts", 9),
        ],
        ..Default::default()
    };
    let out = super::duplicate_route::cross_layer_duplicate_route_findings(&cl);
    assert_eq!(out.len(), 2);
    let msg = &out[0].message;
    assert_landing_order(
        "duplicate-route (namespace branch)",
        msg,
        "confirm that before you edit anything",
        CALLER_BREAKAGE_LANDING,
        "IF YOU KEEP BOTH HANDLERS: namespace the routes apart",
    );
    assert_landing_order(
        "duplicate-route (merge branch)",
        msg,
        "IF YOU KEEP BOTH HANDLERS: namespace the routes apart",
        HANDLER_SUBSTITUTION_LANDING,
        "IF YOU MERGE THEM INSTEAD",
    );
}

/// `cross-layer/route-shadowing` — the same two-cost shape, and the reason the substitution sentence
/// names a whole router rather than one pair: reordering a first-match gateway decides every other
/// literal/pattern collision it holds too.
#[test]
fn cross_tree_route_shadowing_lands_both_costs_each_before_its_own_way_out() {
    let out = super::cross_tree_route_shadowing::cross_tree_route_shadowing_findings(&[
        provide("GET /users/{}", "be", "be/routes.rs", 10),
        provide("GET /users/export", "gw", "gw/routes.ts", 20),
    ]);
    assert_eq!(out.len(), 1);
    let msg = &out[0].message;
    assert_landing_order(
        "route-shadowing (prefix branch)",
        msg,
        "confirm that first",
        CALLER_BREAKAGE_LANDING,
        "IF YOU SEPARATE THEM: mount each source under its own path prefix",
    );
    assert_landing_order(
        "route-shadowing (gateway-order branch)",
        msg,
        "IF YOU SEPARATE THEM: mount each source under its own path prefix",
        HANDLER_SUBSTITUTION_LANDING,
        "IF YOU REORDER THE GATEWAY INSTEAD",
    );
}

/// `external-secret-in-url` — the refusal half of §37 as much as the pin: this rule's subject is an
/// EXTERNAL host, so it must carry `VENDOR_CONTRACT_LANDING` and must NOT carry the internal-route one.
#[test]
fn external_secret_in_url_lands_the_vendor_contract_before_the_move_imperative() {
    let out = super::external_secret_in_url::external_secret_in_url_findings(
        &[consume(
            "GET https://api.vendor.com/v1/users?token=abc123",
            "fe",
            "Client.ts",
            10,
        )],
        super::external_secret_in_url::SECRET_PARAM_NAMES,
    );
    assert_eq!(out.len(), 1);
    assert_landing_order(
        "external-secret-in-url",
        &out[0].message,
        "read the value before you treat this as a leak",
        VENDOR_CONTRACT_LANDING,
        "IF THE VENDOR ACCEPTS IT ELSEWHERE: move",
    );
    assert!(
        !out[0].message.contains(CALLER_BREAKAGE_LANDING),
        "an egress rule must not carry the internal-route landing -- nothing here is the reader's \
         address to move: {}",
        out[0].message
    );
}
