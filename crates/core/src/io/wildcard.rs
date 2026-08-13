//! ANT wildcard route patterns — the ONE place that answers "is this route key a PATTERN rather than
//! an exact key, and does it cover this consume key?".
//!
//! ## What was wrong before
//! A Spring `@GetMapping("/files/**")` normalizes through [`normalize_http_path`](super::normalize_http_path)
//! UNCHANGED — `**` matches no path-param arm, so the key `GET /api/files/**` arrives in the join with the
//! `*`s intact. Both sides of the join therefore already SPELL it identically; what was missing was a reader.
//! Read as a literal segment, one wildcard route produced THREE wrong answers from one cause (measured
//! 2026-08-13, `@GetMapping("/files/**")` plus two calls beneath it):
//!
//! ```text
//! unconsumedProvides : GET /api/files/**            <- a live catch-all called a dead route
//! unprovidedConsumes : GET /api/files/a/b/c
//!                      GET /api/files/img/logo.png  <- served calls called missing routes
//! ```
//!
//! ## The fix is a PARTITION, not a fallback join
//! [`link_cross_layer_io`](super::link_cross_layer_io) is an EXACT join, and that is its contract. A
//! prefix/longest-match fallback would make edges out of a guess and would have to invent a tiebreak when
//! two catch-alls both cover a call. So a wildcard route is lifted OUT of the join entirely: it emits no
//! edge, it is never `unconsumedProvides` (an exact-key consume for a pattern can never exist, so its
//! absence proves nothing), and a consume it covers is dropped from `unprovidedConsumes` (the route DOES
//! serve that call — only the exact-key machinery cannot say so). Edge count is unchanged BY CONSTRUCTION;
//! the payoff is three false findings removed, and the removal is disclosed rather than silent —
//! `CrossLayerResult::wildcard_route_partitions` carries every partitioned route to the engine, which
//! self-reports it on the owning tree's own `warnings` channel.
//!
//! ## Why NOT a new key sentinel
//! [`UNKNOWN_VERB`](super::UNKNOWN_VERB) needed a sentinel because "method unknown" had no spelling at all
//! (adapters were fabricating `[GET, POST]`). A wildcard already has one and it survives normalization
//! byte-for-byte, so re-spelling it would break the byte-pinned `docs/adapters/key-normalization.fixture.json`,
//! the adapter-kit JS port and the MCP `contract` resource for nothing.

/// If `key` is an http route key (`"VERB /path"`) whose PATH carries an ANT wildcard (`*`), returns its
/// path; otherwise `None`. Mirrors [`unknown_verb_route_path`](super::unknown_verb_route_path)'s shape —
/// the partition's reader, asked of PROVIDES only.
///
/// `*` alone is the admission marker, deliberately NOT Spring's single-character `?` wildcard. `?` is
/// overloaded: it is the query separator [`http_consume_interface_key`](super::http_consume_interface_key)
/// strips, and admitting it would let one malformed provide key quietly swallow calls. Narrower means a
/// `?`-only route keeps producing today's wrong answers — an honest under-fix, not a hidden one.
pub fn wildcard_route_path(key: &str) -> Option<&str> {
    key.split_once(' ')
        .and_then(|(_method, path)| path.contains('*').then_some(path))
}

/// Does the wildcard route `route_key` cover the consume `consume_key`? Both are full
/// [`http_interface_key`](super::http_interface_key) keys.
///
/// The VERB must match exactly. A `@GetMapping("/files/**")` serves GET and nothing else, so a
/// `POST /api/files/x` call is genuinely unprovided and must keep firing — the partition suppresses the
/// calls the route really answers, never every call that shares its path space. `route_key` that carries no
/// wildcard is always `false` (it belongs to the exact join, which already answered).
///
/// The ONE definition both axes ask, exactly like
/// [`key_carries_route_identity`](super::key_carries_route_identity): the multi-tree linker asks it of a
/// join MISS, and the single-tree `http/unprovided-consume` rule asks it of its own miss. Those two
/// answering differently about the same fact is the defect class `link::consume_join`'s module doc was
/// written about.
pub fn wildcard_route_covers(route_key: &str, consume_key: &str) -> bool {
    let Some(pattern) = wildcard_route_path(route_key) else {
        return false;
    };
    let (Some((route_verb, _)), Some((consume_verb, consume_path))) =
        (route_key.split_once(' '), consume_key.split_once(' '))
    else {
        return false;
    };
    route_verb == consume_verb && ant_path_matches(pattern, consume_path)
}

/// Spring `AntPathMatcher`-style match of an ANT `pattern` against a concrete `path`, segment-based:
/// `**` matches zero or more whole segments (so `/articles/**` matches `/articles` AND `/articles/{}`),
/// `*` matches exactly one segment, a literal segment matches itself. A route path's `{}` param
/// placeholder is an ordinary segment (a literal pattern segment won't equal it; `*`/`**` will).
///
/// Lifted out of `zzop_parser_java_21::spring_security` (2026-08-13) when the join partition became its
/// second caller — it is one algorithm, and a second hand-copy is how the two would drift.
///
/// **The permissive arms in [`seg_glob`] err toward MATCH, and the two callers read that bias in OPPOSITE
/// directions** — which is why it is documented here once instead of assumed at each site. For Spring
/// security, matching a `permitAll` pattern means the route is NOT exempted from the open-route finding, so
/// erring toward match errs toward FIRING. For the join partition, matching means the call is served, so
/// erring toward match errs toward SUPPRESSING. Both land where this analysis wants to land — a route with
/// an in-segment glob is a pattern either way, and the partition's generosity is bounded to routes that
/// already carry a `*` and is disclosed per route. A future reader tempted to "fix" the bias for one caller
/// must therefore check the other.
pub fn ant_path_matches(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.trim_start_matches('/').split('/').collect();
    let s: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    seg_match(&p, &s)
}

