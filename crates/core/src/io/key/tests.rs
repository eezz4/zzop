//! [`super`]'s tests. Split out on 2026-09-07 when `key.rs` crossed the 300-line cap — the repo's
//! stated remedy for a source file is a directory module, and for a file whose overflow is its own test
//! module the pair is `foo.rs` + `foo/tests.rs` (the cap exists to keep SOURCE units small; tests are
//! exempt by policy and may grow).

use super::*;

#[test]
fn http_key_normalizes_params_slashes_and_method() {
    assert_eq!(
        http_interface_key("get", "authen/getUserInfo"),
        "GET /authen/getUserInfo"
    );
    assert_eq!(http_interface_key("get", "/users/{id}"), "GET /users/{}");
    assert_eq!(http_interface_key("get", "/users/:id"), "GET /users/{}");
    // duplicate slashes collapsed, trailing slash dropped, method upper-cased
    assert_eq!(http_interface_key("post", "//a//b/"), "POST /a/b");
    // root path preserved (single slash, no trailing-drop)
    assert_eq!(http_interface_key("get", ""), "GET /");
}

/// A router suffix bound to a `:param` — an inline regex constraint or an optional marker — is part
/// of the PARAM, not of the path, so it must not reach the key.
///
/// 🔴 Both survived until 2026-09-07 (review ledger V34) and each cost a join outright: a caller
/// spells `/posts/7`, which keys to `GET /posts/{}`, and the route keyed to `GET /posts/{}?` — an
/// exact-match join can never bridge that. Measured on a constructed pair, two of five misses were
/// exactly this, and the reader's only clue was a key with a stray character in it.
#[test]
fn a_router_suffix_on_a_param_does_not_reach_the_key() {
    // Express/Koa optional param.
    assert_eq!(http_interface_key("get", "/posts/:id?"), "GET /posts/{}");
    // Express/Koa inline regex constraint.
    assert_eq!(
        http_interface_key("get", "/items/:n(\\d+)"),
        "GET /items/{}"
    );
    // Both at once, and mid-path rather than trailing.
    assert_eq!(http_interface_key("get", "/a/:n([0-9]+)?/b"), "GET /a/{}/b");
    // The whole point: the suffixed route and the plain one are now ONE key, so a consume that
    // spells the param (an interpolated `` `/posts/${id}` `` -> `GET /posts/{}`) joins either.
    assert_eq!(
        http_interface_key("get", "/posts/:id?"),
        http_interface_key("get", "/posts/:id")
    );
    assert_eq!(
        http_interface_key("get", "/posts/:id?"),
        http_consume_interface_key("get", "/posts/{id}")
    );
    // ⚠ NOT claimed: a LITERAL consume (`/posts/7`) still does not key to `{}`. That is a separate
    // gap by design — never-guess keeps them apart and `cross-layer/path-near-miss` reports the
    // pairing instead of inventing an edge.
    assert_ne!(
        http_interface_key("get", "/posts/:id?"),
        http_consume_interface_key("get", "/posts/7")
    );
}

/// The counter-case that pins WHY the suffixes are bound to the colon arm rather than eaten anywhere.
///
/// In a route PATTERN a bare `?` is Spring's single-character wildcard — a real path character, and
/// [`http_consume_interface_key`]'s doc already records that provides must keep it while consumes
/// strip it as a query separator. A normalizer that ate `?` unconditionally would silently merge two
/// different Spring routes, which is the wrong-edge class this file spends the most care on.
#[test]
fn a_bare_question_mark_is_a_spring_wildcard_and_survives() {
    assert_eq!(http_interface_key("get", "/files/a?c"), "GET /files/a?c");
    assert_ne!(
        http_interface_key("get", "/files/a?c"),
        http_interface_key("get", "/files/ac")
    );
    // A consume, by contrast, treats `?` as the query separator — the documented asymmetry.
    assert_eq!(
        http_consume_interface_key("get", "/files/a?c=1"),
        "GET /files/a"
    );
}

