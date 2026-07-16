//! The `case`/`prefix` structural-dimension matchers for `cross-layer/route-near-miss`, plus the
//! `path_near_miss`-disjointness guard — pure predicates over path segments, split out of the root
//! module (which owns candidate selection, findings, and the near-miss cross-reference output).

use super::super::{path_segments, split_key, HttpProvideSite};

/// The one structural dimension a route-near-miss pair differs by, in priority order (`case` > `prefix`,
/// most to least confident — see module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dimension {
    Case,
    Prefix,
}

impl Dimension {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Dimension::Case => "case",
            Dimension::Prefix => "prefix",
        }
    }
}

/// Whether `consume_segs`/`provide_segs` form a pair `path_near_miss` already owns: same segment count,
/// every segment either exactly equal or `{}` on one side (pure parameter generalization). Used to keep this
/// rule strictly disjoint from `path_near_miss`, even though — see the module doc — neither dimension below
/// can actually produce such a pair; kept as an explicit guard so that invariant is checked, not just
/// reasoned about.
pub(super) fn is_path_near_miss_pair(consume_segs: &[&str], provide_segs: &[&str]) -> bool {
    consume_segs.len() == provide_segs.len()
        && consume_segs
            .iter()
            .zip(provide_segs.iter())
            .all(|(cs, ps)| cs == ps || *cs == "{}" || *ps == "{}")
}

/// `case` dimension: same segment count, every segment equal case-insensitively, and at least one segment
/// differs case-sensitively (otherwise the pair is an exact match, which cannot appear among unprovided
/// consumes in the first place). A segment that is `{}` on one side can never satisfy
/// case-insensitive-equality against a literal segment, so this can never overlap with `path_near_miss`'s
/// parameter-generalization case.
pub(super) fn case_dimension_match(consume_segs: &[&str], provide_segs: &[&str]) -> bool {
    if consume_segs.len() != provide_segs.len() {
        return false;
    }
    let mut any_case_diff = false;
    for (cs, ps) in consume_segs.iter().zip(provide_segs.iter()) {
        if cs.to_lowercase() != ps.to_lowercase() {
            return false;
        }
        if cs != ps {
            any_case_diff = true;
        }
    }
    any_case_diff
}

/// `prefix` dimension: the shorter path's segments are an exact (case-sensitive, `{}`-included) suffix of
/// the longer path's segments, and the leading run of segments added/removed is 1 or 2 AND all-literal (no
/// `{}`). Two guards against unrelated-route false matches: a real base prefix like `/api` or `/api/v1` is
/// short (1-2 segments), and a `{}` parameter is never a base path — `GET /articles` vs `GET /{}/articles`
/// must NOT fire, so the leading diff run must contain no `{}` (the shared suffix may still contain `{}`, as
/// an exact segment match). Returns the 1/2 leading segments (the "prefix") on a match, so the caller can
/// report exactly what differs.
pub(super) fn prefix_dimension_match<'a>(
    consume_segs: &[&'a str],
    provide_segs: &[&'a str],
) -> Option<Vec<&'a str>> {
    let (shorter, longer) = if consume_segs.len() <= provide_segs.len() {
        (consume_segs, provide_segs)
    } else {
        (provide_segs, consume_segs)
    };
    let diff = longer.len() - shorter.len();
    if diff == 0 || diff > 2 || shorter.is_empty() {
        return None;
    }
    if longer[diff..] != *shorter {
        return None;
    }
    let leading = &longer[..diff];
    // The added/removed leading run must be an all-literal base path; a `{}` parameter is not a prefix.
    if leading.contains(&"{}") {
        return None;
    }
    Some(leading.to_vec())
}

pub(super) fn provide_path_segs(p: &HttpProvideSite) -> Option<(&str, Vec<&str>)> {
    let (pmethod, ppath) = split_key(&p.key)?;
    Some((pmethod, path_segments(ppath)))
}
