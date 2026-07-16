//! URL-variant -> interface-key classification: internal, external (host-carrying), base-relative,
//! and the base-carrier head-drop bucket — see [`consume_key_for`]'s doc for the full decision order.

use zzop_core::http_consume_interface_key;

/// Classify one resolved URL variant against one method into its interface key, or `None` if the URL
/// veto-lists out of all three buckets. Internal calls join to a same-repo BE route; external calls get
/// a host-carrying key ("METHOD https://host/path", left verbatim rather than run through
/// `http_interface_key`, which would mangle the origin) so `link_cross_layer_io`'s `"://"` gate routes
/// them external. A base-relative literal (`users/login` — the axios `baseURL` idiom) keys as its
/// root-normalized path; see `base_relative_path`'s doc for the exact veto list.
///
/// Fourth bucket, checked after the three above: **base-carrier head-drop**. A template/concat that
/// assembles to a single leading `{}` immediately followed by a `/`-headed literal (`"{}/me/achievements"`
/// from `` fetch(`${BASE_URL}/me/achievements`) ``) has an opaque, cross-file-invisible base carried by
/// that one interpolation slot — mirroring the base-relative decision above, the base is DROPPED, never
/// valued, and the visible `/`-headed literal keys the call. This is a narrow near-miss prefix class: it
/// only fires when exactly one `{}` leads and what follows starts with `/` but not `//` (a second `{}`
/// right after the head, a non-`/` literal suffix, or a `//` post-drop head all stay unresolved — see
/// `base_relative_path`'s doc for why those shapes are never-guess residue).
pub(super) fn consume_key_for(method: &str, url: &str) -> Option<String> {
    if url.starts_with('/') {
        Some(http_consume_interface_key(method, url))
    } else if is_external(url) {
        Some(format!("{} {}", method.to_uppercase(), url))
    } else if let Some(rest) = url
        .strip_prefix("{}")
        .filter(|rest| rest.starts_with('/') && !rest.starts_with("//"))
    {
        Some(http_consume_interface_key(method, rest))
    } else {
        base_relative_path(url).map(|rooted| http_consume_interface_key(method, &rooted))
    }
}

