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
/// Path params (`{x}`, or `:x` where the colon starts a segment or follows an in-segment separator —
/// see [`re_param`]) -> `{}`; a Google-AIP custom-method suffix (`{id}:activate`) is KEPT, since it
/// identifies the endpoint; duplicate slashes collapsed; trailing slash dropped; method upper-cased.
/// Keeping this in core (not per-adapter) guarantees an FE-emitted key and a BE-emitted key are byte-identical.
pub fn http_interface_key(method: &str, raw_path: &str) -> String {
    format!(
        "{} {}",
        method.to_uppercase(),
        normalize_http_path(raw_path)
    )
}

/// Sentinel method for a route whose PATH is statically known but whose HTTP method(s) are NOT — a
/// `pages/api` serve-all handler, a pathname-dispatch / Go `HandleFunc` block that names no method
/// literal. Replaces the former per-adapter `[GET, POST]` fabrication (three lockstep constants +
/// their pins) with one honest marker: at assemble time the engine PARTITIONS these out of the
/// exact-key join into a path-level "served, verb-unknown" set (see `analyze::assemble`), which
/// suppresses consume-side false unprovided/near-miss findings on that path and drives the
/// `cross-layer/unknown-verb-route` disclosure. NEVER a real verb — a char no HTTP method spells, so
/// a sentinel key `"? /x"` can never collide with a genuine [`http_interface_key`] key.
pub const UNKNOWN_VERB: &str = "?";

/// If `key` is an [`UNKNOWN_VERB`] sentinel `http` key (`"? <path>"`), returns its normalized PATH;
/// otherwise `None`. Used by the assemble-time partition that lifts verb-unknown routes out of the
/// exact-key join into the path-level served-set.
pub fn unknown_verb_route_path(key: &str) -> Option<&str> {
    key.split_once(' ')
        .and_then(|(method, path)| (method == UNKNOWN_VERB).then_some(path))
}

