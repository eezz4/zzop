//! Coverage for `sensitive_response_field_findings`: the vocabulary's three match axes (substring/
//! exact/suffix) with their FP-adversarial negatives, the consumed->critical escalation, the
//! unresolved-ref and empty-field skips, determinism, and site-key dedupe.
use super::*;
use zzop_core::io::{EdgeFrom, EdgeTo};
use zzop_core::ProvideBodyField;

fn site(source: &str, key: &str, file: &str, line: u32, fields: &[&str]) -> ResponseProvideSite {
    ResponseProvideSite {
        source: source.to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
        response: ProvideResponseShape {
            dto_ref: None,
            fields: fields
                .iter()
                .map(|n| ProvideBodyField {
                    name: n.to_string(),
                    optional: false,
                })
                .collect(),
            complete: true,
        },
    }
}

fn http_edge_to(key: &str, source: &str, file: &str, line: u32) -> CrossLayerEdge {
    CrossLayerEdge {
        kind: "http".to_string(),
        key: key.to_string(),
        from: EdgeFrom {
            source: "fe".to_string(),
            file: "api.ts".to_string(),
            line: 1,
        },
        to: EdgeTo {
            source: source.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        },
        cross_source: true,
        low_confidence_reason: None,
    }
}

#[test]
fn substring_exact_and_suffix_axes_each_fire() {
    let sites = vec![
        site("be", "GET /a", "a.ts", 1, &["passwordHash"]), // substring: password
        site("be", "GET /b", "b.ts", 1, &["token"]),        // exact: token
        site("be", "GET /c", "c.ts", 1, &["refreshToken"]), // suffix: token
        site("be", "GET /d", "d.ts", 1, &["client_secret"]), // substring through normalization
    ];
    let out = sensitive_response_field_findings(&sites, &[], SensitiveResponseVocab::built_in());
    assert_eq!(out.len(), 4, "{out:?}");
    assert!(out.iter().all(|f| f.severity == Severity::Warning));
    assert!(out
        .iter()
        .all(|f| f.rule_id == "cross-layer/sensitive-response-field"));
    assert!(out.iter().all(|f| f.message.contains("disabledRules")));
}

#[test]
fn fp_adversarial_benign_names_stay_silent() {
    // The nearest benign look-alikes of each axis: `tokenCount`/`tokenizer` (suffix must not become
    // substring), `contentHash`/`commitHash` (hash is exact-only), `publicKey`/`sortKey` (no bare
    // `key` token at all), `saltedCaramelRating` is a real substring hit for `salt` — excluded from
    // this negative set on purpose (the substring list accepts that tradeoff for `salt`).
    let sites = vec![site(
        "be",
        "GET /a",
        "a.ts",
        1,
        &[
            "id",
            "email",
            "displayName",
            "tokenCount",
            "tokenizer",
            "contentHash",
            "commitHash",
            "publicKey",
            "sortKey",
            "createdAt",
        ],
    )];
    assert!(
        sensitive_response_field_findings(&sites, &[], SensitiveResponseVocab::built_in())
            .is_empty()
    );
}

/// The built-in vocabulary's KNOWN BOUNDARY, pinned in both directions. Every name here was measured
/// against this repo's own corpus (513 response fields) and the values were then deliberately left
/// alone — moving them trades one error class for another (`session` as a substring opens
/// `sessionCount`), and a project that disagrees declares its own list, which replaces the built-in
/// whole. So the boundary is DISCLOSED rather than fixed, in `docs/rules/catalog.md`.
///
/// This test is what makes the disclosure honest. It is a documentation pin, not a correctness pin:
/// it asserts the CURRENT behaviour of names the catalog names out loud, so that a future edit which
/// quietly makes the heuristic smarter turns this red and forces the published sentence to move with
/// it. Without it the catalog's boundary paragraph would be prose nothing checks.
///
/// It also closes the one axis the FP-adversarial negative above cannot reach. That test's names
/// (`tokenCount`, `contentHash`, `publicKey`) are all exact/suffix look-alikes, so they pass
/// STRUCTURALLY — none of them can fail a substring rule. These do: `passwordChangedAt` and
/// `hasPassword` are credential METADATA (a timestamp, a boolean) that the `password` substring
/// cannot tell apart from a credential, and `credentialsRequired` is a route-policy flag.
#[test]
fn the_built_in_vocabularys_measured_boundary_is_pinned_in_both_directions() {
    let fires = |field: &str| {
        !sensitive_response_field_findings(
            &[site("be", "GET /a", "a.ts", 1, &[field])],
            &[],
            SensitiveResponseVocab::built_in(),
        )
        .is_empty()
    };

    // FALSE POSITIVES the built-in accepts. Credential metadata caught by the `password`/`credential`
    // SUBSTRINGS, and pagination cursors caught by the `token` SUFFIX — a cursor is opaque state, not
    // a secret, but it is spelled exactly like one.
    for name in [
        "passwordChangedAt",
        "hasPassword",
        "credentialsRequired",
        "nextPageToken",
        "continuationToken",
    ] {
        assert!(
            fires(name),
            "{name} is a DISCLOSED false positive of the built-in vocabulary — if it stopped firing, \
             the known-boundary section of docs/rules/catalog.md has to stop saying it does"
        );
    }

    // FALSE NEGATIVES the built-in accepts. Real credential-bearing names that no axis reaches:
    // `session`/`auth`/`cookie` are not substrings, none is an exact token, and none ends in `token`.
    for name in ["sessionId", "authorization", "cookie"] {
        assert!(
            !fires(name),
            "{name} is a DISCLOSED false negative of the built-in vocabulary — if it started firing, \
             the known-boundary section of docs/rules/catalog.md has to stop saying it does not"
        );
    }
}