fn is_external(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

/// Public form of the internal/external classification, exported so `zzop-engine`'s late cross-file
/// resolution can apply the same gating (leading `/` = internal, `http(s)://` = external verbatim key,
/// base-relative = root-normalized internal, else unresolved) instead of force-keying every resolved
/// constant as internal.
pub fn is_external_url(u: &str) -> bool {
    is_external(u)
}

/// A base-relative path literal (`users/login`, `articles?limit=10`) — the standard axios/ky idiom of a
/// path resolved against a configured `baseURL` whose value is invisible at the call site (cross-file
/// config or env). Returns the ROOT-NORMALIZED path (`/users/login`): exact when the base carries no
/// path segment, and one prefix dimension off — absorbed by `cross-layer/route-near-miss`'s
/// "missing path prefix" explanation — when it does (decision: cross-layer-resolution, 2026-07-10,
/// pulled by the connected-pair dogfood where 19/19 recognized axios calls fell unresolved).
///
/// NOT base-relative — `None`, left unresolved, never guessed: an empty string, a leading-interpolation
/// template (`{}`-headed — the base itself is the expression), a document-relative `./`/`../` path, a
/// query-only URL (`?page=2` — "same path, new query", which names no path at all), any
/// scheme-carrying URL (`ws://` etc.; `http(s)://` is already the external branch), or
/// whitespace-carrying text (not a path). The `{}`-headed veto here is still correct for the general case
/// (the base itself is an opaque expression with no visible path), but ONE sub-shape of it — exactly one
/// leading `{}` immediately followed by a `/`-headed literal — is keyed upstream in `consume_key_for`
/// (base-carrier head-drop) before this function ever sees it; the veto here still catches everything
/// that shape excludes (`{}{}...`, `{}non-slash`, `{}//host/...`).
pub fn base_relative_path(u: &str) -> Option<String> {
    if u.is_empty()
        || u.starts_with('/')
        || u.starts_with('.')
        || u.starts_with('{')
        || u.starts_with('?')
        || u.contains("://")
        || u.contains(char::is_whitespace)
    {
        return None;
    }
    Some(format!("/{u}"))
}

#[cfg(test)]
mod tests {
    use crate::adapters::egress::{extract_http_egress, files, keys};

    // --- base-relative path literals (`base-relative-egress-v1`) ---

    #[test]
    fn base_relative_literal_keys_as_its_root_normalized_path() {
        // The axios `baseURL` idiom: paths with no leading slash, resolved against a configured base
        // invisible at the call site. Keyed root-normalized; a base prefix (`/api`) is then a
        // route-near-miss "missing path prefix" away, not an unexplained unresolved.
        let out = extract_http_egress(&files(&[(
            "conduit.ts",
            "axios.post('users/login', body); axios.get(`articles/${slug}/comments`); axios.get('articles?limit=10');",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("POST /users/login".to_string()),
                Some("GET /articles/{}/comments".to_string()),
                // Query suffix dropped (`query-drop-v1`) — a route provide never carries one, so
                // `GET /articles?limit=10` could never join `GET /articles` nor be explained by
                // route-near-miss's segment comparison (dogfood round 6: 2/19 corpus consumes
                // were query-suffixed and silently unexplainable).
                Some("GET /articles".to_string()),
            ]
        );
        assert!(out.iter().all(|c| c.raw.is_none()));
    }

    #[test]
    fn interpolated_query_suffix_is_dropped_from_the_key() {
        // `` `articles?${qs}` `` resolves to `articles?{}` — the query drop must also cover the
        // interpolated form, on both rooted and base-relative paths.
        let out = extract_http_egress(&files(&[(
            "q.ts",
            "axios.get(`articles?${qs}`); axios.get(`/articles/feed?${qs}`);",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /articles".to_string()),
                Some("GET /articles/feed".to_string()),
            ]
        );
    }

    #[test]
    fn base_relative_veto_list_still_never_keys() {
        // Leading-interpolation template (the base itself is the expression), document-relative `./`,
        // query-only URL, non-http scheme, whitespace — all stay unresolved with raw+method carried. The
        // first case, `` `${API_ROOT}${url}` ``, assembles to `"{}{}"` — a SECOND `{}` immediately after
        // the head, which is dynamic too (no literal path), so `consume_key_for`'s base-carrier head-drop
        // bucket ("{}"-head + "/"-headed remainder) does not fire here either; it stays unresolved.
        let out = extract_http_egress(&files(&[(
            "v.ts",
            "axios.get(`${API_ROOT}${url}`); axios.get('./users'); axios.get('?page=2'); axios.get('ws://host/ch'); axios.get('not a path');",
        )]));
        assert_eq!(out.len(), 5);
        assert!(
            out.iter().all(|c| c.key.is_none() && c.method.is_some()),
            "got: {:?}",
            keys(&out)
        );
    }

    // --- base-carrier head-drop (`base-carrier-drop-v1`) ---

    #[test]
    fn base_carrier_head_drop_root_ping_keys_as_get_root() {
        // Boundary pin (accepted, not an accident): `` fetch(`${API}/`) `` assembles to `"{}/"` and
        // keys as `GET /` — the least-specific key. Same visible-fact contract as a literal
        // `fetch("/")` (which also keys `GET /`): the call demonstrably targets the base root.
        // The base-carries-a-path-prefix risk is the documented head-drop trade-off (near-miss
        // prefix class absorbs it); vetoing only the root form would be inconsistent with the
        // literal root-relative case.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`${API}/`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /"));
    }

    #[test]
    fn base_carrier_head_drop_keys_the_visible_path_from_a_template() {
        // The liberation-shaped driving case: `` fetch(`${BASE_URL}/me/achievements`) `` assembles to
        // `"{}/me/achievements"` — one opaque leading `{}` (the base, invisible cross-file) followed by a
        // `/`-headed literal. The base is dropped, never valued; the visible literal keys the call.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`${BASE_URL}/me/achievements`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /me/achievements"));
        assert!(out[0].raw.is_none());
    }

    #[test]
    fn base_carrier_head_drop_keeps_inner_interpolation_slots() {
        // A second `{}` NOT immediately after the head is an ordinary path-param slot, not a second
        // dynamic base — existing template-param semantics apply past the dropped head.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${base}/users/${id}`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /users/{}"));
    }

    #[test]
    fn base_carrier_head_drop_still_drops_the_query_suffix() {
        // Query-drop (`query-drop-v1`) composes with head-drop: the base is dropped AND the query is
        // dropped, leaving just the route path.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`${B}/articles?limit=10`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /articles"));
    }

    #[test]
    fn base_carrier_head_drop_refuses_second_dynamic_head() {
        // `` `${a}${b}` `` assembles to `"{}{}"` — the piece right after the head is dynamic too (no
        // literal path at all), so this never-guesses rather than keying on nothing.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${a}${b}`)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("`${a}${b}`"));
    }

    #[test]
    fn base_carrier_head_drop_refuses_non_slash_suffix() {
        // `` `${base}users` `` assembles to `"{}users"` — a non-`/`-headed literal suffix means the
        // segment boundary is invisible (could be a mid-segment concat like `base + "users"`), so it never
        // keys.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${base}users`)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("`${base}users`"));
    }

    #[test]
    fn base_carrier_head_drop_refuses_protocol_relative_host() {
        // `` `${proto}//example.com/x` `` assembles to `"{}//example.com/x"` — the post-drop `//` means
        // the next piece is a HOST (protocol-relative URL), not a path, so it never keys as internal.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.get(`${proto}//example.com/x`)")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("`${proto}//example.com/x`"));
    }

    #[test]
    fn base_carrier_head_drop_does_not_disturb_literal_external_head() {
        // A literal `https://`-headed template is checked in the external branch BEFORE the head-drop
        // branch and is unaffected by it — mirrors the pinned expectation in
        // `absolute_url_becomes_a_host_carrying_key_for_the_external_bucket`.
        let out = extract_http_egress(&files(&[("a.tsx", "fetch(`https://api.ext.com/${p}`)")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET https://api.ext.com/{}"));
        assert!(out[0].raw.is_none());
    }
}
