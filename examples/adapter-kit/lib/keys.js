// Byte-exact port of zzop_core's HTTP interface-key normalization (packages/core/src/io.rs,
// `http_interface_key` / `http_consume_interface_key`) plus the consume-side veto list a call-site
// URL must clear before it is safe to key at all (parser/parser-typescript/src/adapters/egress.rs,
// `consume_key_for` / `base_relative_path` / `is_external`).
//
// Cross-layer linking is an EXACT string join on `key` — there is no fuzzy fallback. An adapter (in
// any language) that computes this key even slightly differently than the engine does will silently
// fail to join: the consume just lands in `unprovidedConsumes` instead of forming an edge.
// `docs/adapters/key-normalization.fixture.json` is the parity contract for the two normalize*
// functions below, generated from the real Rust functions; `test/keys.test.js` replays every row
// against them. If a row fails, fix this file — never the fixture.

const RE_MULTI_SLASH = /\/+/g;
const RE_PARAM = /\{[^}]+\}|:[A-Za-z_][A-Za-z0-9_]*/g;
const RE_TRAILING = /(.)\/+$/;

/**
 * The canonical `http` PROVIDE-side interface key. Path params (`{x}` or `:x`) collapse to `{}`;
 * duplicate slashes collapse; a trailing slash is dropped; the method upper-cases. A `?...` suffix is
 * deliberately NOT stripped here — in a route PATTERN `?` is not always a query separator (e.g. a
 * single-character wildcard), so it is data, not noise. See `normalizeConsumeKey` for the consume-side
 * asymmetry.
 *
 * Exact port of `zzop_core::http_interface_key` (packages/core/src/io.rs).
 */
export function normalizeProvideKey(method, rawPath) {
  const withSlash = `/${rawPath}`;
  const collapsed = withSlash.replace(RE_MULTI_SLASH, '/');
  const params = collapsed.replace(RE_PARAM, '{}');
  const trimmed = params.replace(RE_TRAILING, '$1');
  return `${method.toUpperCase()} ${trimmed}`;
}

/**
 * `normalizeProvideKey` for a CONSUME-side call-site URL: drops a `?...`/`#...` suffix before
 * normalizing. A call-site URL's `?` is always a query separator, and a provide's key never carries
 * one, so an un-stripped consume key is structurally guaranteed to miss the exact join. Provide-side
 * keying must NOT do this (see `normalizeProvideKey`'s doc).
 *
 * Exact port of `zzop_core::http_consume_interface_key` (packages/core/src/io.rs).
 */
export function normalizeConsumeKey(method, rawUrl) {
  const path = rawUrl.split(/[?#]/)[0];
  return normalizeProvideKey(method, path);
}

/**
 * True when `url` carries an explicit scheme this kit treats as third-party egress — matches
 * `is_external` in `parser/parser-typescript/src/adapters/egress.rs`. Only `http://`/`https://` count;
 * anything else (`ws://`, a bare `://`-containing string with no recognized scheme) falls through to
 * the base-relative check in `resolveConsumeKey`.
 */
export function isExternalUrl(url) {
  const lower = url.toLowerCase();
  return lower.startsWith('http://') || lower.startsWith('https://');
}

/**
 * Classifies a raw call-site URL literal that is NEITHER root-internal (`/...`) NOR external
 * (`http(s)://...`) as a "base-relative" path — the axios/ky `baseURL` idiom (`users/login`), whose
 * host is invisible at the call site. Returns the ROOT-NORMALIZED path (`/users/login`) when the
 * literal clears every veto condition below, or `null` when it does not (never guessed).
 *
 * NOT base-relative — vetoed to `null`: an empty string, a leading-interpolation template (`{...` —
 * the base itself is the expression), a document-relative `./`/`../` path, a query-only URL
 * (`?page=2` — "same path, new query", which names no path at all), any scheme-carrying string (`://`
 * anywhere), or whitespace-carrying text (not a path).
 *
 * Ported from `base_relative_path` in `parser/parser-typescript/src/adapters/egress.rs`. Unlike
 * `normalizeProvideKey`/`normalizeConsumeKey` above, this veto list is NOT covered by the
 * key-normalization parity fixture (that fixture only exercises the two normalize* functions) —
 * treat this as a best-effort mirror, and re-check against the Rust source if the two drift.
 */
export function baseRelativePath(url) {
  if (
    url.length === 0 ||
    url.startsWith('/') ||
    url.startsWith('.') ||
    url.startsWith('{') ||
    url.startsWith('?') ||
    url.includes('://') ||
    /\s/.test(url)
  ) {
    return null;
  }
  return `/${url}`;
}

/**
 * The full consume-side key resolution an adapter needs for one call-site URL: internal (leading
 * `/`) keys directly via `normalizeConsumeKey`; external (`http(s)://`) keys verbatim — never run
 * through normalization, which would mangle the origin; base-relative resolves via
 * `baseRelativePath` first; anything else is unresolved (`null`) — reported, never guessed.
 *
 * This is the exact veto-list + dispatch shape every hand-rolled JS adapter in this repo
 * (wrapper-adapter, react-query-adapter) re-derives slightly differently inline — use this instead of
 * re-deriving it.
 *
 * Ported from `consume_key_for` in `parser/parser-typescript/src/adapters/egress.rs`.
 */
export function resolveConsumeKey(method, url) {
  if (url.startsWith('/')) {
    return normalizeConsumeKey(method, url);
  }
  if (isExternalUrl(url)) {
    return `${method.toUpperCase()} ${url}`;
  }
  const rooted = baseRelativePath(url);
  return rooted === null ? null : normalizeConsumeKey(method, rooted);
}