#[test]
fn consumed_route_escalates_to_critical_with_consumer_count() {
    let sites = vec![site("be", "GET /me", "c.ts", 7, &["passwordHash"])];
    let edges = vec![
        http_edge_to("GET /me", "be", "c.ts", 7),
        http_edge_to("GET /me", "be", "c.ts", 7),
    ];
    let out = sensitive_response_field_findings(&sites, &edges, SensitiveResponseVocab::built_in());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Critical);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["consumed"], true);
    assert_eq!(data["consumerCount"], 2);
    assert!(
        out[0].message.contains("2 call sites"),
        "{}",
        out[0].message
    );
}

/// Seals the consumer-map key: an array-path decorator (`@Get(['me','profile'])`) emits TWO provide
/// sites from ONE `(source, file, line)`, so a site-only consumer lookup would inherit `/users/me`'s
/// edge onto the never-consumed `/users/profile` — falsely critical + consumed:true. Only the key
/// the edge actually matched may escalate.
#[test]
fn consumer_escalation_does_not_leak_to_sibling_routes_on_the_same_line() {
    let sites = vec![
        site(
            "be",
            "GET /users/me",
            "user.controller.ts",
            9,
            &["passwordHash"],
        ),
        site(
            "be",
            "GET /users/profile",
            "user.controller.ts",
            9,
            &["passwordHash"],
        ),
    ];
    let edges = vec![http_edge_to("GET /users/me", "be", "user.controller.ts", 9)];
    let out = sensitive_response_field_findings(&sites, &edges, SensitiveResponseVocab::built_in());
    assert_eq!(out.len(), 2, "{out:?}");
    let by_key = |k: &str| {
        out.iter()
            .find(|f| f.data.as_ref().unwrap()["key"] == k)
            .unwrap()
    };
    let me = by_key("GET /users/me");
    assert_eq!(me.severity, Severity::Critical);
    assert_eq!(me.data.as_ref().unwrap()["consumed"], true);
    assert_eq!(me.data.as_ref().unwrap()["consumerCount"], 1);
    let profile = by_key("GET /users/profile");
    assert_eq!(profile.severity, Severity::Warning, "{profile:?}");
    assert_eq!(profile.data.as_ref().unwrap()["consumed"], false);
    assert!(profile
        .data
        .as_ref()
        .unwrap()
        .get("consumerCount")
        .is_none());
}

#[test]
fn non_http_edge_does_not_escalate() {
    let sites = vec![site("be", "GET /me", "c.ts", 7, &["token"])];
    let mut edge = http_edge_to("GET /me", "be", "c.ts", 7);
    edge.kind = "trpc".to_string();
    let out =
        sensitive_response_field_findings(&sites, &[edge], SensitiveResponseVocab::built_in());
    assert_eq!(out[0].severity, Severity::Warning);
}

#[test]
fn unresolved_dto_ref_and_empty_fields_are_skipped() {
    let mut unresolved = site("be", "GET /a", "a.ts", 1, &["passwordHash"]);
    unresolved.response.dto_ref = Some("Leaked".to_string());
    let empty = site("be", "GET /b", "b.ts", 1, &[]);
    assert!(sensitive_response_field_findings(
        &[unresolved, empty],
        &[],
        SensitiveResponseVocab::built_in()
    )
    .is_empty());
}

#[test]
fn duplicate_site_key_pairs_are_deduped_and_output_is_sorted() {
    let sites = vec![
        site("be", "GET /b", "b.ts", 2, &["secret"]),
        site("be", "GET /a", "a.ts", 9, &["secret"]),
        site("be", "GET /a", "a.ts", 9, &["secret"]), // producer duplication -> one finding
    ];
    let out = sensitive_response_field_findings(&sites, &[], SensitiveResponseVocab::built_in());
    assert_eq!(out.len(), 2, "{out:?}");
    assert_eq!((out[0].file.as_str(), out[0].line), ("a.ts", 9));
    assert_eq!((out[1].file.as_str(), out[1].line), ("b.ts", 2));
}