fn seg_match(pat: &[&str], seg: &[&str]) -> bool {
    match pat.split_first() {
        None => seg.is_empty(),
        Some((&"**", rest)) => {
            // `**` consumes zero or more segments — a trailing `**` matches any remainder (incl. none).
            rest.is_empty() || (0..=seg.len()).any(|k| seg_match(rest, &seg[k..]))
        }
        Some((&ph, prest)) => match seg.split_first() {
            Some((&sh, srest)) if seg_glob(ph, sh) => seg_match(prest, srest),
            _ => false,
        },
    }
}

/// Single-segment match: `*` matches any one segment; a path-variable segment (`{id}`, and the normalized
/// `{}` an [`http_interface_key`](super::http_interface_key) route path carries) matches any one segment
/// too — crucially reconciling the two mirror halves, which normalize path variables differently, so a
/// `permitAll("/users/{id}")` is not under-matched against `/users/{}`; a within-segment glob
/// (`feed*`/`user?`, rare) is treated permissively as a match; else a literal must be equal. Every
/// non-exact case errs toward MATCH — see [`ant_path_matches`] for what that means for each caller.
fn seg_glob(pat: &str, seg: &str) -> bool {
    pat == "*"
        || pat == seg
        || (pat.starts_with('{') && pat.ends_with('}'))
        || pat.contains('*')
        || pat.contains('?')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_route_path_admits_star_only_and_only_on_a_verb_keyed_path() {
        assert_eq!(
            wildcard_route_path("GET /api/files/**"),
            Some("/api/files/**")
        );
        assert_eq!(
            wildcard_route_path("GET /files/*/thumb"),
            Some("/files/*/thumb")
        );
        // Exact keys, param keys and the verb-unknown sentinel are all NOT wildcard routes.
        assert_eq!(wildcard_route_path("GET /api/files"), None);
        assert_eq!(wildcard_route_path("GET /users/{}"), None);
        assert_eq!(wildcard_route_path("? /api/files"), None);
        // `?` is deliberately not admitted (see this fn's doc — it is the consume side's query separator).
        assert_eq!(wildcard_route_path("GET /file?"), None);
        // Non-"VERB /path" shapes (a topic, a table) are outside this vocabulary.
        assert_eq!(wildcard_route_path("table:users"), None);
    }

    #[test]
    fn a_wildcard_route_covers_the_calls_beneath_it_but_only_on_its_own_verb() {
        // The measured defect: two live calls under one `@GetMapping("/files/**")`.
        assert!(wildcard_route_covers(
            "GET /api/files/**",
            "GET /api/files/a/b/c"
        ));
        assert!(wildcard_route_covers(
            "GET /api/files/**",
            "GET /api/files/img/logo.png"
        ));
        // `**` matches zero segments too — the bare prefix is served.
        assert!(wildcard_route_covers("GET /api/files/**", "GET /api/files"));
        // VERB discrimination: the route serves GET only, so a POST under it is genuinely unprovided.
        assert!(!wildcard_route_covers(
            "GET /api/files/**",
            "POST /api/files/a"
        ));
        // Outside the pattern's path space.
        assert!(!wildcard_route_covers(
            "GET /api/files/**",
            "GET /api/users"
        ));
        // A non-wildcard route covers nothing — the exact join already answered for it.
        assert!(!wildcard_route_covers("GET /api/files", "GET /api/files"));
    }

    #[test]
    fn single_star_matches_exactly_one_segment_while_double_star_matches_any_depth() {
        assert!(wildcard_route_covers("GET /files/*", "GET /files/a"));
        assert!(!wildcard_route_covers("GET /files/*", "GET /files/a/b"));
        assert!(wildcard_route_covers("GET /files/**", "GET /files/a/b"));
        // A mid-path `**` still has to match the tail literally.
        assert!(wildcard_route_covers(
            "GET /files/**/meta",
            "GET /files/a/b/meta"
        ));
        assert!(!wildcard_route_covers(
            "GET /files/**/meta",
            "GET /files/a/b/data"
        ));
    }

    #[test]
    fn a_normalized_param_segment_matches_a_concrete_one_in_either_direction() {
        // `http_interface_key` normalizes `{id}` to `{}` on BOTH sides, so a wildcard route carrying a
        // param must still cover a concrete call and an interpolated one alike.
        assert!(wildcard_route_covers(
            "GET /users/{}/files/**",
            "GET /users/7/files/a"
        ));
        assert!(wildcard_route_covers(
            "GET /users/{}/files/**",
            "GET /users/{}/files/a"
        ));
    }

    #[test]
    fn a_root_catch_all_covers_every_call_on_its_verb_and_that_is_the_truth_not_a_bug() {
        // `GET /**` really is served by that handler; the partition discloses it per route rather than
        // pretending the calls beneath it are missing routes.
        assert!(wildcard_route_covers("GET /**", "GET /anything/at/all"));
        assert!(!wildcard_route_covers("GET /**", "DELETE /anything"));
    }
}
