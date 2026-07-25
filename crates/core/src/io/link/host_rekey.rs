//! Deployment-topology host re-key — the transform [`super::classify_consume_join`] runs BEFORE its
//! `"://"` external-egress gate, split out of `link.rs` to keep that file under the 300-line source cap.
//! Behavior is byte-identical to the in-place version; only its address changed.
//!
//! DELIBERATELY not re-exported past `crate::io::link`: it is half of a gate sequence whose ORDER is the
//! contract, and every caller that ever wanted it (the linker, the single-tree `http/unprovided-consume`
//! rule) wants the whole sequence. Handing out the transform alone re-opens the "predicate copied without
//! its transform" defect class from the other side — see [`super::consume_join`]'s module doc.

use super::super::key::http_consume_interface_key;

/// Attempts to re-key an absolute-URL consume key (`"METHOD http(s)://authority/rest..."`) against
/// `internal_hosts` — see [`LinkOptions::internal_hosts`]'s doc for the exact matching rule. Returns
/// `Some((rekeyed_key, matched_host))` on a hit (`matched_host` is the literal entry from
/// `internal_hosts` that matched, for [`CrossLayerResult::host_rekey_counts`] bookkeeping); `None` when
/// the key isn't a `"METHOD scheme://..."` shape, the scheme isn't `http`/`https`, or the authority
/// matches no declared host — the caller falls through to the ordinary external-egress gate untouched.
///
/// Reached only through [`super::classify_consume_join`], which is what the single-tree
/// `http/unprovided-consume` rule (another crate) calls — that rule must apply this SAME transform before
/// its own `://` veto or it answers differently than this join does about one fact. It used to hand-copy
/// the body; sharing the whole sequence closes that drift by construction rather than by convention.
///
/// [`LinkOptions::internal_hosts`]: super::LinkOptions::internal_hosts
/// [`CrossLayerResult::host_rekey_counts`]: crate::io::CrossLayerResult::host_rekey_counts
pub fn rekey_if_internal_host(key: &str, internal_hosts: &[String]) -> Option<(String, String)> {
    let (method, rest) = key.split_once(' ')?;
    let scheme_end = rest.find("://")?;
    let scheme = &rest[..scheme_end];
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None; // ws/wss (and anything else) stay external in v1
    }
    let after_scheme = &rest[scheme_end + 3..];
    let (authority, path) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => (after_scheme, "/"),
    };
    let authority_host = authority.split(':').next().unwrap_or(authority);
    for declared in internal_hosts {
        let matched = match declared.split_once(':') {
            // Declared host carries an explicit port — the consume must match host:port exactly.
            Some((decl_host, decl_port)) => match authority.split_once(':') {
                Some((host, port)) => host.eq_ignore_ascii_case(decl_host) && port == decl_port,
                None => false,
            },
            // Declared host carries no port — the consume side's port (if any) is ignored.
            None => authority_host.eq_ignore_ascii_case(declared),
        };
        if matched {
            return Some((http_consume_interface_key(method, path), declared.clone()));
        }
    }
    None
}