#[test]
fn message_names_fields_sorted_and_deduped_and_states_the_substrate_boundary() {
    let sites = vec![site(
        "be",
        "GET /me",
        "c.ts",
        1,
        &["token", "passwordHash", "email"],
    )];
    let out = sensitive_response_field_findings(&sites, &[], SensitiveResponseVocab::built_in());
    assert_eq!(out.len(), 1);
    let m = &out[0].message;
    assert!(m.contains("`passwordHash, token`"), "{m}");
    assert!(!m.contains("email"), "clean fields are not listed: {m}");
    assert!(
        m.contains("literal secret VALUES"),
        "the security-pack substrate boundary is stated: {m}"
    );
    assert!(
        m.contains("declared field NAME only"),
        "the name-evidence honesty bound is stated: {m}"
    );
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(
        data["sensitiveFields"],
        serde_json::json!(["passwordHash", "token"])
    );
}

#[test]
fn undeclared_vocabulary_makes_no_judgment() {
    // The declared-or-not-judged contract every vocabulary key follows: the SAME site that fires
    // under the built-in vocabulary is silent when nothing is declared (all three axes empty).
    let sites = vec![site("be", "GET /a", "a.ts", 1, &["passwordHash"])];
    let empty = SensitiveResponseVocab {
        substrings: &[],
        exact_names: &[],
        suffixes: &[],
    };
    assert_eq!(
        sensitive_response_field_findings(&sites, &[], SensitiveResponseVocab::built_in()).len(),
        1
    );
    assert!(sensitive_response_field_findings(&sites, &[], empty).is_empty());
}

/// §27 pin — POSITION, not existence, for the removal landing (2026-08-30).
///
/// This rule already satisfied §27 for the question *is this finding true*: the disqualifier ("the
/// value may be benign ... an auth route returning a token can be by design") sits ahead of a
/// CONDITIONED imperative ("Verify ...; if not, remove it"). What was absent is the third leg for the
/// reader who answers "no, it does not belong" — the message never said the edit is class-scoped while
/// the finding is route-scoped, and never said that nothing fails to build afterwards.
///
/// INVALIDATION PROBE: move `REMOVAL_LANDING` behind `Verify the field belongs` in `message.rs` and
/// every token of it is still present — only the ordering assertions here go red.
#[test]
fn the_removal_landing_precedes_the_imperative() {
    let sites = vec![site("be", "GET /me", "c.ts", 7, &["passwordHash"])];
    let out = sensitive_response_field_findings(&sites, &[], SensitiveResponseVocab::built_in());
    assert_eq!(out.len(), 1);
    let msg = &out[0].message;
    let disq = msg
        .find("Evidence is the declared field NAME only")
        .expect("the disqualifier is missing from the message");
    let landing = msg
        .find("WHAT REMOVAL COSTS IS NOT SCOPED")
        .expect("the removal landing is missing from the message");
    let verb = msg
        .find("Verify the field belongs")
        .expect("the imperative is missing from the message");
    assert!(
        disq < landing && landing < verb,
        "order must be disqualifier({disq}) < landing({landing}) < imperative({verb}): {msg}"
    );
    // Each spelled ONCE, or an index comparison means nothing.
    for needle in [
        "WHAT REMOVAL COSTS IS NOT SCOPED",
        "Verify the field belongs",
    ] {
        assert_eq!(
            msg.matches(needle).count(),
            1,
            "`{needle}` must appear exactly once: {msg}"
        );
    }
}

/// The CRITICAL arm names the witnessed consumers as the population the edit breaks — the same count
/// that escalated the finding. Its absence on the warning arm is designed, not an oversight: with no
/// witnessed consumer there is no set to point at, and the exposure clause already says callers
/// outside this analysis stay invisible.
#[test]
fn only_the_critical_arm_names_the_witnessed_consumers_as_the_broken_population() {
    let sites = vec![site("be", "GET /me", "c.ts", 7, &["passwordHash"])];
    let edges = vec![
        http_edge_to("GET /me", "be", "c.ts", 7),
        http_edge_to("GET /me", "be", "c.ts", 7),
    ];
    let hot = sensitive_response_field_findings(&sites, &edges, SensitiveResponseVocab::built_in());
    assert_eq!(hot.len(), 1);
    assert_eq!(hot[0].severity, Severity::Critical);
    // Verb-agnostic needle, because agreement follows the count and the singular arm is the common
    // one (the fixture's own critical finding has exactly one consumer).
    let needle = "that population";
    assert!(
        hot[0]
            .message
            .contains("The 2 witnessed call sites counted above ARE that population"),
        "the critical arm must name the witnessed consumers as what the edit breaks: {}",
        hot[0].message
    );
    // And it must sit AHEAD of the imperative too — it is part of the landing, not a footnote.
    let tail = hot[0].message.find(needle).expect("tail present");
    let verb = hot[0]
        .message
        .find("Verify the field belongs")
        .expect("imperative present");
    assert!(
        tail < verb,
        "tail at {tail} must precede the imperative at {verb}"
    );

    let cold = sensitive_response_field_findings(&sites, &[], SensitiveResponseVocab::built_in());
    assert_eq!(cold[0].severity, Severity::Warning);
    assert!(
        !cold[0].message.contains(needle),
        "the warning arm has no witnessed consumer set to name: {}",
        cold[0].message
    );
}