/// The PATH-only half of [`http_interface_key`]'s normalization: path params (`{x}` or a
/// separator-led `:x` — see [`re_param`]) -> `{}`, duplicate slashes collapsed, trailing slash
/// dropped, leading slash ensured. Exposed so a caller
/// composing a PATH PREFIX (not a full `"METHOD path"` key) — e.g. the engine's router-mount compose
/// pass building an `EntityRef::PathScope` prefix — can normalize onto the exact same param vocabulary
/// an `http_interface_key`-keyed route path uses, without duplicating this regex logic. A raw prefix
/// like `/users/:id/admin` built by joining an Express `.use(':id', ...)` chain otherwise never covers
/// the normalized route key (`/users/{}/admin/...`), since [`crate::attributes::AttributeStore::route_attr`]'s
/// `path_under` compares literally.
pub fn normalize_http_path(raw_path: &str) -> String {
    let with_slash = format!("/{raw_path}");
    let collapsed = re_multi_slash().replace_all(&with_slash, "/");
    // `${1}` is the separator the colon arm had to consume to express "preceded by"; it expands to
    // the empty string on the brace arm, which captures nothing.
    let params = re_param().replace_all(&collapsed, "${1}{}");
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

/// Does this consume key name a route at all — the shared minimum-information gate.
///
/// False for a `"VERB /path"` key whose path segments are ALL `{}` placeholders (`GET /{}`,
/// `POST /{}/{}`): that is the head-drop artifact of an interpolation the extractor could not resolve
/// (`` fetch(`${BASE}/${id}`) `` with `BASE` unresolved), so it picks out no particular endpoint and
/// carries ZERO joinable evidence. True otherwise, including two shapes that look adjacent but are not:
/// - a root key (`GET /`, zero segments) — the path IS fully known, it just has no segments;
/// - a key with no `"VERB /path"` shape at all (`table:users`, a topic, an env key) — the `{}`
///   placeholder vocabulary is [`normalize_http_path`]'s alone, so this gate has nothing to say there
///   and an unrecognized shape is never treated as evidence of anything.
///
/// Single definition for every surface that must answer this ONE question identically: the linker's
/// route-identity gate ([`link_cross_layer_io`](crate::io::link_cross_layer_io) — a miss on such a key
/// goes to `unresolvedConsumes`, not `unprovidedConsumes`), the single-tree `http/unprovided-consume`
/// rule's matching veto, and the engine's extraction-blindness caveat (such a key is not visibility
/// evidence). Those three used to answer it in three hand-written copies with three slightly different
/// edge cases; sharing it makes the drift a compile-time impossibility.
///
/// NOT the same question as the near-miss rules' `is_all_slot_path` (rules-cross-layer), which also
/// treats a SEGMENTLESS path as vacuous — it asks "is a suggestion computed from this key meaningless",
/// where `GET /` genuinely resembles nothing. Deliberately kept apart.
pub fn key_carries_route_identity(key: &str) -> bool {
    let Some((_verb, path)) = key.split_once(' ') else {
        return true; // not a "VERB /path" key — out of this gate's vocabulary
    };
    let mut segments = path.split('/').filter(|s| !s.is_empty()).peekable();
    if segments.peek().is_none() {
        return true; // "VERB /" — a real root route, not a lost target
    }
    segments.any(|s| s != "{}")
}

/// The `db-table` PROVIDE channel's canonical casing: the FIRST character lowercased, everything else
/// unchanged (e.g. `Article` -> `article`, `UserProfile` -> `userProfile`). PINNED shared transform for
/// two independent extractors that must agree byte-for-byte so a `CREATE TABLE "Article"` DDL name and a
/// Prisma `model Article` name key the SAME physical table: `zzop_parser_sql::extract::bare_table_name`
/// (applied as the LAST step, after its quote-aware unquoted-dot schema-qualifier split and quote strip)
/// and `zzop_parser_prisma::analysis::accessor_casing`. Those two parser crates cannot depend on each
/// other (parser isolation), so this lives here in `zzop-core`, which both already depend on, instead of
/// being a local twin in either — a real cross-parser dependency edge on ONE 3-line transform is not
/// worth the isolation violation, but leaving it un-shared (prose-only agreement) is worse: this makes
/// drift a compile-time impossibility instead of a convention.
pub fn db_table_channel_casing(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn re_multi_slash() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"/+").unwrap())
}
/// The path-param vocabulary. Two arms, and the SECOND one is position-sensitive on purpose.
///
/// `\{[^}]+\}` — the brace spelling (`{id}`, Spring's `{id:[0-9]+}`, gRPC-Gateway's `{name=x/*}`).
/// Unconditional: a brace param is unambiguous wherever it appears.
///
/// `([/.-]):[A-Za-z_][A-Za-z0-9_]*` — the colon spelling, admitted ONLY when the character before the
/// `:` is a path or in-segment separator, and that separator is CAPTURED and put back (`${1}{}`) so
/// consecutive params still each match (`:from-:to` -> the `-` is not consumed by the first match).
/// Rust's regex has no lookbehind, so consume-and-restore is the way to express "preceded by".
///
/// The separator condition is the whole point. `:` is overloaded across the ecosystems zzop keys:
/// - Express/Rails/NestJS: `:id` is a PARAM, and it always starts a segment (`/users/:id`) or follows
///   an in-segment separator — Express documents exactly `/flights/:from-:to` and
///   `/plantae/:genus.:species`, Rails documents `.:format`. Hence the `[/.-]` class, no wider.
/// - Google-AIP / gRPC-Gateway / Buf (the Go and Java server convention): `:` is the CUSTOM-METHOD
///   separator and it is a SUFFIX on a segment that already has content (`/v1/users/{id}:activate`,
///   `/v1/users:batchGet`). The token after it is the most identity-bearing part of the path.
///
/// Collapsing an AIP custom method keyed `:activate` and `:deactivate` IDENTICALLY — three wrong
/// answers from one cause: `http/duplicate-route` false-fired on two distinct endpoints; the exact
/// join linked a consume of one to the provide of the OTHER (a WRONG edge, worse than silence, which
/// also hid the true unconsumed/unprovided pair); and near-miss suggestions off the key were nonsense.
/// The path-parameter vocabulary: a `{brace}` param, or a `:colon` param with the two suffixes a router
/// may attach to it — an inline regex constraint (`:n(\\d+)`) and an optional marker (`:id?`).
///
/// 🔴 Both suffixes used to survive into the key (2026-09-07, review ledger V34), and a surviving suffix
/// is not a cosmetic blemish: the key is the JOIN's only identity, so `GET /posts/{}?` and
/// `GET /items/{}(d+)` could never match the `GET /posts/{}` / `GET /items/{}` a caller spells. Two of
/// five measured exact-key misses on a constructed pair were this, and the only signal a reader got was
/// a visibly malformed key with nothing naming the cause.
///
/// ⚠ The suffixes are bound to the colon arm ON PURPOSE. A bare `?` elsewhere in a route PATTERN is
/// Spring's single-character wildcard, which is a real path character — [`http_consume_interface_key`]'s
/// doc states that asymmetry, and eating `?` anywhere would break it. Anchoring both suffixes to a
/// matched `:param` keeps this change to the routers that actually spell them (Express/Koa).
fn re_param() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        regex::Regex::new(r"\{[^}]+\}|([/.-]):[A-Za-z_][A-Za-z0-9_]*(?:\([^)]*\))?\??").unwrap()
    })
}
fn re_trailing() -> &'static regex::Regex {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"(.)/+$").unwrap())
}

#[cfg(test)]
mod tests;
