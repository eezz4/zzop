//! HTTP interface-key normalization — the canonical keying both sides of the cross-layer join must
//! produce. PINNED: [`http_interface_key`] / [`http_consume_interface_key`] are mirrored by
//! `docs/adapters/key-normalization.fixture.json`, the `key_normalization_fixture` parity test, and
//! the examples adapter-kit JS port — their logic must not change here without changing every mirror.

/// The verb-shaped-NAME vocabulary: every place a cross-layer HTTP verb is inferred from an
/// identifier's NAME — a member callee (`axios.get`, Angular `this.http.get`), a computed-member
/// string literal (`axios['post']`), a hono `$get`-style terminal, an Express-style `.get(path, h)`
/// registration, a Spring `@GetMapping` annotation — draws from this ONE set (single definition;
/// per-vocabulary spellings stay at their call sites but are pinned to this const by tests, so a verb
/// added here is added everywhere deliberately, never by drift). Deliberately NOT a filter on keys
/// overall: extractors that read the verb from an EXPLICIT attribute (`fetch(url, { method: 'HEAD' })`,
/// Spring `method = RequestMethod.HEAD`) pass their literal through verbatim — an explicit spelling is
/// a visible fact, while a name-shaped match outside this set would be a guess.
pub const HTTP_KEY_VERBS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH"];

/// The canonical `http` interface key both sides must produce so the join is exact.
/// Path params (`{x}` or `:x`) -> `{}`; duplicate slashes collapsed; trailing slash dropped; method upper-cased.
/// Keeping this in core (not per-adapter) guarantees an FE-emitted key and a BE-emitted key are byte-identical.
pub fn http_interface_key(method: &str, raw_path: &str) -> String {
    format!(
        "{} {}",
        method.to_uppercase(),
        normalize_http_path(raw_path)
    )
}

/// The PATH-only half of [`http_interface_key`]'s normalization: path params (`{x}` or `:x`) -> `{}`,
/// duplicate slashes collapsed, trailing slash dropped, leading slash ensured. Exposed so a caller
/// composing a PATH PREFIX (not a full `"METHOD path"` key) — e.g. the engine's router-mount compose
/// pass building an `EntityRef::PathScope` prefix — can normalize onto the exact same param vocabulary
/// an `http_interface_key`-keyed route path uses, without duplicating this regex logic. A raw prefix
/// like `/users/:id/admin` built by joining an Express `.use(':id', ...)` chain otherwise never covers
/// the normalized route key (`/users/{}/admin/...`), since [`crate::attributes::AttributeStore::route_attr`]'s
/// `path_under` compares literally.
pub fn normalize_http_path(raw_path: &str) -> String {
    let with_slash = format!("/{raw_path}");
    let collapsed = re_multi_slash().replace_all(&with_slash, "/");
    let params = re_param().replace_all(&collapsed, "{}");
    re_trailing().replace(&params, "$1").into_owned()
}

/// [`http_interface_key`] for a CONSUME-side URL: drops the query/fragment suffix (`?...`/`#...`)
/// before normalization. A call-site URL's `?` is always a query separator (`axios.get('articles?limit=10')`,
/// `` `articles?${qs}` `` -> `articles?{}`), and a route provide's key never carries one, so a
/// query-suffixed consume key is structurally guaranteed to miss the exact join AND the near-miss
/// segment comparison. Provide-side keying must NOT use this: in a route PATTERN a `?` is not a query
/// separator (e.g. Spring's `?` single-character wildcard), so provides keep [`http_interface_key`].
pub fn http_consume_interface_key(method: &str, raw_url: &str) -> String {
    let path = raw_url.split(['?', '#']).next().unwrap_or(raw_url);
    http_interface_key(method, path)
}

fn re_multi_slash() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"/+").unwrap())
}
fn re_param() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"\{[^}]+\}|:[A-Za-z_][A-Za-z0-9_]*").unwrap())
}
fn re_trailing() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"(.)/+$").unwrap())
}

#[cfg(test)]
mod tests {
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
}