#[test]
fn colon_is_a_param_sigil_only_when_the_segment_has_no_content_before_it() {
    // AIP / gRPC-Gateway / Buf custom methods: the token after `:` is a SUFFIX on a segment that
    // already has content, and it is the most identity-bearing part of the path. Eating it made
    // `:activate` and `:deactivate` the SAME key — a false `http/duplicate-route` plus a WRONG
    // cross-layer edge (a consume of one joined to the provide of the other).
    assert_eq!(
        http_interface_key("post", "/v1/users/{id}:activate"),
        "POST /v1/users/{}:activate"
    );
    assert_eq!(
        http_interface_key("post", "/v1/users/{id}:deactivate"),
        "POST /v1/users/{}:deactivate"
    );
    assert_ne!(
        http_interface_key("post", "/v1/users/{id}:activate"),
        http_interface_key("post", "/v1/users/{id}:deactivate")
    );
    // Custom method directly on a collection (AIP-231 `:batchGet`) — content is a plain literal.
    assert_eq!(
        http_interface_key("post", "/v1/users:batchGet"),
        "POST /v1/users:batchGet"
    );
    assert_eq!(http_interface_key("get", "/a/b:c"), "GET /a/b:c");
    // The consume side must reach the same key, including after `${id}` -> `{}` interpolation
    // normalization upstream (`{}` is not a `{x}` param, so the suffix survives there too).
    assert_eq!(
        http_consume_interface_key("post", "/v1/users/{}:activate?force=1"),
        "POST /v1/users/{}:activate"
    );

    // Express/Rails/NestJS `:param` — the segment has NO content before the colon. Unchanged.
    assert_eq!(http_interface_key("get", "/users/:id"), "GET /users/{}");
    // Express's documented multi-param segment spellings, where the param follows an in-segment
    // separator rather than a `/`: `/flights/:from-:to` and `/plantae/:genus.:species`.
    assert_eq!(
        http_interface_key("get", "/flights/:from-:to"),
        "GET /flights/{}-{}"
    );
    assert_eq!(
        http_interface_key("get", "/plantae/:genus.:species"),
        "GET /plantae/{}.{}"
    );
    // Rails' `.:format` suffix, and a brace param followed by a separator-led param.
    assert_eq!(
        http_interface_key("get", "/posts/:id.:format"),
        "GET /posts/{}.{}"
    );
    assert_eq!(
        http_interface_key("get", "/users/{id}-:rev"),
        "GET /users/{}-{}"
    );
}

#[test]
fn http_consume_key_drops_query_and_fragment_suffix() {
    // Literal query — the fe-axios RealWorld corpus shape (`axios.get('articles?limit=10')`)
    // whose keyed form could never join `GET /articles` (dogfood round 6, 2026-07-10).
    assert_eq!(
        http_consume_interface_key("get", "articles?limit=10"),
        "GET /articles"
    );
    // Interpolated query is normalized to `?{}` upstream — same drop.
    assert_eq!(
        http_consume_interface_key("get", "/articles?{}"),
        "GET /articles"
    );
    // Query after a path param, and fragment suffix.
    assert_eq!(
        http_consume_interface_key("get", "/articles/{slug}?include=author"),
        "GET /articles/{}"
    );
    assert_eq!(
        http_consume_interface_key("get", "/docs#anchor"),
        "GET /docs"
    );
    // No suffix -> identical to http_interface_key.
    assert_eq!(
        http_consume_interface_key("post", "//a//b/"),
        http_interface_key("post", "//a//b/")
    );
    // Query-only URL degrades to the root path (the egress extractor vetoes this shape
    // earlier — see `base_relative_path` — so it only arises from an explicit `/?x=1`).
    assert_eq!(http_consume_interface_key("get", "/?page=2"), "GET /");
}

#[test]
fn key_carries_route_identity_rejects_only_all_placeholder_paths() {
    // The head-drop artifact this gate exists for: an unresolved `${BASE}` prefix leaves a key
    // that names no endpoint (mono-hub `joke-generator/fetchJoke.ts`, 2026-07-25).
    assert!(!key_carries_route_identity("GET /{}"));
    assert!(!key_carries_route_identity("POST /{}/{}"));
    assert!(!key_carries_route_identity("GET /{}/{}/{}"));
    // One literal segment anywhere is enough — the key can still be joined/compared.
    assert!(key_carries_route_identity("GET /api/{}"));
    assert!(key_carries_route_identity("GET /{}/users"));
    assert!(key_carries_route_identity("DELETE /users/{}"));
    // A root route is fully known, not a lost target.
    assert!(key_carries_route_identity("GET /"));
    // Non-"VERB /path" shapes are outside the `{}` vocabulary and always pass.
    assert!(key_carries_route_identity("table:users"));
    assert!(key_carries_route_identity("{}"));
    // Absolute-URL keys never reach this gate in the linker (the `://` egress gate fires first),
    // but the predicate is total: a host makes the key non-all-placeholder anyway.
    assert!(key_carries_route_identity("GET https://api.example.com/{}"));
}

#[test]
fn db_table_channel_casing_lower_firsts_only_the_first_character() {
    // PascalCase model name -> Prisma client accessor casing (the shape this transform exists for).
    assert_eq!(db_table_channel_casing("Article"), "article");
    assert_eq!(db_table_channel_casing("UserProfile"), "userProfile");
    // Already-lowercase / snake_case DDL names are a no-op (the common hand-written-SQL case).
    assert_eq!(db_table_channel_casing("users"), "users");
    assert_eq!(db_table_channel_casing("article_tags"), "article_tags");
    // Single character and empty string are edge cases both call sites can hit after quote-stripping.
    assert_eq!(db_table_channel_casing("A"), "a");
    assert_eq!(db_table_channel_casing(""), "");
}
