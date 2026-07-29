//! Coverage for `apply_config_client_base` — the config-declared calling-side base.

use super::apply_config_client_base;
use zzop_core::IoConsume;

fn consume(kind: &str, key: Option<&str>, client: Option<&str>) -> IoConsume {
    IoConsume {
        kind: kind.to_string(),
        key: key.map(str::to_string),
        file: "src/api.ts".to_string(),
        line: 1,
        raw: None,
        method: None,
        retry_configured: None,
        body: None,
        client: client.map(str::to_string),
    }
}

fn keys(consumes: &[IoConsume]) -> Vec<Option<&str>> {
    consumes.iter().map(|c| c.key.as_deref()).collect()
}

/// The shape the knob exists for: a front end whose calls carry only the suffix because its base is a
/// cross-file constant. After declaring it, the keys match what the backend serves.
#[test]
fn a_declared_base_is_prepended_to_every_keyed_relative_http_consume() {
    let mut consumes = vec![
        consume("http", Some("GET /articles"), None),
        consume("http", Some("POST /articles"), Some("axios")),
    ];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api"), &[], &mut warnings);
    assert_eq!(
        keys(&consumes),
        vec![Some("GET /api/articles"), Some("POST /api/articles")]
    );
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// Not scoped by `client`, unlike the sentinel pass: a config declaration is the author's statement about
/// the whole tree, the same scope `mountedAt` has on the serving side.
#[test]
fn it_applies_regardless_of_which_client_the_call_came_from() {
    let mut consumes = vec![
        consume("http", Some("GET /a"), Some("axios")),
        consume("http", Some("GET /b"), Some("generated")),
        consume("http", Some("GET /c"), None),
    ];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api"), &[], &mut warnings);
    assert_eq!(
        keys(&consumes),
        vec![Some("GET /api/a"), Some("GET /api/b"), Some("GET /api/c")]
    );
}

/// Never guessed, never wrong-channel: an unresolved consume stays unresolved, an absolute URL is not a
/// thing a client base applies to, and a non-http consume is not this pass's business.
#[test]
fn unresolved_absolute_and_non_http_consumes_are_left_alone() {
    let mut consumes = vec![
        consume("http", None, None),
        consume("http", Some("GET https://vendor.example.com/x"), None),
        consume("db-table", Some("table:users"), None),
    ];
    let before = keys(&consumes)
        .into_iter()
        .map(|k| k.map(str::to_string))
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api"), &[], &mut warnings);
    assert_eq!(
        keys(&consumes),
        before.iter().map(|k| k.as_deref()).collect::<Vec<_>>()
    );
    // Nothing was rewritten, so the zero-effect tripwire fires rather than staying silent.
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("no effect"), "{}", warnings[0]);
}

/// A declared knob that moved nothing is almost always stale or on the wrong tree; silence would let it
/// look effective.
#[test]
fn a_declaration_that_rewrites_nothing_warns() {
    let mut consumes = vec![consume("db-table", Some("table:users"), None)];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api"), &[], &mut warnings);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("clientBase"), "{}", warnings[0]);
}

/// THE REFUTATION THAT SET THE SEMANTICS (`cases/trees/fe-axios`, 2026-07-29). One tree reaches its own
/// API two ways: a browser client whose base is implicit, and server-side rendering calling `fetch` with
/// the FULL path. The knob's scope covers both, so a blind prepend would re-key every already-correct call
/// to `/api/api/...` — trading this knob's own failure for the same failure pointed the other way.
#[test]
fn a_mixed_tree_prefixes_the_suffix_calls_and_leaves_the_full_path_calls_alone() {
    let mut consumes = vec![
        consume("http", Some("GET /articles"), Some("axios")),
        consume("http", Some("GET /api/articles"), None),
        consume("http", Some("POST /api/articles/drafts"), None),
    ];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api"), &[], &mut warnings);
    assert_eq!(
        keys(&consumes),
        vec![
            Some("GET /api/articles"),
            Some("GET /api/articles"),
            Some("POST /api/articles/drafts")
        ]
    );
    // A partial apply IS the normal shape — warning here would fire on the common case.
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// Idempotence must not degrade into prefix-substring matching: `/api` is not a base of `/apiv2/...`.
/// A guard that skipped on a bare `starts_with` would silently refuse to fix exactly the tree it was
/// declared for.
#[test]
fn the_already_under_check_respects_segment_boundaries() {
    let mut consumes = vec![
        consume("http", Some("GET /apiv2/users"), None),
        consume("http", Some("GET /api"), None),
    ];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api"), &[], &mut warnings);
    assert_eq!(
        keys(&consumes),
        vec![Some("GET /api/apiv2/users"), Some("GET /api")]
    );
}

/// Stacking cannot happen any more — the code-extracted pass already made those keys carry the base, so
/// they are skipped. What remains is the useful half of the old stacking warning: the declaration is a
/// duplicate of something zzop could read for itself, and it says which client it read it from.
#[test]
fn a_declaration_that_duplicates_a_code_extracted_base_is_a_no_op_that_names_the_client() {
    let mut consumes = vec![consume("http", Some("GET /api/articles"), Some("axios"))];
    let mut warnings = Vec::new();
    apply_config_client_base(
        &mut consumes,
        Some("/api"),
        &["axios".to_string()],
        &mut warnings,
    );
    assert_eq!(keys(&consumes), vec![Some("GET /api/articles")]);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("axios"), "{}", warnings[0]);
    assert!(warnings[0].contains("no effect"), "{}", warnings[0]);
}

/// The same no-op, but with no code-extracted base to blame — the call sites simply write the base
/// themselves, so the message must not invent a client that never applied anything.
#[test]
fn a_duplicate_declaration_with_no_extracted_base_blames_the_call_sites() {
    let mut consumes = vec![consume("http", Some("GET /api/articles"), None)];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api"), &[], &mut warnings);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("call sites already write the base"),
        "{}",
        warnings[0]
    );
}

/// The mapper is the fail-fast gate, but this crate is also a library an embedder builds requests for by
/// hand — a malformed value must degrade with a warning, never rewrite keys into nonsense.
#[test]
fn a_malformed_declared_base_rewrites_nothing_and_warns() {
    for bad in ["api", "https://x.example.com/api", "/api/{}"] {
        let mut consumes = vec![consume("http", Some("GET /articles"), None)];
        let mut warnings = Vec::new();
        apply_config_client_base(&mut consumes, Some(bad), &[], &mut warnings);
        assert_eq!(
            keys(&consumes),
            vec![Some("GET /articles")],
            "{bad} must not rewrite"
        );
        assert_eq!(warnings.len(), 1, "{bad}: {warnings:?}");
    }
}

#[test]
fn no_declaration_is_a_no_op_with_no_warning() {
    let mut consumes = vec![consume("http", Some("GET /articles"), None)];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, None, &[], &mut warnings);
    assert_eq!(keys(&consumes), vec![Some("GET /articles")]);
    assert!(warnings.is_empty());
}

/// A trailing slash in the declaration must not produce a doubled separator.
#[test]
fn a_trailing_slash_in_the_declaration_is_normalized() {
    let mut consumes = vec![consume("http", Some("GET /articles"), None)];
    let mut warnings = Vec::new();
    apply_config_client_base(&mut consumes, Some("/api/"), &[], &mut warnings);
    assert_eq!(keys(&consumes), vec![Some("GET /api/articles")]);
}
