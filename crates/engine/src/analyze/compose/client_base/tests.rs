//! Coverage for `apply_client_base_prefixes`: the single-prefix apply, client-scoping (a
//! differently- or un-tagged consume is untouched), the external/absolute-URL and unresolved
//! never-touch gates, non-`http`-kind gating, the conflicting-sentinels honest degrade, the
//! same-path duplicate-sentinel no-warning case, and the deliberate double-prefix idempotence
//! pin.
use super::*;

fn sentinel(path: &str, client: &str, file: &str, line: u32) -> IoConsume {
    IoConsume {
        client: Some(client.to_string()),
        body: None,
        kind: "client-base-prefix".to_string(),
        key: Some(path.to_string()),
        file: file.to_string(),
        line,
        raw: None,
        method: None,
    }
}

fn http_consume(key: &str, client: Option<&str>, file: &str, line: u32) -> IoConsume {
    IoConsume {
        client: client.map(str::to_string),
        body: None,
        kind: "http".to_string(),
        key: Some(key.to_string()),
        file: file.to_string(),
        line,
        raw: None,
        method: None,
    }
}

fn unresolved_consume(client: Option<&str>, raw: &str, file: &str, line: u32) -> IoConsume {
    IoConsume {
        client: client.map(str::to_string),
        body: None,
        kind: "http".to_string(),
        key: None,
        file: file.to_string(),
        line,
        raw: Some(raw.to_string()),
        method: Some("GET".to_string()),
    }
}

#[test]
fn single_prefix_rewrites_axios_tagged_http_consumes_and_strips_the_sentinel() {
    let mut consumes = vec![
        sentinel("/api", "axios", "src/bootstrap.ts", 3),
        http_consume("GET /users", Some("axios"), "src/api/users.ts", 10),
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes.len(), 1);
    assert_eq!(consumes[0].key.as_deref(), Some("GET /api/users"));
    assert!(warnings.is_empty());
    assert!(consumes.iter().all(|c| c.kind != "client-base-prefix"));
}

#[test]
fn non_axios_tagged_consume_is_untouched() {
    // `client: None` and `client: Some("fetch")` both must be left alone — the prefix is
    // scoped to the SAME client tag the sentinel names.
    let mut consumes = vec![
        sentinel("/api", "axios", "src/bootstrap.ts", 3),
        http_consume("GET /users", None, "src/a.ts", 1),
        http_consume("GET /orders", Some("fetch"), "src/b.ts", 2),
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes.len(), 2);
    assert_eq!(consumes[0].key.as_deref(), Some("GET /users"));
    assert_eq!(consumes[1].key.as_deref(), Some("GET /orders"));
}

#[test]
fn absolute_url_key_is_untouched() {
    let mut consumes = vec![
        sentinel("/api", "axios", "src/bootstrap.ts", 3),
        http_consume("GET https://x.io/users", Some("axios"), "src/a.ts", 1),
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes.len(), 1);
    assert_eq!(consumes[0].key.as_deref(), Some("GET https://x.io/users"));
}

#[test]
fn unresolved_consume_key_is_untouched() {
    let mut consumes = vec![
        sentinel("/api", "axios", "src/bootstrap.ts", 3),
        unresolved_consume(Some("axios"), "axios.get(url)", "src/a.ts", 1),
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes.len(), 1);
    assert_eq!(consumes[0].key, None);
    assert_eq!(consumes[0].raw.as_deref(), Some("axios.get(url)"));
}

#[test]
fn conflicting_sentinels_apply_nothing_and_warn_once_naming_both() {
    let mut consumes = vec![
        sentinel("/api", "axios", "src/a.ts", 1),
        sentinel("/v2", "axios", "src/b.ts", 2),
        http_consume("GET /users", Some("axios"), "src/c.ts", 10),
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes.len(), 1);
    assert_eq!(consumes[0].key.as_deref(), Some("GET /users")); // unchanged — never guess
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("axios"));
    assert!(warnings[0].contains("/api"));
    assert!(warnings[0].contains("src/a.ts:1"));
    assert!(warnings[0].contains("/v2"));
    assert!(warnings[0].contains("src/b.ts:2"));
    assert!(consumes.iter().all(|c| c.kind != "client-base-prefix"));
}

#[test]
fn duplicate_sentinels_with_the_same_path_apply_once_with_no_warning() {
    let mut consumes = vec![
        sentinel("/api", "axios", "src/a.ts", 1),
        sentinel("/api", "axios", "src/b.ts", 2),
        http_consume("GET /users", Some("axios"), "src/c.ts", 10),
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes.len(), 1);
    assert_eq!(consumes[0].key.as_deref(), Some("GET /api/users"));
    assert!(warnings.is_empty());
}

#[test]
fn non_http_kind_with_axios_tag_is_untouched() {
    // Shouldn't exist in practice (only `http` consumes carry a client tag today), but the
    // gate must be on `kind`, not merely on the client tag being present.
    let mut consumes = vec![
        sentinel("/api", "axios", "src/a.ts", 1),
        IoConsume {
            client: Some("axios".to_string()),
            body: None,
            kind: "trpc".to_string(),
            key: Some("GET users".to_string()),
            file: "src/c.ts".to_string(),
            line: 1,
            raw: None,
            method: None,
        },
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes.len(), 1);
    assert_eq!(consumes[0].key.as_deref(), Some("GET users"));
}

#[test]
fn prefix_already_present_in_the_path_is_still_prepended() {
    // Deliberate: the axios runtime really does double it (`baseURL: '/api'` + a call site
    // that itself already resolves to `/api/users` -> the wire request really goes to
    // `/api/api/users`). Pins the semantic rather than trying to detect/dedupe it.
    let mut consumes = vec![
        sentinel("/api", "axios", "src/a.ts", 1),
        http_consume("GET /api/users", Some("axios"), "src/c.ts", 10),
    ];
    let mut warnings = Vec::new();
    apply_client_base_prefixes(&mut consumes, &mut warnings);
    assert_eq!(consumes[0].key.as_deref(), Some("GET /api/api/users"));
}
