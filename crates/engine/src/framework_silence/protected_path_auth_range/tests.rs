//! S18's tests. Split into its own file so the tripwire and the pins that keep its three mirrored
//! constants honest can each be read whole.

use super::*;
use zzop_core::RuleConfig;

/// The shipped `http` pack's JSON, read from the repo rather than from a copy — the whole point of the
/// pins below is that they fail when the pack and this module disagree, which a copy cannot do.
fn http_pack() -> serde_json::Value {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl/http/http.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("shipped http pack must be readable at {path:?}: {e}"));
    serde_json::from_str(&text).expect("shipped http pack must be valid JSON")
}

fn affected_rule_matcher() -> serde_json::Value {
    let pack = http_pack();
    let short_id = AFFECTED_RULE_ID
        .split_once('/')
        .expect("the affected rule id is `<pack>/<rule>`")
        .1;
    pack["rules"]
        .as_array()
        .expect("the pack has a rules array")
        .iter()
        .find(|r| r["id"].as_str() == Some(short_id))
        .unwrap_or_else(|| panic!("{AFFECTED_RULE_ID} is not in the shipped http pack"))["matcher"]
        .clone()
}

fn provide(file: &str, key: &str) -> IoProvide {
    IoProvide {
        response: None,
        kind: "http".to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line: 1,
        symbol: None,
        body: None,
        ..Default::default()
    }
}

/// An all-on gate — what an ordinary run has, and what every test below except the two gate tests uses.
fn on() -> RuleConfig {
    RuleConfig::default()
}

// --- the three pins: this module mirrors the rule, so it must fail when the rule moves ------------

/// The id this warning publishes must be a rule the shipped pack really carries. Same pin
/// `orm_schema_silence` grew after shipping without one; that one reads the native registry because its
/// rule is native, this one reads the pack because its rule is DSL.
#[test]
fn the_affected_rule_id_is_a_real_shipped_dsl_rule() {
    let pack = http_pack();
    assert_eq!(pack["id"].as_str(), Some(AFFECTED_PACK_ID));
    // `affected_rule_matcher` panics with the same message if the rule is gone; calling it IS the pin.
    let matcher = affected_rule_matcher();
    assert_eq!(
        matcher["type"].as_str(),
        Some("io-scan"),
        "S18's gate mirrors an io-scan population; if the rule changed matcher kind the gate is wrong"
    );
}

/// [`SCOPED_EXTENSIONS`] must be exactly the extension set the rule's own `file_pattern` names — in
/// BOTH directions. Drift either way is a wrong disclosure: an extension here that the rule does not
/// scan discloses a blind spot on routes it never evaluates, and one the rule scans but this list omits
/// leaves the disclosure silent exactly where it is needed.
#[test]
fn the_scope_matches_the_shipped_rule() {
    let matcher = affected_rule_matcher();
    let pattern = matcher["file_pattern"]
        .as_str()
        .expect("the affected rule declares a file_pattern");
    let alternation = pattern
        .split_once("(?:")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inner, _)| inner)
        .unwrap_or_else(|| {
            panic!("file_pattern shape changed, cannot read its extensions: {pattern}")
        });
    let mut from_pack: Vec<&str> = alternation.split('|').collect();
    from_pack.sort_unstable();
    let mut mirrored: Vec<&str> = SCOPED_EXTENSIONS.to_vec();
    mirrored.sort_unstable();
    assert_eq!(
        mirrored, from_pack,
        "SCOPED_EXTENSIONS and the rule's file_pattern disagree — one of the two moved"
    );
}

/// Same pin for the protected segments, against the rule's `key_pattern`.
#[test]
fn the_protected_segments_match_the_shipped_rule() {
    let matcher = affected_rule_matcher();
    let pattern = matcher["key_pattern"]
        .as_str()
        .expect("the affected rule declares a key_pattern");
    for seg in PROTECTED_SEGMENTS {
        assert!(
            pattern.contains(seg),
            "key_pattern {pattern} does not name the segment {seg} this module mirrors"
        );
    }
    // The reverse leg: a segment the rule gained must not stay unmirrored here. The pattern's
    // alternation is the authority.
    let alternation = pattern
        .split_once("/(")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inner, _)| inner)
        .unwrap_or_else(|| panic!("key_pattern shape changed: {pattern}"));
    let mut from_pack: Vec<&str> = alternation.split('|').collect();
    from_pack.sort_unstable();
    let mut mirrored: Vec<&str> = PROTECTED_SEGMENTS.to_vec();
    mirrored.sort_unstable();
    assert_eq!(
        mirrored, from_pack,
        "PROTECTED_SEGMENTS drifted from key_pattern"
    );
}

// --- behaviour ------------------------------------------------------------------------------------

#[test]
fn a_protected_route_in_a_scanned_language_discloses_the_range() {
    let w = protected_path_auth_range_warning(
        &[
            provide(
                "src/controllers/user-admin.controller.ts",
                "GET /api/admin/users",
            ),
            provide(
                "src/controllers/user-admin.controller.ts",
                "POST /api/admin/users",
            ),
            provide("src/controllers/album.controller.ts", "GET /api/albums"),
        ],
        &on(),
    )
    .expect("protected-path routes in a scanned language must disclose");
    assert!(w.contains("2 http route(s)"), "{w}");
    assert!(w.contains(AFFECTED_RULE_ID), "{w}");
    // The three idioms are the whole reason this exists; a disclosure that names none says nothing.
    // One was measured in each of the three corpus trees that fire the affected rule, so dropping any
    // of them would leave one of those trees reading a disclosure that does not describe it.
    assert!(w.contains("APP_GUARD"), "{w}");
    assert!(w.contains("applyDecorators"), "{w}");
    assert!(w.contains("authedAdminProcedure"), "{w}");
    // The remedy must be reachable without writing an adapter.
    assert!(w.contains("zzop-protected-path-no-auth-evidence-ok"), "{w}");
}

/// The gate is the RULE's range, not "this tree has routes" — an ordinary route is outside it.
#[test]
fn a_tree_with_no_protected_path_route_stays_silent() {
    assert!(protected_path_auth_range_warning(
        &[
            provide("src/api.ts", "GET /api/users"),
            provide("src/api.ts", "POST /api/albums"),
        ],
        &on()
    )
    .is_none());
}

/// Go and C# are deliberately outside the rule's language scope (no `auth-guarded` producer), so a Go
/// `/admin/` route is outside this disclosure's business too. If this ever goes red because the rule
/// re-widened, S18's `SCOPED_EXTENSIONS` pin above went red first and named the reason.
#[test]
fn a_protected_route_in_an_unscanned_language_is_not_this_disclosures_business() {
    assert!(protected_path_auth_range_warning(
        &[provide("cmd/server/main.go", "GET /admin/users")],
        &on()
    )
    .is_none());
}

/// A path segment that merely STARTS with a protected word is not a protected segment — the same
/// whole-segment judgment the rule's `key_pattern` makes with its `(/|$)` tail.
#[test]
fn a_longer_word_containing_a_protected_segment_does_not_trip_it() {
    assert!(protected_path_auth_range_warning(
        &[provide("src/api.ts", "GET /api/administrators")],
        &on()
    )
    .is_none());
    assert!(
        protected_path_auth_range_warning(&[provide("src/api.ts", "GET /api/admin")], &on())
            .is_some()
    );
}

/// Naming a rule the user switched off is a worse answer than naming none — both the per-rule gate and
/// the pack gate have to close this disclosure.
#[test]
fn a_disabled_rule_or_pack_closes_the_disclosure() {
    let route = [provide("src/api.ts", "GET /api/admin/users")];
    assert!(protected_path_auth_range_warning(&route, &on()).is_some());

    let rule_off = RuleConfig {
        disabled_rules: vec![AFFECTED_RULE_ID.to_string()],
        ..RuleConfig::default()
    };
    assert!(
        protected_path_auth_range_warning(&route, &rule_off).is_none(),
        "a disabled rule must not be named"
    );

    let pack_off = RuleConfig {
        disabled_rules: vec![AFFECTED_PACK_ID.to_string()],
        ..RuleConfig::default()
    };
    assert!(
        protected_path_auth_range_warning(&route, &pack_off).is_none(),
        "a disabled pack must not be named"
    );

    // The allowlist half of the pack axis — a run that opted into other packs only.
    let other_only = RuleConfig {
        only_packs: vec!["security".to_string()],
        ..RuleConfig::default()
    };
    assert!(
        protected_path_auth_range_warning(&route, &other_only).is_none(),
        "a pack outside an allowlist must not be named"
    );
}

/// A non-http provide is not this rule's population.
#[test]
fn a_non_http_provide_does_not_trip_it() {
    let mut p = provide("src/db.ts", "admin_users");
    p.kind = "db-table".to_string();
    assert!(protected_path_auth_range_warning(&[p], &on()).is_none());
}
